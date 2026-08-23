// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    fs,
    net::SocketAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicBool, AtomicU64, Ordering},
    },
};

use tauri::{Emitter, Manager};
use tokio_util::sync::CancellationToken;

use crate::{
    actions, config,
    keyboard::KeyboardExecution,
    security::SecurityState,
    server,
    state::{SharedBridgeState, StatePublisher},
};

use super::{bridge::state_publisher, pairing::pairing_status};

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
pub(crate) fn stop_server(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
    control: tauri::State<'_, ServerControl>,
) {
    security.clear_pairing_and_code();
    super::bridge::publish_security_snapshot(&app, &bridge, &security);
    let _ = app.emit("pairing-status-changed", pairing_status(&security));
    control.started.store(false, Ordering::Release);
    control.generation.fetch_add(1, Ordering::AcqRel);
    control.cancel();
}

#[cfg(test)]
#[path = "../tests/commands_server.rs"]
mod tests;
