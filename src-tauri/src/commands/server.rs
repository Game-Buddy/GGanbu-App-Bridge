// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    fs,
    net::SocketAddr,
    sync::{
        Arc, Mutex, MutexGuard,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use tauri::Manager;
use tokio_util::sync::CancellationToken;

use crate::{
    actions, config,
    keyboard::KeyboardExecution,
    security::SecurityState,
    server,
    state::{SharedBridgeState, StatePublisher},
};

use super::{
    bridge::state_publisher,
    pairing::{pairing_status, publish_pairing_status},
};

const SERVER_ADDRESS: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 53177);

pub(crate) struct ServerControl {
    cancellation: Mutex<CancellationToken>,
    started: Arc<AtomicBool>,
    generation: Arc<AtomicU64>,
}

impl ServerControl {
    pub(crate) fn new() -> Self {
        Self {
            cancellation: Mutex::new(CancellationToken::new()),
            started: Arc::new(AtomicBool::new(false)),
            generation: Arc::new(AtomicU64::new(0)),
        }
    }

    pub(crate) fn cancel(&self) {
        self.cancellation
            .lock()
            .expect("server control lock poisoned")
            .cancel();
    }

    fn begin_start(&self) -> Option<MutexGuard<'_, CancellationToken>> {
        let cancellation = self
            .cancellation
            .lock()
            .expect("server control lock poisoned");
        if self.started.swap(true, Ordering::AcqRel) {
            return None;
        }
        Some(cancellation)
    }

    fn stop(&self) {
        self.stop_with_enter_hook(|| {});
    }

    #[cfg(test)]
    fn stop_for_test(&self, on_enter: impl FnOnce()) {
        self.stop_with_enter_hook(on_enter);
    }

    fn stop_with_enter_hook(&self, on_enter: impl FnOnce()) {
        on_enter();
        let mut cancellation = self
            .cancellation
            .lock()
            .expect("server control lock poisoned");
        self.started.store(false, Ordering::Release);
        self.generation.fetch_add(1, Ordering::AcqRel);
        cancellation.cancel();
        *cancellation = CancellationToken::new();
    }
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
pub(crate) fn start_server(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
    control: tauri::State<'_, ServerControl>,
) -> Result<(), String> {
    let Some(mut cancellation_guard) = control.begin_start() else {
        return Ok(());
    };
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
    let publisher: StatePublisher = state_publisher(&app);
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
        KeyboardExecution::system(),
        security.inner().clone(),
    );
    let task_bridge = bridge.inner().clone();
    *cancellation_guard = CancellationToken::new();
    let cancellation = cancellation_guard.clone();
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
pub(crate) fn stop_server(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
    control: tauri::State<'_, ServerControl>,
) {
    security.clear_pairing_and_code();
    super::bridge::publish_security_snapshot(&app, &bridge, &security);
    publish_pairing_status(&app, pairing_status(&security));
    control.stop();
}

#[cfg(test)]
#[path = "../tests/commands_server.rs"]
mod tests;
