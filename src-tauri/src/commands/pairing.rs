// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use chrono::{DateTime, Duration, Utc};
use rand::{Rng, RngCore, rngs::OsRng};
use serde::Serialize;
use tauri::Emitter;

use crate::{
    security::{PairingSession, SecurityState},
    state::SharedBridgeState,
};

use super::bridge::publish_security_snapshot;

#[derive(Clone, Debug, Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PairingStatusResponse {
    active: bool,
    code: Option<String>,
    expires_at: Option<DateTime<Utc>>,
    failed_attempts: u8,
}

pub(crate) fn pairing_status(security: &SecurityState) -> PairingStatusResponse {
    let Some(session) = security.pairing() else {
        return PairingStatusResponse {
            active: false,
            code: None,
            expires_at: None,
            failed_attempts: 0,
        };
    };
    if session.is_expired(Utc::now()) {
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

pub(crate) fn publish_pairing_status(app: &tauri::AppHandle, status: PairingStatusResponse) {
    if let Err(error) = app.emit("pairing-status-changed", status) {
        tracing::warn!(%error, "could not publish pairing status update");
    }
}

#[tauri::command]
pub(crate) fn start_pairing(
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
    let _pairing_transition = bridge.pairing_transition();
    security
        .start_pairing_with_code(session, code)
        .map_err(|_| "a pairing session is already active".to_owned())?;
    let status = pairing_status(&security);
    publish_security_snapshot(&app, &bridge, &security);
    publish_pairing_status(&app, status.clone());
    Ok(status)
}

#[tauri::command]
pub(crate) fn cancel_pairing(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
) {
    let _pairing_transition = bridge.pairing_transition();
    security.clear_pairing_and_code();
    publish_security_snapshot(&app, &bridge, &security);
    publish_pairing_status(&app, pairing_status(&security));
}

#[tauri::command]
pub(crate) fn get_pairing_status(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, SharedBridgeState>,
    security: tauri::State<'_, SecurityState>,
) -> PairingStatusResponse {
    let _pairing_transition = bridge.pairing_transition();
    let expired = security.clear_expired_pairing(Utc::now());
    let status = pairing_status(&security);
    if expired {
        publish_security_snapshot(&app, &bridge, &security);
        publish_pairing_status(&app, status.clone());
    }
    status
}
