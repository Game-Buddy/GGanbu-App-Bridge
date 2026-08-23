// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use crate::mappings::{self, KeybindingPreview};

#[tauri::command]
pub(crate) async fn pick_keybindings_file(
    app: tauri::AppHandle,
) -> Result<Option<KeybindingPreview>, String> {
    let selected = rfd::AsyncFileDialog::new()
        .add_filter("War Thunder keybindings", &["blk", "blkx"])
        .pick_file()
        .await;
    selected
        .map(|file| mappings::load_keybindings_from_path(&app, Some(file.path().to_path_buf())))
        .transpose()
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub(crate) fn load_default_keybindings(app: tauri::AppHandle) -> Result<KeybindingPreview, String> {
    mappings::load_keybindings_from_path(&app, None)
}
