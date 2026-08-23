// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use crate::mappings::{self, KeybindingPreview};
use tauri::Manager;

#[tauri::command]
pub(crate) async fn pick_keybindings_file(
    app: tauri::AppHandle,
) -> Result<Option<KeybindingPreview>, String> {
    let dialog =
        rfd::AsyncFileDialog::new().add_filter("War Thunder keybindings", &["blk", "blkx"]);
    let dialog = if let Some(window) = app.get_webview_window("main") {
        dialog.set_parent(&window)
    } else {
        dialog
    };
    let Some(file) = dialog.pick_file().await else {
        return Ok(None);
    };
    let path = file.path().to_path_buf();
    let preview = tauri::async_runtime::spawn_blocking(move || {
        mappings::load_keybindings_from_path(&app, Some(path))
    })
    .await
    .map_err(|error| error.to_string())??;
    Ok(Some(preview))
}

#[tauri::command]
pub(crate) async fn load_default_keybindings(
    app: tauri::AppHandle,
) -> Result<KeybindingPreview, String> {
    tauri::async_runtime::spawn_blocking(move || mappings::load_keybindings_from_path(&app, None))
        .await
        .map_err(|error| error.to_string())?
}
