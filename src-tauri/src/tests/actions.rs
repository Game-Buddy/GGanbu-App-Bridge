// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn embedded_war_thunder_mapping_file_is_valid() {
    let catalog = ActionCatalog::parse(ACTION_MAPPINGS_JSON).unwrap();

    for action in [
        "ID_CAMERA_BINOCULARS",
        "ID_RANGEFINDER",
        "ID_SCOUT_UAV",
        "ID_TACTICAL_MAP",
    ] {
        assert!(catalog.mappings.contains_key(action));
    }
}

#[test]
fn numeric_scan_codes_resolve_to_keys_and_chords() {
    let resolver = ActionResolver::for_platform(TargetPlatform::Linux);

    let scout = resolver.resolve("ID_SCOUT_UAV").unwrap();
    assert_eq!(scout.label, "Scout UAV");
    assert_eq!(scout.shortcut.key_codes, [46, 47]);
    assert_eq!(scout.shortcut.keys, ["C", "V"]);
    assert_eq!(scout.shortcut.modifiers, []);
    assert_eq!(scout.shortcut.display, "C + V");

    let rangefinder = resolver.resolve("ID_RANGEFINDER").unwrap();
    assert_eq!(rangefinder.shortcut.key_codes, [56, 45]);
    assert_eq!(rangefinder.shortcut.modifiers, [ShortcutModifier::Alt]);
    assert_eq!(rangefinder.shortcut.keys, ["X"]);
    assert_eq!(rangefinder.shortcut.display, "Alt + X");
}

#[test]
fn scalar_codes_and_unknown_codes_are_supported_safely() {
    let shortcut = ResolvedShortcut::from_key_codes(&[50]);
    assert_eq!(shortcut.keys, ["M"]);
    assert_eq!(shortcut.display, "M");

    let unknown = ResolvedShortcut::from_key_codes(&[999]);
    assert_eq!(unknown.keys, ["Scan code 999"]);
    assert_eq!(unknown.display, "Scan code 999");
}

#[test]
fn alternative_input_bindings_use_the_first_keyboard_binding() {
    let catalog = ActionCatalog::parse(
        r#"{
            "controls": {"version": 5, "basePresetPaths": {
                "default": "config/hotkeys/hotkey.empty_ver1.blk"
            }, "hotkeys": {
                "ID_MIXED_INPUT": [
                    {"mouseButton": 4},
                    {"keyboardKey": [29, 19]},
                    {"keyboardKey": 20}
                ],
                "ID_MOUSE_ONLY": {"mouseButton": 1}
            }}
        }"#,
    )
    .unwrap();

    assert_eq!(catalog.mappings["ID_MIXED_INPUT"], [29, 19]);
    assert!(!catalog.mappings.contains_key("ID_MOUSE_ONLY"));
}

#[test]
fn malformed_catalogs_are_rejected() {
    for (json, expected) in [
        (
            r#"{"controls":{"version":0,"hotkeys":{"ID_TEST":{"keyboardKey":1}}}}"#,
            "controls.version",
        ),
        (
            r#"{"controls":{"version":5,"hotkeys":{"bad-code":{"keyboardKey":1}}}}"#,
            "invalid action code",
        ),
        (
            r#"{"controls":{"version":5,"hotkeys":{"ID_TEST":{"keyboardKey":[]}}}}"#,
            "between 1 and 8",
        ),
        (
            r#"{"controls":{"version":5,"hotkeys":{"ID_TEST":{"keyboardKey":[46,46]}}}}"#,
            "duplicate keyboard scan codes",
        ),
    ] {
        assert!(
            ActionCatalog::parse(json)
                .unwrap_err()
                .to_string()
                .contains(expected)
        );
    }
}

#[test]
fn unknown_actions_and_unsupported_platforms_are_rejected() {
    assert_eq!(
        ActionResolver::for_platform(TargetPlatform::Linux).resolve("ID_FIRE_WEAPON"),
        Err(ResolveError::UnknownAction)
    );
    assert_eq!(
        ActionResolver::for_platform(TargetPlatform::Unsupported).resolve("ID_TACTICAL_MAP"),
        Err(ResolveError::UnsupportedPlatform)
    );
}
