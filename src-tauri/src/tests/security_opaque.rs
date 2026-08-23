// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

#[test]
fn master_secret_reconstructs_the_same_server_setup() {
    let first = OpaqueProtocol::from_master_secret(b"stable credential-store secret").unwrap();
    let second = OpaqueProtocol::from_master_secret(b"stable credential-store secret").unwrap();
    let different = OpaqueProtocol::from_master_secret(b"different secret").unwrap();

    assert_eq!(first.setup.serialize(), second.setup.serialize());
    assert_ne!(first.setup.serialize(), different.setup.serialize());
}
