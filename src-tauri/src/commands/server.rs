// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::bridge::state_publisher;
use crate::{
    config,
    keyboard::KeyboardExecution,
    security::SecurityState,
    server,
    state::{ServerStatus, SharedBridgeState},
};
use std::{net::SocketAddr, sync::Mutex, time::Duration};
use tauri::Manager;
use tokio_util::sync::CancellationToken;

const SERVER_ADDRESS: SocketAddr =
    SocketAddr::new(std::net::IpAddr::V4(std::net::Ipv4Addr::UNSPECIFIED), 53177);

pub(crate) struct ServerControl {
    task: tokio::sync::Mutex<Option<tokio::task::JoinHandle<()>>>,
    cancellation: Mutex<CancellationToken>,
}
impl ServerControl {
    pub(crate) fn new() -> Self {
        Self {
            task: tokio::sync::Mutex::new(None),
            cancellation: Mutex::new(CancellationToken::new()),
        }
    }
    pub(crate) fn cancellation(&self) -> CancellationToken {
        self.cancellation
            .lock()
            .expect("server control lock poisoned")
            .clone()
    }
    pub(crate) fn cancel(&self) {
        self.cancellation
            .lock()
            .expect("server control lock poisoned")
            .cancel();
    }
}

#[tauri::command]
pub(crate) async fn start_server(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
    control: tauri::State<'_, ServerControl>,
) -> Result<(), String> {
    let mut task = control.task.lock().await;
    if task.as_ref().is_some_and(|running| !running.is_finished()) {
        return Ok(());
    }
    if let Some(finished) = task.take() {
        let _ = finished.await;
    }
    let publisher = state_publisher(&app);
    bridge.set_server_status(ServerStatus::Starting, None, &publisher);
    let startup = async {
        if !security.is_persistent() {
            return Err("security storage is unavailable".to_owned());
        }
        let directory = app.path().app_data_dir().map_err(|e| e.to_string())?;
        tauri::async_runtime::spawn_blocking(move || {
            crate::mappings::load_saved_keybindings(&directory)
        })
        .await
        .map_err(|e| e.to_string())??;
        let origins = config::read_allowed_origins().map_err(|e| e.to_string())?;
        bridge.set_allowed_origins(origins.values().to_vec(), &publisher);
        let listener = tokio::net::TcpListener::bind(SERVER_ADDRESS)
            .await
            .map_err(|e| format!("Unable to listen on {SERVER_ADDRESS}: {e}"))?;
        Ok((origins, listener))
    }
    .await;
    let (origins, listener) = match startup {
        Ok(startup) => startup,
        Err(error) => {
            bridge.set_server_status(ServerStatus::Error, Some(error.clone()), &publisher);
            return Err(error);
        }
    };
    let cancellation = CancellationToken::new();
    *control
        .cancellation
        .lock()
        .expect("server control lock poisoned") = cancellation.clone();
    let keyboard = KeyboardExecution::system().with_cancellation(cancellation.clone());
    let router = server::build_router_with_execution_and_security(
        origins,
        bridge.inner().clone(),
        publisher.clone(),
        keyboard,
        security.inner().clone(),
    );
    let task_bridge = bridge.inner().clone();
    let task_security = security.inner().clone();
    bridge.set_server_status(ServerStatus::Running, None, &publisher);
    *task = Some(tokio::spawn(async move {
        let serving = server::serve_listener(
            listener,
            router,
            task_bridge.clone(),
            publisher.clone(),
            cancellation.clone(),
        );
        tokio::pin!(serving);
        let mut interval = tokio::time::interval(Duration::from_secs(1));
        loop {
            tokio::select! {
                result = &mut serving => { if let Err(error) = result { tracing::warn!(%error, "bridge server stopped"); } break; },
                _ = interval.tick() => {
                    let bridge = task_bridge.clone();
                    let security = task_security.clone();
                    let publisher = publisher.clone();
                    let _ = tokio::task::spawn_blocking(move || {
                        let _transition = bridge.pairing_transition();
                        security.expire_sessions();
                        bridge.set_security_snapshot(&security, &publisher);
                    }).await;
                },
                _ = cancellation.cancelled() => {
                    if tokio::time::timeout(Duration::from_secs(2), &mut serving).await.is_err() {
                        task_bridge.set_server_status(ServerStatus::Stopped, None, &publisher);
                    }
                    break;
                }
            }
        }
        cancellation.cancel();
        task_security.close_connections();
        task_bridge.set_security_snapshot(&task_security, &publisher);
    }));
    Ok(())
}

#[tauri::command]
pub(crate) async fn stop_server(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
    control: tauri::State<'_, ServerControl>,
) -> Result<(), String> {
    let mut task = control.task.lock().await;
    control.cancel();
    let stop_bridge = bridge.inner().clone();
    let stop_security = security.inner().clone();
    let publisher = state_publisher(&app);
    tokio::task::spawn_blocking(move || {
        let _transition = stop_bridge.pairing_transition();
        stop_security.clear_pairing_and_code();
        stop_security.close_connections();
        stop_bridge.set_security_snapshot(&stop_security, &publisher);
    })
    .await
    .map_err(|e| e.to_string())?;
    if let Some(running) = task.take() {
        running.await.map_err(|e| e.to_string())?;
    }
    bridge.set_server_status(ServerStatus::Stopped, None, &state_publisher(&app));
    Ok(())
}
