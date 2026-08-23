// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn defaults_are_local_vite_origins() {
    let origins = AllowedOrigins::parse_override(None).unwrap();
    assert_eq!(origins.values(), DEFAULT_ORIGINS);
}

#[test]
fn release_origins_are_static() {
    assert_eq!(RELEASE_BUILD_ENV, "GGANBU_RELEASE_BUILD");
    assert_eq!(RELEASE_ORIGINS, ["https://gganbu.app"]);
}

#[test]
fn configured_origins_follow_compile_time_build_mode() {
    let origins = read_allowed_origins().unwrap();

    if is_release_build() {
        assert_eq!(origins.values(), RELEASE_ORIGINS);
    } else {
        assert_eq!(origins.values(), DEFAULT_ORIGINS);
    }
}

#[test]
fn override_replaces_defaults_normalizes_and_deduplicates() {
    let origins = AllowedOrigins::parse_override(Some(
        "https://app.example.com/, http://localhost:5173,https://app.example.com",
    ))
    .unwrap();

    assert_eq!(
        origins.values(),
        ["https://app.example.com", "http://localhost:5173"]
    );
}

#[test]
fn invalid_origins_are_rejected() {
    for candidate in [
        "",
        "*",
        "https://*.example.com",
        "ftp://example.com",
        "https://user@example.com",
        "https://example.com/path",
        "https://example.com?query=yes",
        "https://example.com#fragment",
        "not a URL",
        "https://example.com,",
    ] {
        assert!(
            AllowedOrigins::parse_override(Some(candidate)).is_err(),
            "expected '{candidate}' to be rejected"
        );
    }
}
