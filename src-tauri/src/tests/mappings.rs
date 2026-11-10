// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn resolves_hotkey_assets_without_case_sensitive_path_matching() {
    let path = PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("ASSETS/CONFIG/HOTKEYS/HOTKEY.KEYBOARD_SHOOTER_VER3.BLKX");
    let resolved = resolve_case_insensitive(&path).unwrap();
    let expected = fs::canonicalize(
        PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("assets/config/hotkeys/hotkey.keyboard_shooter_ver3.blkx"),
    )
    .unwrap();

    assert_eq!(fs::canonicalize(resolved).unwrap(), expected);
}

#[test]
fn parses_a_single_quote_without_panicking() {
    assert_eq!(
        parse_blk_value("\""),
        serde_json::Value::String("\"".into())
    );
}

#[test]
fn rejects_oversized_keybinding_files_before_parsing() {
    let path = std::env::temp_dir().join(format!(
        "gganbu-keybindings-oversized-{}.blkx",
        std::process::id()
    ));
    let oversized = vec![b' '; (MAX_KEYBINDING_FILE_BYTES + 1) as usize];
    fs::write(&path, oversized).expect("test fixture should be writable");

    let result = load_keybinding_chain(
        &path,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut serde_json::Map::new(),
    );
    let _ = fs::remove_file(&path);

    let error = result.expect_err("oversized input must be rejected");
    assert!(error.contains("byte limit"));
}

#[test]
fn keybinding_preview_rows_are_bounded() {
    let hotkeys = (0..=MAX_KEYBINDING_PREVIEW_ROWS)
        .map(|index| (format!("ACTION_{index}"), serde_json::json!({})))
        .collect::<serde_json::Map<_, _>>();
    let rows = keybinding_rows(&hotkeys);

    assert_eq!(rows.len(), MAX_KEYBINDING_PREVIEW_ROWS);
    assert_eq!(hotkeys.len(), MAX_KEYBINDING_PREVIEW_ROWS + 1);
}

#[test]
fn rejects_base_preset_paths_that_escape_the_preset_root() {
    let path = std::env::temp_dir().join(format!(
        "gganbu-keybindings-unsafe-base-{}.blkx",
        std::process::id()
    ));
    fs::write(
        &path,
        r#"{"controls":{"basePresetPaths":{"escape":"../outside.blkx"}}}"#,
    )
    .expect("test fixture should be writable");

    let result = load_keybinding_chain(
        &path,
        &mut Vec::new(),
        &mut Vec::new(),
        &mut serde_json::Map::new(),
    );
    let _ = fs::remove_file(&path);

    let error = result.expect_err("parent-directory base must be rejected");
    assert!(error.contains("unsupported basePresetPaths entry"));
}

#[test]
fn saved_controls_restore_without_rewriting_the_file() {
    let root = std::env::temp_dir().join(format!("gganbu-restore-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let mut document: serde_json::Value =
        serde_json::from_str(include_str!("../../action-mappings.json")).unwrap();
    document["source"] = serde_json::json!("my-controls.blkx");
    let bytes = serde_json::to_vec(&document).unwrap();
    let saved = root.join("user-keybindings.json");
    fs::write(&saved, &bytes).unwrap();
    let preview = load_saved_keybindings(&root).unwrap().unwrap();
    assert_eq!(preview.source, "my-controls.blkx");
    assert_eq!(fs::read(&saved).unwrap(), bytes);
    let _ = fs::remove_dir_all(root);
}

#[test]
fn rejected_import_preserves_the_previous_file() {
    let root = std::env::temp_dir().join(format!("gganbu-rejected-import-{}", std::process::id()));
    fs::create_dir_all(&root).unwrap();
    let saved = root.join("user-keybindings.json");
    fs::write(&saved, b"previous controls").unwrap();
    let selected = root.join("invalid.blkx");
    fs::write(
        &selected,
        r#"{"controls":{"hotkeys":{"ID_TEST":{"keyboardKey":0}}}}"#,
    )
    .unwrap();
    assert!(load_keybindings(&root, &root.join("resources"), Some(selected)).is_err());
    assert_eq!(fs::read(&saved).unwrap(), b"previous controls");
    let _ = fs::remove_dir_all(root);
}
