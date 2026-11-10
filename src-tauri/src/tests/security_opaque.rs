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

#[test]
fn ristretto_suite_preserves_crypto_bigint_exception_assumptions() {
    use std::any::TypeId;
    // Changing either suite invalidates the RUSTSEC-2026-0387 reachability review.
    assert_eq!(
        TypeId::of::<<Suite as CipherSuite>::OprfCs>(),
        TypeId::of::<Ristretto255>()
    );
    assert_eq!(
        TypeId::of::<<Suite as CipherSuite>::KeyExchange>(),
        TypeId::of::<TripleDh<Ristretto255, Sha512>>()
    );
}
