// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use tauri::{LogicalSize, Manager};

use crate::{
    commands::{self, bridge::state_publisher, server::ServerControl},
    config,
    security::{self, SecurityState},
    state::{ServerStatus, SharedBridgeState, StatePublisher},
};

const MIN_WINDOW_WIDTH: f64 = 960.0;
const MIN_WINDOW_HEIGHT: f64 = 540.0;

pub(crate) fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::try_from_default_env()
                .unwrap_or_else(|_| "gganbu_app_bridge_lib=info,tower_http=info".into()),
        )
        .try_init()
        .ok();

    let bridge = SharedBridgeState::new(Vec::new());
    let setup_bridge = bridge.clone();
    let app = tauri::Builder::default()
        .manage(bridge)
        .manage(ServerControl::new())
        .invoke_handler(tauri::generate_handler![
            commands::bridge::get_bridge_snapshot,
            commands::bridge::get_host_address,
            commands::pairing::start_pairing,
            commands::pairing::cancel_pairing,
            commands::pairing::get_pairing_status,
            commands::devices::remove_device,
            commands::devices::rename_device,
            commands::server::start_server,
            commands::server::stop_server,
            commands::mappings::pick_keybindings_file,
            commands::mappings::load_default_keybindings,
            commands::mappings::load_saved_keybindings
        ])
        .setup(move |app| {
            let directory = app.path().app_data_dir()?;
            let instance = crate::persistence::lock_instance(&directory)
                .map_err(|error| format!("Another bridge instance is running, or the data directory cannot be locked: {error}"))?;
            app.manage(instance);
            let dotenv_error = if config::is_release_build() {
                None
            } else {
                config::load_executable_dotenv(app.handle()).err()
            };

            if let Some(window) = app.get_webview_window("main") {
                if let Err(error) =
                    window.set_min_size(Some(LogicalSize::new(MIN_WINDOW_WIDTH, MIN_WINDOW_HEIGHT)))
                {
                    tracing::warn!(%error, "could not set minimum window size");
                }
                match tauri::image::Image::from_bytes(include_bytes!("../icons/icon.png")) {
                    Ok(icon) => {
                        if let Err(error) = window.set_icon(icon) {
                            tracing::warn!(%error, "could not set application icon");
                        }
                    }
                    Err(error) => tracing::warn!(%error, "could not load application icon"),
                }
            }

            let publisher: StatePublisher = state_publisher(app.handle());
            let security = match app.path().app_data_dir() {
                Ok(app_data_dir) => {
                    let security_path = app_data_dir.join("security.json");
                    match SecurityState::load(security::DeviceStore::new(security_path)) {
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
                    }
                }
                Err(error) => {
                    tracing::error!(%error, "could not resolve application data directory");
                    setup_bridge.set_server_status(
                        ServerStatus::Error,
                        Some(format!("Application data directory unavailable: {error}")),
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
        .build(tauri::generate_context!());
    let app = match app {
        Ok(app) => app,
        Err(error) => {
            tracing::error!(%error, "could not initialize the bridge");
            rfd::MessageDialog::new()
                .set_title("GGanbu Bridge unavailable")
                .set_description(error.to_string())
                .set_level(rfd::MessageLevel::Error)
                .show();
            return;
        }
    };

    app.run(|app_handle, event| {
        if let tauri::RunEvent::ExitRequested {
            api, code: None, ..
        } = event
        {
            api.prevent_exit();
            app_handle.state::<ServerControl>().cancel();
            let app = app_handle.clone();
            tauri::async_runtime::spawn(async move {
                if let Err(error) = commands::server::stop_server(
                    app.clone(),
                    app.state::<SharedBridgeState>(),
                    app.state::<SecurityState>(),
                    app.state::<ServerControl>(),
                )
                .await
                {
                    tracing::warn!(%error, "bridge shutdown failed");
                }
                app.exit(0);
            });
        }
    });
}
