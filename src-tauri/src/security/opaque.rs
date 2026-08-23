// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use hkdf::Hkdf;
use opaque_ke::{
    CipherSuite, ClientRegistration, ClientRegistrationFinishParameters, Ristretto255, ServerLogin,
    ServerLoginParameters, ServerRegistration, ServerSetup, TripleDh,
};
use rand::SeedableRng;
use rand::rngs::OsRng;
use rand_chacha::ChaCha20Rng;
use sha2::Sha512;

pub struct Suite;
impl CipherSuite for Suite {
    type OprfCs = Ristretto255;
    type KeyExchange = TripleDh<Ristretto255, Sha512>;
    // Must match game-buddy-frontend's explicit 19 MiB / 2-pass / 1-lane
    // Argon2id profile in BRIDGE_KEY_STRETCHING.
    type Ksf = opaque_ke::argon2::Argon2<'static>;
}

#[derive(Clone)]
pub struct OpaqueProtocol {
    setup: ServerSetup<Suite>,
}
impl Default for OpaqueProtocol {
    fn default() -> Self {
        Self::new()
    }
}
impl OpaqueProtocol {
    pub fn new() -> Self {
        let mut rng = OsRng;
        Self {
            setup: ServerSetup::new(&mut rng),
        }
    }

    pub fn from_master_secret(secret: &[u8]) -> Result<Self, String> {
        let mut seed = [0_u8; 32];
        Hkdf::<sha2::Sha256>::new(None, secret)
            .expand(b"gganbu/opaque-server-setup", &mut seed)
            .map_err(|_| "unable to derive OPAQUE server setup".to_owned())?;
        let mut rng = ChaCha20Rng::from_seed(seed);
        Ok(Self {
            setup: ServerSetup::new(&mut rng),
        })
    }

    pub fn create_password_file(
        &self,
        password: &[u8],
        identifier: &[u8],
    ) -> Result<Vec<u8>, String> {
        let mut rng = OsRng;
        let registration = ClientRegistration::<Suite>::start(&mut rng, password)
            .map_err(|error| error.to_string())?;
        let server =
            ServerRegistration::<Suite>::start(&self.setup, registration.message, identifier)
                .map_err(|error| error.to_string())?;
        let finish = registration
            .state
            .finish(
                &mut rng,
                password,
                server.message,
                ClientRegistrationFinishParameters::default(),
            )
            .map_err(|error| error.to_string())?;
        Ok(ServerRegistration::<Suite>::finish(finish.message)
            .serialize()
            .to_vec())
    }

    pub fn login_start(
        &self,
        password_file: &[u8],
        request: &[u8],
        identifier: &[u8],
    ) -> Result<(Vec<u8>, Vec<u8>), String> {
        let record = ServerRegistration::<Suite>::deserialize(password_file)
            .map_err(|error| error.to_string())?;
        let request = opaque_ke::CredentialRequest::<Suite>::deserialize(request)
            .map_err(|error| error.to_string())?;
        let mut rng = OsRng;
        let result = ServerLogin::<Suite>::start(
            &mut rng,
            &self.setup,
            Some(record),
            request,
            identifier,
            ServerLoginParameters::default(),
        )
        .map_err(|error| error.to_string())?;
        Ok((
            result.message.serialize().to_vec(),
            result.state.serialize().to_vec(),
        ))
    }

    pub fn login_finish(&self, state: &[u8], message: &[u8]) -> Result<Vec<u8>, String> {
        let state = ServerLogin::<Suite>::deserialize(state).map_err(|error| error.to_string())?;
        let message = opaque_ke::CredentialFinalization::<Suite>::deserialize(message)
            .map_err(|error| error.to_string())?;
        let result = state
            .finish(message, ServerLoginParameters::default())
            .map_err(|error| error.to_string())?;
        Ok(result.session_key.to_vec())
    }
}

impl OpaqueProtocol {
    pub fn registration_start(&self, request: &[u8], identifier: &[u8]) -> Result<Vec<u8>, String> {
        let request = opaque_ke::RegistrationRequest::<Suite>::deserialize(request)
            .map_err(|error| error.to_string())?;
        let result = ServerRegistration::<Suite>::start(&self.setup, request, identifier)
            .map_err(|error| error.to_string())?;
        Ok(result.message.serialize().to_vec())
    }

    pub fn registration_finish(&self, _state: &[u8], upload: &[u8]) -> Result<Vec<u8>, String> {
        let upload = opaque_ke::RegistrationUpload::<Suite>::deserialize(upload)
            .map_err(|error| error.to_string())?;
        Ok(ServerRegistration::<Suite>::finish(upload)
            .serialize()
            .to_vec())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn master_secret_reconstructs_the_same_server_setup() {
        let first = OpaqueProtocol::from_master_secret(b"stable credential-store secret").unwrap();
        let second = OpaqueProtocol::from_master_secret(b"stable credential-store secret").unwrap();
        let different = OpaqueProtocol::from_master_secret(b"different secret").unwrap();

        assert_eq!(first.setup.serialize(), second.setup.serialize());
        assert_ne!(first.setup.serialize(), different.setup.serialize());
    }
}
