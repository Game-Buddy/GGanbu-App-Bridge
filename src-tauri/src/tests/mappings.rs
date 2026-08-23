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
        "gganbu-keybindings-{}-{}.blkx",
        std::process::id(),
        std::thread::current().name().unwrap_or("test")
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
