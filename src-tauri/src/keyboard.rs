// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    fmt,
    sync::{Arc, Mutex},
    thread,
    time::Duration,
};

#[cfg(target_os = "linux")]
use std::sync::mpsc;

const KEY_HOLD_DURATION: Duration = Duration::from_millis(35);
const PRE_EXECUTION_DELAY: Duration = Duration::from_millis(100); // TODO: Change delay
#[cfg(target_os = "linux")]
const WAYLAND_PORTAL_TIMEOUT: Duration = Duration::from_secs(15);

pub trait KeyboardExecutor: Send + Sync {
    fn execute(&self, key_codes: &[u16]) -> Result<(), ExecutionError>;
}

#[derive(Clone)]
pub struct KeyboardExecution {
    executor: Arc<dyn KeyboardExecutor>,
    execution_lock: Arc<Mutex<()>>,
}

impl fmt::Debug for KeyboardExecution {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("KeyboardExecution")
            .finish_non_exhaustive()
    }
}

impl KeyboardExecution {
    pub fn system() -> Self {
        Self {
            executor: system_executor(),
            execution_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn disabled() -> Self {
        Self {
            executor: Arc::new(DisabledExecutor),
            execution_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn with_executor(executor: Arc<dyn KeyboardExecutor>) -> Self {
        Self {
            executor,
            execution_lock: Arc::new(Mutex::new(())),
        }
    }

    pub fn execute(&self, key_codes: &[u16]) -> Result<(), ExecutionError> {
        let _guard = self
            .execution_lock
            .lock()
            .expect("keyboard execution lock poisoned");
        self.executor.execute(key_codes)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ExecutionError {
    message: String,
}

impl ExecutionError {
    pub fn new(message: impl Into<String>) -> Self {
        Self {
            message: message.into(),
        }
    }
}

impl fmt::Display for ExecutionError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.message)
    }
}

impl std::error::Error for ExecutionError {}

#[derive(Debug)]
struct DisabledExecutor;

impl KeyboardExecutor for DisabledExecutor {
    fn execute(&self, _key_codes: &[u16]) -> Result<(), ExecutionError> {
        Err(ExecutionError::new("keyboard execution is disabled"))
    }
}

#[cfg(target_os = "linux")]
fn system_executor() -> Arc<dyn KeyboardExecutor> {
    if is_wayland_session() {
        Arc::new(WaylandPortalKeyboardExecutor::new())
    } else {
        Arc::new(X11KeyboardExecutor::new())
    }
}

#[cfg(target_os = "windows")]
fn system_executor() -> Arc<dyn KeyboardExecutor> {
    Arc::new(WindowsKeyboardExecutor)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
fn system_executor() -> Arc<dyn KeyboardExecutor> {
    Arc::new(UnsupportedKeyboardExecutor)
}

#[cfg(target_os = "windows")]
#[derive(Debug)]
struct WindowsKeyboardExecutor;

#[cfg(target_os = "windows")]
impl KeyboardExecutor for WindowsKeyboardExecutor {
    fn execute(&self, key_codes: &[u16]) -> Result<(), ExecutionError> {
        let keys = key_codes
            .iter()
            .map(|scan_code| windows_scan_code(*scan_code))
            .collect::<Result<Vec<_>, _>>()?;

        thread::sleep(PRE_EXECUTION_DELAY);
        let mut pressed = Vec::with_capacity(keys.len());
        let press_result = (|| {
            for key in &keys {
                send_windows_key(*key, false)?;
                pressed.push(*key);
            }
            thread::sleep(KEY_HOLD_DURATION);
            Ok(())
        })();

        let mut release_error = None;
        for key in pressed.iter().rev() {
            if let Err(error) = send_windows_key(*key, true) {
                release_error.get_or_insert(error);
            }
        }

        press_result?;
        if let Some(error) = release_error {
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(target_os = "windows")]
#[derive(Clone, Copy, Debug)]
struct WindowsScanCode {
    code: u16,
    extended: bool,
}

#[cfg(target_os = "windows")]
fn windows_scan_code(direct_input_code: u16) -> Result<WindowsScanCode, ExecutionError> {
    let code = u8::try_from(direct_input_code).map_err(|_| {
        ExecutionError::new(format!(
            "keyboard scan code {direct_input_code} cannot be represented by Windows"
        ))
    })?;
    let extended = code & 0x80 != 0;
    let code = u16::from(code & 0x7f);
    if code == 0 {
        return Err(ExecutionError::new(format!(
            "keyboard scan code {direct_input_code} is not a valid Windows scan code"
        )));
    }

    Ok(WindowsScanCode { code, extended })
}

#[cfg(target_os = "windows")]
fn send_windows_key(key: WindowsScanCode, released: bool) -> Result<(), ExecutionError> {
    use windows_sys::Win32::UI::Input::KeyboardAndMouse::{
        INPUT, INPUT_0, INPUT_KEYBOARD, KEYBDINPUT, KEYEVENTF_EXTENDEDKEY, KEYEVENTF_KEYUP,
        KEYEVENTF_SCANCODE, SendInput,
    };

    let mut flags = KEYEVENTF_SCANCODE;
    if key.extended {
        flags |= KEYEVENTF_EXTENDEDKEY;
    }
    if released {
        flags |= KEYEVENTF_KEYUP;
    }
    let input = INPUT {
        r#type: INPUT_KEYBOARD,
        Anonymous: INPUT_0 {
            ki: KEYBDINPUT {
                wVk: 0,
                wScan: key.code,
                dwFlags: flags,
                time: 0,
                dwExtraInfo: 0,
            },
        },
    };

    // SAFETY: `input` is a fully initialized keyboard INPUT and remains valid for the call.
    let sent = unsafe { SendInput(1, &input, std::mem::size_of::<INPUT>() as i32) };
    if sent == 1 {
        return Ok(());
    }

    let operation = if released { "release" } else { "press" };
    let os_error = std::io::Error::last_os_error();
    Err(ExecutionError::new(format!(
        "could not {operation} Windows scan code {}: {os_error}; Windows may block input sent to a higher-integrity process",
        key.code
    )))
}

#[cfg(target_os = "linux")]
struct X11KeyboardExecutor {
    connection: Mutex<Option<Arc<x11rb::rust_connection::RustConnection>>>,
}

#[cfg(target_os = "linux")]
impl X11KeyboardExecutor {
    fn new() -> Self {
        Self {
            connection: Mutex::new(None),
        }
    }

    fn connection(&self) -> Result<Arc<x11rb::rust_connection::RustConnection>, ExecutionError> {
        let mut cached = self
            .connection
            .lock()
            .expect("X11 connection lock poisoned");
        if let Some(connection) = cached.as_ref() {
            return Ok(Arc::clone(connection));
        }
        let (connection, _) = x11rb::connect(None).map_err(|error| {
            let session = std::env::var("XDG_SESSION_TYPE").unwrap_or_default();
            if session.eq_ignore_ascii_case("wayland") {
                ExecutionError::new(format!(
                    "could not connect to XWayland through DISPLAY; native Wayland input injection is unsupported: {error}"
                ))
            } else {
                ExecutionError::new(format!(
                    "could not connect to the X11 display; check DISPLAY and XTEST permissions: {error}"
                ))
            }
        })?;
        let connection = Arc::new(connection);
        *cached = Some(Arc::clone(&connection));
        Ok(connection)
    }

    fn invalidate_connection(&self) {
        self.connection
            .lock()
            .expect("X11 connection lock poisoned")
            .take();
    }
}

#[cfg(target_os = "linux")]
fn is_wayland_session() -> bool {
    std::env::var("XDG_SESSION_TYPE").is_ok_and(|session| session.eq_ignore_ascii_case("wayland"))
        || std::env::var_os("WAYLAND_DISPLAY").is_some()
}

#[cfg(target_os = "linux")]
struct WaylandPortalKeyboardExecutor {
    sender: Result<mpsc::Sender<WaylandKeyboardCommand>, String>,
}

#[cfg(target_os = "linux")]
impl fmt::Debug for WaylandPortalKeyboardExecutor {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter
            .debug_struct("WaylandPortalKeyboardExecutor")
            .field("worker_available", &self.sender.is_ok())
            .finish_non_exhaustive()
    }
}

#[cfg(target_os = "linux")]
impl WaylandPortalKeyboardExecutor {
    fn new() -> Self {
        Self {
            sender: start_wayland_portal_worker(),
        }
    }
}

#[cfg(target_os = "linux")]
struct WaylandKeyboardCommand {
    key_codes: Vec<u16>,
    response: mpsc::SyncSender<Result<(), ExecutionError>>,
}

#[cfg(target_os = "linux")]
struct WaylandPortalSession {
    portal: ashpd::desktop::remote_desktop::RemoteDesktop,
    session: ashpd::desktop::Session<ashpd::desktop::remote_desktop::RemoteDesktop>,
}

#[cfg(target_os = "linux")]
impl KeyboardExecutor for WaylandPortalKeyboardExecutor {
    fn execute(&self, key_codes: &[u16]) -> Result<(), ExecutionError> {
        let linux_key_codes = key_codes
            .iter()
            .map(|scan_code| direct_input_to_linux_keycode(*scan_code))
            .collect::<Result<Vec<_>, _>>()?;
        let sender = self.sender.as_ref().map_err(|error| {
            ExecutionError::new(format!(
                "could not start the Wayland portal worker: {error}"
            ))
        })?;
        let (response_sender, response_receiver) = mpsc::sync_channel(1);
        sender
            .send(WaylandKeyboardCommand {
                key_codes: linux_key_codes,
                response: response_sender,
            })
            .map_err(|_| ExecutionError::new("the Wayland portal worker stopped unexpectedly"))?;
        response_receiver
            .recv_timeout(WAYLAND_PORTAL_TIMEOUT)
            .map_err(|_| {
                ExecutionError::new("the Wayland portal worker did not return an execution result")
            })?
    }
}

#[cfg(target_os = "linux")]
fn start_wayland_portal_worker() -> Result<mpsc::Sender<WaylandKeyboardCommand>, String> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .map_err(|error| error.to_string())?;
    let (sender, receiver) = mpsc::channel::<WaylandKeyboardCommand>();

    thread::Builder::new()
        .name("wayland-keyboard-portal".to_owned())
        .spawn(move || {
            let mut session = None;
            while let Ok(command) = receiver.recv() {
                let result = runtime.block_on(execute_through_wayland_portal(
                    &mut session,
                    &command.key_codes,
                ));
                let _ = command.response.send(result);
            }
        })
        .map_err(|error| error.to_string())?;

    Ok(sender)
}

#[cfg(target_os = "linux")]
async fn execute_through_wayland_portal(
    portal_session: &mut Option<WaylandPortalSession>,
    key_codes: &[u16],
) -> Result<(), ExecutionError> {
    use ashpd::desktop::remote_desktop::{DeviceType, RemoteDesktop, SelectDevicesOptions};
    use ashpd::desktop::{PersistMode, Session};

    if portal_session.is_none() {
        let portal = RemoteDesktop::new().await.map_err(|error| {
            ExecutionError::new(format!(
                "could not connect to the Wayland Remote Desktop portal: {error}"
            ))
        })?;
        let session: Session<RemoteDesktop> = portal
            .create_session(Default::default())
            .await
            .map_err(|error| {
                ExecutionError::new(format!(
                    "could not create a Wayland Remote Desktop session: {error}"
                ))
            })?;

        portal
            .select_devices(
                &session,
                SelectDevicesOptions::default()
                    .set_devices(enumflags2::BitFlags::from_flag(DeviceType::Keyboard))
                    .set_persist_mode(PersistMode::Application),
            )
            .await
            .and_then(|request| request.response())
            .map_err(|error| {
                ExecutionError::new(format!("Wayland keyboard access was not selected: {error}"))
            })?;

        let selected = portal
            .start(&session, None, Default::default())
            .await
            .and_then(|request| request.response())
            .map_err(|error| {
                ExecutionError::new(format!("Wayland keyboard access was not granted: {error}"))
            })?;
        if !selected.devices().contains(DeviceType::Keyboard) {
            return Err(ExecutionError::new(
                "the Wayland Remote Desktop portal did not grant keyboard access",
            ));
        }

        *portal_session = Some(WaylandPortalSession { portal, session });
    }

    let result = send_wayland_keys(
        portal_session
            .as_ref()
            .expect("Wayland portal session was initialized above"),
        key_codes,
    )
    .await;
    if result.is_err() {
        *portal_session = None;
    }
    result
}

#[cfg(target_os = "linux")]
async fn send_wayland_keys(
    portal_session: &WaylandPortalSession,
    key_codes: &[u16],
) -> Result<(), ExecutionError> {
    use ashpd::desktop::remote_desktop::{KeyState, NotifyKeyboardKeycodeOptions};

    tokio::time::sleep(PRE_EXECUTION_DELAY).await;

    let mut pressed = Vec::with_capacity(key_codes.len());
    let press_result = async {
        for key_code in key_codes {
            portal_session
                .portal
                .notify_keyboard_keycode(
                    &portal_session.session,
                    i32::from(*key_code),
                    KeyState::Pressed,
                    NotifyKeyboardKeycodeOptions::default(),
                )
                .await
                .map_err(|error| {
                    ExecutionError::new(format!("could not press Wayland key {key_code}: {error}"))
                })?;
            pressed.push(*key_code);
        }
        tokio::time::sleep(KEY_HOLD_DURATION).await;
        Ok::<(), ExecutionError>(())
    }
    .await;

    let mut release_error = None;
    for key_code in pressed.iter().rev() {
        if let Err(error) = portal_session
            .portal
            .notify_keyboard_keycode(
                &portal_session.session,
                i32::from(*key_code),
                KeyState::Released,
                NotifyKeyboardKeycodeOptions::default(),
            )
            .await
        {
            release_error.get_or_insert_with(|| {
                ExecutionError::new(format!("could not release Wayland key {key_code}: {error}"))
            });
        }
    }

    press_result?;
    if let Some(error) = release_error {
        return Err(error);
    }
    Ok(())
}

#[cfg(target_os = "linux")]
impl KeyboardExecutor for X11KeyboardExecutor {
    fn execute(&self, key_codes: &[u16]) -> Result<(), ExecutionError> {
        use x11rb::{
            connection::Connection,
            protocol::{
                xproto::{KEY_PRESS_EVENT, KEY_RELEASE_EVENT},
                xtest::ConnectionExt as _,
            },
        };

        let x11_key_codes = key_codes
            .iter()
            .map(|scan_code| direct_input_to_x11_keycode(*scan_code))
            .collect::<Result<Vec<_>, _>>()?;

        let connection = self.connection()?;

        let mut pressed = Vec::with_capacity(x11_key_codes.len());
        let press_result = (|| {
            thread::sleep(PRE_EXECUTION_DELAY);
            for key_code in &x11_key_codes {
                connection
                    .xtest_fake_input(KEY_PRESS_EVENT, *key_code, 0, 0, 0, 0, 0)
                    .map_err(|error| {
                        ExecutionError::new(format!("could not press X11 key {key_code}: {error}"))
                    })?
                    .check()
                    .map_err(|error| {
                        ExecutionError::new(format!("X11 rejected key {key_code}: {error}"))
                    })?;
                pressed.push(*key_code);
            }
            connection.flush().map_err(|error| {
                ExecutionError::new(format!("could not flush X11 key presses: {error}"))
            })?;
            thread::sleep(KEY_HOLD_DURATION);
            Ok(())
        })();

        let mut release_error = None;
        for key_code in pressed.iter().rev() {
            let result =
                match connection.xtest_fake_input(KEY_RELEASE_EVENT, *key_code, 0, 0, 0, 0, 0) {
                    Ok(cookie) => cookie.check().map_err(|error| error.to_string()),
                    Err(error) => Err(error.to_string()),
                };
            if let Err(error) = result {
                release_error.get_or_insert_with(|| {
                    ExecutionError::new(format!("could not release X11 key {key_code}: {error}"))
                });
            }
        }
        if let Err(error) = connection.flush() {
            release_error.get_or_insert_with(|| {
                ExecutionError::new(format!("could not flush X11 key releases: {error}"))
            });
        }

        if press_result.is_err() || release_error.is_some() {
            self.invalidate_connection();
        }
        press_result?;
        if let Some(error) = release_error {
            return Err(error);
        }
        Ok(())
    }
}

#[cfg(target_os = "linux")]
fn direct_input_to_x11_keycode(scan_code: u16) -> Result<u8, ExecutionError> {
    let linux_input_code = direct_input_to_linux_keycode(scan_code)?;

    u8::try_from(linux_input_code + 8).map_err(|_| {
        ExecutionError::new(format!(
            "keyboard scan code {scan_code} cannot be represented by X11"
        ))
    })
}

#[cfg(target_os = "linux")]
fn direct_input_to_linux_keycode(scan_code: u16) -> Result<u16, ExecutionError> {
    let linux_input_code = match scan_code {
        1..=83 | 87 | 88 => scan_code,
        100..=102 => 183 + (scan_code - 100),
        141 => 117,
        144 => 165,
        153 => 163,
        156 => 96,
        157 => 97,
        160 => 113,
        161 => 140,
        162 => 164,
        164 => 166,
        174 => 114,
        176 => 115,
        178 => 172,
        179 => 121,
        181 => 98,
        183 => 99,
        184 => 100,
        197 => 119,
        199 => 102,
        200 => 103,
        201 => 104,
        203 => 105,
        205 => 106,
        207 => 107,
        208 => 108,
        209 => 109,
        210 => 110,
        211 => 111,
        219 => 125,
        220 => 126,
        221 => 127,
        222 => 116,
        223 => 142,
        227 => 143,
        _ => {
            return Err(ExecutionError::new(format!(
                "keyboard scan code {scan_code} has no supported X11 translation"
            )));
        }
    };

    Ok(linux_input_code)
}

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
#[derive(Debug)]
struct UnsupportedKeyboardExecutor;

#[cfg(not(any(target_os = "linux", target_os = "windows")))]
impl KeyboardExecutor for UnsupportedKeyboardExecutor {
    fn execute(&self, _key_codes: &[u16]) -> Result<(), ExecutionError> {
        Err(ExecutionError::new(
            "keyboard execution is currently implemented only for Windows and Linux",
        ))
    }
}

#[cfg(test)]
#[path = "tests/keyboard.rs"]
mod tests;
