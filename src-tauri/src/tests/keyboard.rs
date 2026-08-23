// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[cfg(target_os = "linux")]
#[test]
fn direct_input_codes_translate_to_x11_keycodes() {
    assert_eq!(direct_input_to_linux_keycode(56).unwrap(), 56); // Left Alt
    assert_eq!(direct_input_to_linux_keycode(157).unwrap(), 97); // Right Ctrl
    assert_eq!(direct_input_to_x11_keycode(56).unwrap(), 64); // Left Alt
    assert_eq!(direct_input_to_x11_keycode(22).unwrap(), 30); // U
    assert_eq!(direct_input_to_x11_keycode(200).unwrap(), 111); // Arrow Up
    assert_eq!(direct_input_to_x11_keycode(211).unwrap(), 119); // Delete
    assert!(direct_input_to_x11_keycode(999).is_err());
}

#[cfg(target_os = "windows")]
#[test]
fn direct_input_codes_translate_to_windows_scan_codes() {
    let left_alt = windows_scan_code(56).unwrap();
    assert_eq!(left_alt.code, 56);
    assert!(!left_alt.extended);

    let right_control = windows_scan_code(157).unwrap();
    assert_eq!(right_control.code, 29);
    assert!(right_control.extended);

    let arrow_up = windows_scan_code(200).unwrap();
    assert_eq!(arrow_up.code, 72);
    assert!(arrow_up.extended);

    assert!(windows_scan_code(128).is_err());
    assert!(windows_scan_code(999).is_err());
}
