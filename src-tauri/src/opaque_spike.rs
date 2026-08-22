// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

//! Phase 1 only: test the pinned RFC 9807 OPAQUE implementation.
#[cfg(test)]
mod tests {
    use opaque_ke::{
        CipherSuite, ClientLogin, ClientLoginFinishParameters, ClientRegistration,
        ClientRegistrationFinishParameters, Ristretto255, ServerLogin, ServerLoginParameters,
        ServerRegistration, ServerSetup, TripleDh,
    };
    use sha2::Sha512;
    struct Suite;
    impl CipherSuite for Suite {
        type OprfCs = Ristretto255;
        type KeyExchange = TripleDh<Ristretto255, Sha512>;
        type Ksf = opaque_ke::argon2::Argon2<'static>;
    }
    #[test]
    fn registration_and_login_match() {
        let mut rng = rand::rngs::OsRng;
        let setup = ServerSetup::<Suite>::new(&mut rng);
        let start = ClientRegistration::<Suite>::start(&mut rng, b"phase-1-password").unwrap();
        let server = ServerRegistration::<Suite>::start(&setup, start.message, b"device").unwrap();
        let finish = start
            .state
            .finish(
                &mut rng,
                b"phase-1-password",
                server.message,
                ClientRegistrationFinishParameters::default(),
            )
            .unwrap();
        let file = ServerRegistration::<Suite>::finish(finish.message);
        let login = ClientLogin::<Suite>::start(&mut rng, b"phase-1-password").unwrap();
        let server_login = ServerLogin::<Suite>::start(
            &mut rng,
            &setup,
            Some(file),
            login.message,
            b"device",
            ServerLoginParameters::default(),
        )
        .unwrap();
        let client_finish = login
            .state
            .finish(
                &mut rng,
                b"phase-1-password",
                server_login.message,
                ClientLoginFinishParameters::default(),
            )
            .unwrap();
        let server_finish = server_login
            .state
            .finish(client_finish.message, ServerLoginParameters::default())
            .unwrap();
        assert_eq!(client_finish.session_key, server_finish.session_key);
    }
}
