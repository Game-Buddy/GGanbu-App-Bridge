// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::Arc;

use tauri::Emitter;

use crate::{
    security::SecurityState,
    state::{BRIDGE_STATE_EVENT, BridgeSnapshot, SharedBridgeState, StatePublisher},
};

pub(crate) fn state_publisher(app: &tauri::AppHandle) -> StatePublisher {
    let app = app.clone();
    Arc::new(move |snapshot| {
        if let Err(error) = app.emit(BRIDGE_STATE_EVENT, snapshot) {
            tracing::warn!(%error, "could not publish bridge state update");
        }
    })
}

pub(crate) fn publish_security_snapshot(
    app: &tauri::AppHandle,
    bridge: &SharedBridgeState,
    security: &SecurityState,
) {
    let publisher = state_publisher(app);
    bridge.set_security_snapshot(security, &publisher);
}

#[tauri::command]
pub(crate) fn get_bridge_snapshot(state: tauri::State<'_, SharedBridgeState>) -> BridgeSnapshot {
    state.snapshot()
}

#[tauri::command]
pub(crate) fn get_host_address() -> String {
    std::net::UdpSocket::bind("0.0.0.0:0")
        .ok()
        .and_then(|socket| {
            socket.connect("8.8.8.8:80").ok()?;
            socket.local_addr().ok()
        })
        .map(|address| address.ip().to_string())
        .unwrap_or_else(|| "127.0.0.1".to_string())
}
