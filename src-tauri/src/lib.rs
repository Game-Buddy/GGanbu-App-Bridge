pub mod actions;
pub mod config;
pub mod keyboard;
#[cfg(test)]
mod opaque_spike;
pub mod protocol;
pub mod security;
pub mod server;
pub mod state;

use chrono::{DateTime, Duration, Utc};
use rand::{Rng, RngCore, rngs::OsRng};
use serde::Serialize;

use std::{
    fs,
    net::SocketAddr,
    path::{Path, PathBuf},
    sync::{
        Arc, Mutex, OnceLock,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use rust_embed::RustEmbed;
use security::{PairingSession, SecurityState};
use state::{BRIDGE_STATE_EVENT, BridgeSnapshot, ServerStatus, SharedBridgeState, StatePublisher};
use tauri::{Emitter, LogicalSize, Manager};
use tokio_util::sync::CancellationToken;

const SERVER_ADDRESS: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 53177);
const DEFAULT_KEYBINDINGS: &str = "config/hotkeys/hotkey.keyboard_shooter_ver3.blkx";
const MIN_WINDOW_WIDTH: f64 = 960.0;
const MIN_WINDOW_HEIGHT: f64 = 540.0;

#[derive(RustEmbed)]
#[folder = "assets/"]
struct EmbeddedAssets;

static ASSET_ROOT: OnceLock<PathBuf> = OnceLock::new();

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct KeybindingPreview {
    source: String,
    output: String,
    files: Vec<String>,
    bindings: usize,
    hotkeys: Vec<KeybindingRow>,
}
#[derive(Clone, Debug, Serialize)]
struct KeybindingRow {
    action: String,
    binding: String,
}

fn parse_blk_value(raw: &str) -> serde_json::Value {
    let value = raw.trim();
    if value.len() >= 2 && value.starts_with('"') && value.ends_with('"') {
        return serde_json::Value::String(value[1..value.len() - 1].replace("\\\"", "\""));
    }
    value
        .parse::<i64>()
        .map(serde_json::Value::from)
        .unwrap_or_else(|_| serde_json::Value::String(value.to_owned()))
}

fn insert_blk_value(
    object: &mut serde_json::Map<String, serde_json::Value>,
    key: String,
    value: serde_json::Value,
) {
    match object.remove(&key) {
        None => {
            object.insert(key, value);
        }
        Some(serde_json::Value::Array(mut values)) => {
            values.push(value);
            object.insert(key, serde_json::Value::Array(values));
        }
        Some(previous) => {
            object.insert(key, serde_json::Value::Array(vec![previous, value]));
        }
    }
}

fn parse_blk(source: &str) -> Result<serde_json::Value, String> {
    let mut stack: Vec<(String, serde_json::Map<String, serde_json::Value>)> =
        vec![(String::new(), serde_json::Map::new())];
    for raw_line in source.lines() {
        let line = raw_line.trim();
        if line.is_empty() || line.starts_with("//") {
            continue;
        }
        if let Some(stripped) = line.strip_suffix("{}") {
            let key = stripped.trim().to_owned();
            insert_blk_value(
                &mut stack.last_mut().ok_or("invalid block")?.1,
                key,
                serde_json::json!({}),
            );
        } else if let Some(stripped) = line.strip_suffix('{') {
            let key = stripped.trim().to_owned();
            stack.push((key, serde_json::Map::new()));
        } else if line == "}" {
            let (key, child) = stack.pop().ok_or("unexpected closing brace")?;
            insert_blk_value(
                &mut stack.last_mut().ok_or("unexpected block end")?.1,
                key,
                serde_json::Value::Object(child),
            );
        } else if let Some((left, raw_value)) = line.split_once('=') {
            let key = left
                .trim()
                .split(':')
                .next()
                .unwrap_or(left.trim())
                .to_owned();
            insert_blk_value(
                &mut stack.last_mut().ok_or("invalid value")?.1,
                key,
                parse_blk_value(raw_value),
            );
        }
    }
    if stack.len() != 1 {
        return Err("unterminated .blk block".to_owned());
    }
    Ok(serde_json::Value::Object(stack.pop().unwrap().1))
}

fn resolve_case_insensitive(path: &Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return Ok(path.to_path_buf());
    }

    let file_name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::NotFound,
            format!("path '{}' does not exist", path.display()),
        )
    })?;
    let parent = path.parent().unwrap_or_else(|| Path::new("."));
    let resolved_parent = resolve_case_insensitive(parent)?;
    let wanted = file_name.to_string_lossy();
    let entry = fs::read_dir(&resolved_parent)?
        .filter_map(Result::ok)
        .find(|entry| {
            entry
                .file_name()
                .to_string_lossy()
                .eq_ignore_ascii_case(&wanted)
        })
        .ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                format!("path '{}' does not exist", path.display()),
            )
        })?;
    Ok(entry.path())
}

fn load_keybinding_chain(
    path: &Path,
    files: &mut Vec<String>,
    seen: &mut Vec<PathBuf>,
    merged: &mut serde_json::Map<String, serde_json::Value>,
) -> Result<(), String> {
    let path = resolve_case_insensitive(path)
        .and_then(fs::canonicalize)
        .map_err(|e| format!("could not read '{}': {e}", path.display()))?;
    if seen.contains(&path) {
        return Err(format!(
            "cyclic basePresetPaths reference at '{}'",
            path.display()
        ));
    }
    seen.push(path.clone());
    let source = fs::read_to_string(&path).map_err(|e| e.to_string())?;
    let document = if path
        .extension()
        .is_some_and(|extension| extension.eq_ignore_ascii_case("blk"))
    {
        parse_blk(&source).map_err(|e| format!("invalid .blk file '{}': {e}", path.display()))?
    } else {
        serde_json::from_str(&source)
            .map_err(|e| format!("invalid JSON in '{}': {e}", path.display()))?
    };
    let controls = document
        .get("controls")
        .ok_or("mapping file is missing controls")?;
    if let Some(bases) = controls
        .get("basePresetPaths")
        .and_then(serde_json::Value::as_object)
    {
        for base in bases.values().filter_map(serde_json::Value::as_str) {
            let mut parent = if base.starts_with("config/") {
                let asset_root = path
                    .ancestors()
                    .find(|candidate| candidate.file_name().is_some_and(|name| name == "assets"))
                    .map(Path::to_path_buf)
                    .unwrap_or_else(|| {
                        ASSET_ROOT.get().cloned().unwrap_or_else(|| {
                            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("assets")
                        })
                    });
                asset_root.join(base)
            } else {
                path.parent().unwrap_or(Path::new(".")).join(base)
            };
            // War Thunder presets sometimes reference the legacy .blk suffix while
            // the distributed asset is stored as JSON with a .blkx suffix.
            if resolve_case_insensitive(&parent).is_err()
                && parent
                    .extension()
                    .is_some_and(|extension| extension.eq_ignore_ascii_case("blk"))
            {
                parent.set_extension("blkx");
            }
            load_keybinding_chain(&parent, files, seen, merged)?;
        }
    }
    if let Some(hotkeys) = controls
        .get("hotkeys")
        .and_then(serde_json::Value::as_object)
    {
        for (key, value) in hotkeys {
            merged.insert(key.clone(), value.clone());
        }
    }
    files.push(path.display().to_string());
    seen.pop();
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn resolves_hotkey_assets_without_case_sensitive_path_matching() {
        let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("ASSETS/CONFIG/HOTKEYS/HOTKEY.KEYBOARD_SHOOTER_VER3.BLKX");
        let resolved = resolve_case_insensitive(&path).unwrap();
        let expected = fs::canonicalize(
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("assets/config/hotkeys/hotkey.keyboard_shooter_ver3.blkx"),
        )
        .unwrap();

        assert_eq!(fs::canonicalize(resolved).unwrap(), expected);
    }

    #[test]
    fn parses_a_single_quote_without_panicking() {
        assert_eq!(
            parse_blk_value("\""),
            serde_json::Value::String("\"".into())
        );
    }
}

#[tauri::command]
async fn pick_keybindings_file() -> Option<String> {
    rfd::AsyncFileDialog::new()
        .add_filter("War Thunder keybindings", &["blk", "blkx"])
        .pick_file()
        .await
        .map(|file| file.path().to_string_lossy().into_owned())
}

#[tauri::command]
fn load_keybindings(
    app: tauri::AppHandle,
    path: Option<String>,
) -> Result<KeybindingPreview, String> {
    let embedded_root = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("assets");
    for asset in EmbeddedAssets::iter() {
        let target = embedded_root.join(asset.as_ref());
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        if !target.exists()
            && let Some(contents) = EmbeddedAssets::get(asset.as_ref())
        {
            fs::write(&target, contents.data.as_ref()).map_err(|e| e.to_string())?;
        }
    }
    let _ = ASSET_ROOT.set(embedded_root.clone());
    let source = match path {
        Some(path) => {
            let selected = PathBuf::from(path);
            if !selected.extension().is_some_and(|extension| {
                extension.eq_ignore_ascii_case("blk") || extension.eq_ignore_ascii_case("blkx")
            }) {
                return Err("only .blk and .blkx files can be selected".to_owned());
            }
            selected
        }
        None => {
            let packaged = app
                .path()
                .resource_dir()
                .map_err(|e| e.to_string())?
                .join("assets")
                .join(DEFAULT_KEYBINDINGS);
            if packaged.exists() {
                packaged
            } else {
                embedded_root.join(DEFAULT_KEYBINDINGS)
            }
        }
    };
    let mut files = Vec::new();
    let mut seen = Vec::new();
    let mut hotkeys = serde_json::Map::new();
    load_keybinding_chain(&source, &mut files, &mut seen, &mut hotkeys)?;
    let output = app
        .path()
        .app_data_dir()
        .map_err(|e| e.to_string())?
        .join("user-keybindings.json");
    if let Some(parent) = output.parent() {
        fs::create_dir_all(parent).map_err(|e| e.to_string())?;
    }
    let result =
        serde_json::json!({"controls":{"version":5,"hotkeys":hotkeys,"basePresetPaths":{}}});
    fs::write(
        &output,
        serde_json::to_vec_pretty(&result).map_err(|e| e.to_string())?,
    )
    .map_err(|e| e.to_string())?;
    actions::set_user_catalog(&serde_json::to_string(&result).map_err(|e| e.to_string())?)?;
    let rows = hotkeys
        .iter()
        .map(|(action, value)| {
            let binding = value.get("keyboardKey").or_else(|| {
                value
                    .as_array()
                    .and_then(|a| a.iter().find(|entry| entry.get("keyboardKey").is_some()))
                    .and_then(|v| v.get("keyboardKey"))
            });
            let display = binding.and_then(|binding| {
                let codes = match binding {
                    serde_json::Value::Number(number) => {
                        vec![number.as_u64()? as u16]
                    }
                    serde_json::Value::Array(values) => values
                        .iter()
                        .filter_map(serde_json::Value::as_u64)
                        .map(|code| code as u16)
                        .collect::<Vec<_>>(),
                    _ => return None,
                };
                Some(actions::display_key_codes(&codes))
            });
            KeybindingRow {
                action: action.clone(),
                binding: display.unwrap_or_else(|| "—".to_owned()),
            }
        })
        .collect();
    Ok(KeybindingPreview {
        source: source.display().to_string(),
        output: output.display().to_string(),
        files,
        bindings: hotkeys.len(),
        hotkeys: rows,
    })
}

struct ServerControl {
    cancellation: Mutex<CancellationToken>,
    started: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
}

fn finish_server_generation(
    started: &AtomicBool,
    current_generation: &AtomicU64,
    generation: u64,
) -> bool {
    if current_generation.load(Ordering::Acquire) != generation {
        return false;
    }
    started.store(false, Ordering::Release);
    true
}

#[tauri::command]
fn start_server(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
    control: tauri::State<'_, ServerControl>,
) -> Result<(), String> {
    if control.started.swap(true, Ordering::AcqRel) {
        return Ok(());
    }
    if !security.is_persistent() {
        control.started.store(false, Ordering::Release);
        return Err("security storage is unavailable".to_owned());
    }
    let app_data_dir = match app.path().app_data_dir() {
        Ok(directory) => directory,
        Err(error) => {
            control.started.store(false, Ordering::Release);
            return Err(error.to_string());
        }
    };
    let persisted_bindings = app_data_dir.join("user-keybindings.json");
    if persisted_bindings.exists() {
        let document = fs::read_to_string(&persisted_bindings).map_err(|error| {
            control.started.store(false, Ordering::Release);
            format!("could not read user keybindings: {error}")
        })?;
        actions::set_user_catalog(&document).map_err(|error| {
            control.started.store(false, Ordering::Release);
            format!("could not load user keybindings: {error}")
        })?;
    }
    let publisher: StatePublisher = Arc::new(move |snapshot| {
        let _ = app.emit(BRIDGE_STATE_EVENT, snapshot);
    });
    let origins = match config::read_allowed_origins() {
        Ok(origins) => origins,
        Err(error) => {
            control.started.store(false, Ordering::Release);
            return Err(error.to_string());
        }
    };
    bridge.set_allowed_origins(origins.values().to_vec(), &publisher);
    let router = server::build_router_with_execution_and_security(
        origins,
        bridge.inner().clone(),
        publisher.clone(),
        keyboard::KeyboardExecution::system(),
        security.inner().clone(),
    );
    let task_bridge = bridge.inner().clone();
    let cancellation = {
        let mut current = control
            .cancellation
            .lock()
            .expect("server control lock poisoned");
        *current = CancellationToken::new();
        current.clone()
    };
    let generation = control.generation.fetch_add(1, Ordering::AcqRel) + 1;
    let started = control.started.clone();
    let current_generation = control.generation.clone();
    tauri::async_runtime::spawn(async move {
        let result =
            server::serve_on_address(SERVER_ADDRESS, router, task_bridge, publisher, cancellation)
                .await;
        finish_server_generation(&started, &current_generation, generation);
        if let Err(error) = result {
            tracing::warn!(%error, "bridge server task stopped");
        }
    });
    Ok(())
}

#[tauri::command]
fn stop_server(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
    control: tauri::State<'_, ServerControl>,
) {
    security.clear_pairing_and_code();
    publish_security_snapshot(&app, &bridge, &security);
    let _ = app.emit("pairing-status-changed", pairing_status(&security));
    control.started.store(false, Ordering::Release);
    control.generation.fetch_add(1, Ordering::AcqRel);
    control
        .cancellation
        .lock()
        .expect("server control lock poisoned")
        .cancel();
}

#[tauri::command]
fn get_bridge_snapshot(state: tauri::State<'_, SharedBridgeState>) -> BridgeSnapshot {
    state.snapshot()
}

#[tauri::command]
fn get_host_address() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .ok()
        .and_then(|socket| {
            socket.connect("8.8.8.8:80").ok()?;
            socket.local_addr().ok()
        })
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gganbu_bridge=info,tower_http=info".into()),
        )
        .try_init()
        .ok();

    let bridge = SharedBridgeState::new(Vec::new());
    let cancellation = CancellationToken::new();

    let setup_bridge = bridge.clone();
    let app = tauri::Builder::default()
        .manage(bridge)
        .manage(ServerControl {
            cancellation: Mutex::new(cancellation),
            started: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
        })
        .invoke_handler(tauri::generate_handler![
            get_bridge_snapshot,
            start_pairing,
            cancel_pairing,
            get_pairing_status,
            remove_device,
            rename_device,
            get_host_address,
            start_server,
            stop_server,
            pick_keybindings_file,
            load_keybindings
        ])
        .setup(move |app| {
            let dotenv_error = if config::is_release_build() {
                None
            } else {
                config::load_executable_dotenv(app.handle()).err()
            };

            if let Some(window) = app.get_webview_window("main") {
                window
                    .set_min_size(Some(LogicalSize::new(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)))
                    .expect("failed to set minimum window size");
                let icon = tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png"))
                    .expect("failed to load application icon");
                window
                    .set_icon(icon)
                    .expect("failed to set application icon");
            }

            let app_handle = app.handle().clone();
            let publisher: StatePublisher = Arc::new(move |snapshot| {
                if let Err(error) = app_handle.emit(BRIDGE_STATE_EVENT, snapshot) {
                    tracing::warn!(%error, "could not publish bridge state update");
                }
            });

            let security_path = app.path().app_data_dir()?.join("security.json");
            let security = match SecurityState::load(security::DeviceStore::new(security_path)) {
                Ok(security) => security,
                Err(error) => {
                    tracing::error!(%error, "could not load security storage");
                    setup_bridge.set_server_status(
                        ServerStatus::Error,
                        Some(format!("Security storage unavailable: {error}")),
                        &publisher,
                    );
                    SecurityState::default()
                }
            };
            app.manage(security.clone());
            setup_bridge.set_security_snapshot(&security, &publisher);

            if let Some(error) = &dotenv_error {
                setup_bridge.set_server_status(
                    ServerStatus::Error,
                    Some(format!("Invalid .env configuration: {error}")),
                    &publisher,
                );
                return Ok(());
            }

            let origins = match config::read_allowed_origins() {
                Ok(origins) => origins,
                Err(error) => {
                    setup_bridge.set_server_status(
                        ServerStatus::Error,
                        Some(format!("Invalid origin configuration: {error}")),
                        &publisher,
                    );
                    return Ok(());
                }
            };
            setup_bridge.set_allowed_origins(origins.values().to_vec(), &publisher);
            Ok(())
        })
        .build(tauri::generate_context!())
        .expect("failed to build GGanbu Bridge");

    app.run(|app_handle, event| {
        if matches!(event, tauri::RunEvent::ExitRequested { .. }) {
            app_handle
                .state::<ServerControl>()
                .cancellation
                .lock()
                .expect("server control lock poisoned")
                .cancel();
        }
    });
}

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct PairingStatusResponse {
    active: bool,
    code: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    failed_attempts: u8,
}

fn publish_security_snapshot(
    app: &tauri::AppHandle,
    bridge: &SharedBridgeState,
    security: &SecurityState,
) {
    let app = app.clone();
    let publisher: StatePublisher = Arc::new(move |snapshot| {
        let _ = app.emit(BRIDGE_STATE_EVENT, snapshot);
    });
    bridge.set_security_snapshot(security, &publisher);
}

fn pairing_status(security: &SecurityState) -> PairingStatusResponse {
    let Some(session) = security.pairing() else {
        return PairingStatusResponse {
            active: false,
            code: None,
            expires_at: None,
            failed_attempts: 0,
        };
    };
    if session.is_expired(Utc::now()) {
        security.clear_pairing_and_code();
        return PairingStatusResponse {
            active: false,
            code: None,
            expires_at: None,
            failed_attempts: 0,
        };
    }
    PairingStatusResponse {
        active: true,
        code: security.pairing_code(),
        expires_at: Some(session.expires_at),
        failed_attempts: session.failed_attempts,
    }
}

#[tauri::command]
fn start_pairing(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
) -> Result<PairingStatusResponse, String> {
    let mut rng = OsRng;
    let code = format!("{:08}", rng.gen_range(0..100_000_000_u32));
    let pairing_id = format!("{:016x}{:016x}", rng.next_u64(), rng.next_u64());
    let expires_at = Utc::now() + Duration::minutes(1);
    let record = security
        .opaque()
        .create_password_file(code.as_bytes(), pairing_id.as_bytes())
        .map_err(|_| "unable to initialize pairing".to_owned())?;
    let session = PairingSession::new(pairing_id, record, expires_at);
    security
        .start_pairing_with_code(session, code)
        .map_err(|_| "a pairing session is already active".to_owned())?;
    let status = pairing_status(&security);
    publish_security_snapshot(&app, &bridge, &security);
    let _ = app.emit("pairing-status-changed", status.clone());
    Ok(status)
}

#[tauri::command]
fn cancel_pairing(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
) {
    security.clear_pairing_and_code();
    publish_security_snapshot(&app, &bridge, &security);
    let _ = app.emit("pairing-status-changed", pairing_status(&security));
}

#[tauri::command]
fn get_pairing_status(security: tauri::State<'_, SecurityState>) -> PairingStatusResponse {
    pairing_status(&security)
}

#[tauri::command]
fn remove_device(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
    device_id: String,
) -> Result<(), String> {
    if security
        .remove_device(&device_id)
        .map_err(|error| format!("could not save device revocation: {error}"))?
    {
        publish_security_snapshot(&app, &bridge, &security);
        Ok(())
    } else {
        Err("device not found".to_owned())
    }
}

#[tauri::command]
fn rename_device(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
    device_id: String,
    display_name: String,
) -> Result<(), String> {
    let display_name = display_name.trim();
    if display_name.is_empty() || display_name.chars().count() > 80 {
        return Err("device name must contain between 1 and 80 characters".to_owned());
    }
    if security
        .rename_device(&device_id, display_name.to_owned())
        .map_err(|error| format!("could not save device name: {error}"))?
    {
        publish_security_snapshot(&app, &bridge, &security);
        Ok(())
    } else {
        Err("device not found".to_owned())
    }
}

#[cfg(test)]
mod server_control_tests {
    use super::*;

    #[test]
    fn failed_current_generation_allows_restart() {
        let started = AtomicBool::new(true);
        let generation = AtomicU64::new(4);

        assert!(finish_server_generation(&started, &generation, 4));
        assert!(!started.load(Ordering::Acquire));
    }

    #[test]
    fn old_server_completion_cannot_clear_a_new_start() {
        let started = AtomicBool::new(true);
        let generation = AtomicU64::new(5);

        assert!(!finish_server_generation(&started, &generation, 4));
        assert!(started.load(Ordering::Acquire));
    }
}
