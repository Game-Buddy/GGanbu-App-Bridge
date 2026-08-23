// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use crate::security::SecurityState;

use super::bridge::publish_security_snapshot;

#[tauri::command]
pub(crate) fn remove_device(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, crate::state::SharedBridgeState>,
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
pub(crate) fn rename_device(
    app: tauri::AppHandle,
    bridge: tauri::State<'_, crate::state::SharedBridgeState>,
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
