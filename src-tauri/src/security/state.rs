// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::{ConnectionSession, DeviceRecord, PairingSession};
use super::{DeviceStore, OpaqueProtocol, StorageError};
use chrono::{DateTime, Utc};
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, RwLock},
};

const MAX_TRACKED_LOGIN_FAILURES: usize = 1024;

#[derive(Default)]
struct Inner {
    pairing: Option<PairingSession>,
    pairing_code: Option<zeroize::Zeroizing<String>>,
    devices: Vec<DeviceRecord>,
    connections: Vec<ConnectionSession>,
    pending_logins: HashMap<String, (String, Vec<u8>, chrono::DateTime<chrono::Utc>)>,
    device_login_failures: HashMap<String, VecDeque<chrono::DateTime<chrono::Utc>>>,
}

#[derive(Clone)]
pub struct SecurityState {
    inner: Arc<RwLock<Inner>>,
    protocol: Arc<OpaqueProtocol>,
    store: Option<Arc<DeviceStore>>,
}

impl Default for SecurityState {
    fn default() -> Self {
        Self {
            inner: Arc::new(RwLock::new(Inner::default())),
            protocol: Arc::new(OpaqueProtocol::new()),
            store: None,
        }
    }
}

impl SecurityState {
    pub fn is_persistent(&self) -> bool {
        self.store.is_some()
    }

    pub fn load(store: DeviceStore) -> Result<Self, StorageError> {
        let devices = store.load_devices()?;
        let secret = store.load_or_create_server_secret()?;
        let protocol =
            OpaqueProtocol::from_master_secret(&secret).map_err(StorageError::SecretEncoding)?;
        Ok(Self {
            inner: Arc::new(RwLock::new(Inner {
                devices,
                ..Inner::default()
            })),
            protocol: Arc::new(protocol),
            store: Some(Arc::new(store)),
        })
    }

    #[cfg(test)]
    pub fn with_store_and_protocol(
        store: DeviceStore,
        protocol: OpaqueProtocol,
    ) -> Result<Self, StorageError> {
        let devices = store.load_devices()?;
        Ok(Self {
            inner: Arc::new(RwLock::new(Inner {
                devices,
                ..Inner::default()
            })),
            protocol: Arc::new(protocol),
            store: Some(Arc::new(store)),
        })
    }

    fn save_devices(&self, devices: &[DeviceRecord]) -> Result<(), StorageError> {
        self.store
            .as_ref()
            .map_or(Ok(()), |store| store.save_devices(devices))
    }

    pub fn opaque(&self) -> Arc<OpaqueProtocol> {
        self.protocol.clone()
    }

    #[allow(clippy::result_large_err)]
    pub fn start_pairing(&self, session: PairingSession) -> Result<(), PairingSession> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        if inner
            .pairing
            .as_ref()
            .is_some_and(|pairing| !pairing.is_expired(Utc::now()))
        {
            return Err(session);
        }
        inner.pairing = Some(session);
        Ok(())
    }
    pub fn pairing(&self) -> Option<PairingSession> {
        self.inner
            .read()
            .expect("security state lock poisoned")
            .pairing
            .clone()
    }
    pub fn clear_pairing(&self) {
        self.inner
            .write()
            .expect("security state lock poisoned")
            .pairing = None;
    }
    pub fn add_device(&self, device: DeviceRecord) -> Result<(), StorageError> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let mut devices = inner.devices.clone();
        devices.retain(|stored| stored.device_id != device.device_id);
        devices.push(device);
        self.save_devices(&devices)?;
        inner.devices = devices;
        Ok(())
    }

    pub fn add_device_if_pairing_matches(
        &self,
        pairing_id: &str,
        device: DeviceRecord,
    ) -> Result<bool, StorageError> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        if inner
            .pairing
            .as_ref()
            .is_none_or(|pairing| pairing.pairing_id != pairing_id)
        {
            return Ok(false);
        }
        let mut devices = inner.devices.clone();
        devices.retain(|stored| stored.device_id != device.device_id);
        devices.push(device);
        self.save_devices(&devices)?;
        inner.devices = devices;
        Ok(true)
    }
    pub fn devices(&self) -> Vec<DeviceRecord> {
        self.inner
            .read()
            .expect("security state lock poisoned")
            .devices
            .clone()
    }
    #[allow(clippy::result_large_err)]
    pub fn add_connection(&self, connection: ConnectionSession) -> Result<(), ConnectionSession> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        if inner
            .devices
            .iter()
            .find(|device| device.device_id == connection.device_id)
            .is_none_or(DeviceRecord::is_revoked)
        {
            return Err(connection);
        }
        let now = Utc::now();
        inner.connections.retain(|session| !session.is_expired(now));
        const MAX_SESSIONS_PER_DEVICE: usize = 3;
        if inner
            .connections
            .iter()
            .filter(|session| session.device_id == connection.device_id)
            .count()
            >= MAX_SESSIONS_PER_DEVICE
        {
            tracing::warn!(device_id = %connection.device_id, "maximum active sessions reached");
            return Err(connection);
        }
        inner.connections.push(connection);
        Ok(())
    }
    pub fn connections(&self) -> Vec<ConnectionSession> {
        self.inner
            .read()
            .expect("security state lock poisoned")
            .connections
            .clone()
    }
    pub fn has_active_connection(&self) -> bool {
        let now = Utc::now();
        self.inner
            .read()
            .expect("security state lock poisoned")
            .connections
            .iter()
            .any(|session| !session.is_expired(now))
    }
    pub fn remove_connection(&self, session_id: &str) -> bool {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let before = inner.connections.len();
        inner
            .connections
            .retain(|session| session.session_id != session_id);
        before != inner.connections.len()
    }
    pub fn close_device_connections(&self, device_id: &str) {
        self.inner
            .write()
            .expect("security state lock poisoned")
            .connections
            .retain(|session| session.device_id != device_id);
    }
}

impl SecurityState {
    pub fn remove_device(&self, device_id: &str) -> Result<bool, StorageError> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let mut devices = inner.devices.clone();
        let Some(device) = devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
        else {
            return Ok(false);
        };
        device.revoked_at = Some(Utc::now());
        self.save_devices(&devices)?;
        inner.devices = devices;
        inner
            .connections
            .retain(|session| session.device_id != device_id);
        Ok(true)
    }

    pub fn rename_device(
        &self,
        device_id: &str,
        display_name: String,
    ) -> Result<bool, StorageError> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let mut devices = inner.devices.clone();
        let Some(device) = devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
        else {
            return Ok(false);
        };
        device.display_name = display_name;
        self.save_devices(&devices)?;
        inner.devices = devices;
        Ok(true)
    }
}

impl SecurityState {
    #[allow(clippy::result_large_err)]
    pub fn start_pairing_with_code(
        &self,
        session: PairingSession,
        code: String,
    ) -> Result<(), PairingSession> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        if inner
            .pairing
            .as_ref()
            .is_some_and(|pairing| !pairing.is_expired(Utc::now()))
        {
            return Err(session);
        }
        inner.pairing = Some(session);
        inner.pairing_code = Some(zeroize::Zeroizing::new(code));
        Ok(())
    }

    pub fn pairing_code(&self) -> Option<String> {
        self.inner
            .read()
            .expect("security state lock poisoned")
            .pairing_code
            .as_ref()
            .map(|code| code.to_string())
    }

    pub fn clear_pairing_and_code(&self) {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        inner.pairing = None;
        inner.pairing_code = None;
    }

    pub fn clear_expired_pairing(&self, now: DateTime<Utc>) -> bool {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        if !inner
            .pairing
            .as_ref()
            .is_some_and(|pairing| pairing.is_expired(now))
        {
            return false;
        }
        inner.pairing = None;
        inner.pairing_code = None;
        true
    }

    pub fn record_pairing_failure(&self) -> bool {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let Some(pairing) = inner.pairing.as_mut() else {
            return false;
        };
        pairing.failed_attempts = pairing.failed_attempts.saturating_add(1);
        tracing::warn!(pairing_id = %pairing.pairing_id, attempts = pairing.failed_attempts, "pairing authentication failed");
        if pairing.failed_attempts >= 5 {
            inner.pairing = None;
            inner.pairing_code = None;
            tracing::warn!("pairing cancelled after repeated authentication failures");
            false
        } else {
            true
        }
    }
}
impl SecurityState {
    pub fn set_login_state(&self, state: Vec<u8>) -> bool {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let Some(pairing) = inner.pairing.as_mut() else {
            return false;
        };
        pairing.opaque_login_state = zeroize::Zeroizing::new(state);
        true
    }
    pub fn take_login_state(&self) -> Option<Vec<u8>> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let pairing = inner.pairing.as_mut()?;
        if pairing.opaque_login_state.is_empty() {
            return None;
        }
        Some(std::mem::take(&mut pairing.opaque_login_state).to_vec())
    }
    pub fn set_temporary_session(&self, session_id: String, key: Vec<u8>) -> bool {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let Some(pairing) = inner.pairing.as_mut() else {
            return false;
        };
        pairing.temporary_session_id = Some(session_id);
        pairing.temporary_session_key = Some(zeroize::Zeroizing::new(key));
        pairing.temporary_receive_counter = 0;
        pairing.temporary_send_counter = 0;
        true
    }

    pub fn temporary_receive_key(&self, session_id: &str, counter: u64) -> Option<Vec<u8>> {
        let inner = self.inner.read().expect("security state lock poisoned");
        let pairing = inner.pairing.as_ref()?;
        if pairing.is_expired(Utc::now())
            || pairing.temporary_session_id.as_deref() != Some(session_id)
            || pairing.temporary_receive_counter.checked_add(1) != Some(counter)
        {
            return None;
        }
        pairing
            .temporary_session_key
            .as_ref()
            .map(|key| key.to_vec())
    }

    pub fn commit_temporary_receive_counter(&self, session_id: &str, counter: u64) -> bool {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let Some(pairing) = inner.pairing.as_mut() else {
            return false;
        };
        if pairing.is_expired(Utc::now())
            || pairing.temporary_session_id.as_deref() != Some(session_id)
            || pairing.temporary_receive_counter.checked_add(1) != Some(counter)
        {
            return false;
        }
        pairing.temporary_receive_counter = counter;
        true
    }

    pub fn next_temporary_send_key(&self, session_id: &str) -> Option<(Vec<u8>, u64)> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let pairing = inner.pairing.as_mut()?;
        if pairing.is_expired(Utc::now())
            || pairing.temporary_session_id.as_deref() != Some(session_id)
        {
            return None;
        }
        pairing.temporary_send_counter = pairing.temporary_send_counter.checked_add(1)?;
        Some((
            pairing.temporary_session_key.as_ref()?.to_vec(),
            pairing.temporary_send_counter,
        ))
    }
    pub fn set_registration_state(
        &self,
        pairing_id: &str,
        state: Vec<u8>,
        device_id: String,
        display_name: String,
    ) -> bool {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let Some(pairing) = inner.pairing.as_mut() else {
            return false;
        };
        if pairing.pairing_id != pairing_id {
            return false;
        }
        pairing.opaque_registration_state = zeroize::Zeroizing::new(state);
        pairing.pending_device_id = Some(device_id);
        pairing.pending_display_name = Some(display_name);
        true
    }
    pub fn take_registration_state_for(
        &self,
        pairing_id: &str,
    ) -> Option<(Vec<u8>, String, String)> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let pairing = inner.pairing.as_mut()?;
        if pairing.pairing_id != pairing_id {
            return None;
        }
        if pairing.opaque_registration_state.is_empty() {
            return None;
        }
        Some((
            std::mem::take(&mut pairing.opaque_registration_state).to_vec(),
            pairing.pending_device_id.take()?,
            pairing.pending_display_name.take()?,
        ))
    }

    pub fn clear_pairing_if_matches(&self, pairing_id: &str) -> bool {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        if inner
            .pairing
            .as_ref()
            .is_none_or(|pairing| pairing.pairing_id != pairing_id)
        {
            return false;
        }
        inner.pairing = None;
        inner.pairing_code = None;
        true
    }
}
impl SecurityState {
    pub fn find_device(&self, device_id: &str) -> Option<DeviceRecord> {
        self.inner
            .read()
            .expect("security state lock poisoned")
            .devices
            .iter()
            .find(|device| device.device_id == device_id && !device.is_revoked())
            .cloned()
    }
    pub fn begin_device_login(
        &self,
        handshake_id: String,
        device_id: String,
        state: Vec<u8>,
    ) -> bool {
        const MAX_PENDING_LOGINS: usize = 256;
        const MAX_PENDING_LOGINS_PER_DEVICE: usize = 4;
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let now = Utc::now();
        inner
            .pending_logins
            .retain(|_, (_, _, expires_at)| now < *expires_at);
        if inner.pending_logins.len() >= MAX_PENDING_LOGINS {
            tracing::warn!("maximum pending device logins reached");
            return false;
        }
        if inner
            .pending_logins
            .values()
            .filter(|(pending_device, _, _)| pending_device == &device_id)
            .count()
            >= MAX_PENDING_LOGINS_PER_DEVICE
        {
            tracing::warn!(device_id = %device_id, "maximum pending device logins for device reached");
            return false;
        }
        if inner
            .devices
            .iter()
            .any(|device| device.device_id == device_id && !device.is_revoked())
        {
            inner.pending_logins.insert(
                handshake_id,
                (device_id, state, now + chrono::Duration::seconds(30)),
            );
            true
        } else {
            false
        }
    }
    pub fn take_device_login(&self, handshake_id: &str, device_id: &str) -> Option<Vec<u8>> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let (stored_device, state, expires_at) = inner.pending_logins.remove(handshake_id)?;
        if stored_device == device_id && Utc::now() < expires_at {
            Some(state)
        } else {
            tracing::warn!(device_id = %device_id, "expired or mismatched session handshake");
            None
        }
    }
    pub fn receive_key(&self, session_id: &str, counter: u64) -> Option<[u8; 32]> {
        let inner = self.inner.read().expect("security state lock poisoned");
        let session = inner
            .connections
            .iter()
            .find(|session| session.session_id == session_id)?;
        if session.receive_counter.checked_add(1) != Some(counter) || session.is_expired(Utc::now())
        {
            return None;
        }
        Some(*session.encryption_key)
    }
    pub fn commit_receive_counter(&self, session_id: &str, counter: u64) -> bool {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let Some(session) = inner
            .connections
            .iter_mut()
            .find(|session| session.session_id == session_id)
        else {
            return false;
        };
        if session.receive_counter.checked_add(1) != Some(counter) || session.is_expired(Utc::now())
        {
            return false;
        }
        session.receive_counter = counter;
        true
    }
    pub fn reserve_request_id(&self, session_id: &str, request_id: &str) -> bool {
        const MAX_PROCESSED_REQUEST_IDS: usize = 1024;

        let mut inner = self.inner.write().expect("security state lock poisoned");
        let Some(session) = inner
            .connections
            .iter_mut()
            .find(|session| session.session_id == session_id)
        else {
            return false;
        };
        if session.is_expired(Utc::now())
            || session
                .processed_request_ids
                .iter()
                .any(|stored| stored == request_id)
        {
            return false;
        }
        if session.processed_request_ids.len() >= MAX_PROCESSED_REQUEST_IDS {
            session.processed_request_ids.pop_front();
        }
        session
            .processed_request_ids
            .push_back(request_id.to_owned());
        true
    }

    pub fn allow_device_login(&self, device_id: &str) -> bool {
        const LOGIN_FAILURE_WINDOW_SECONDS: i64 = 30;
        const MAX_LOGIN_ATTEMPTS: usize = 10;
        let inner = self.inner.read().expect("security state lock poisoned");
        let cutoff = Utc::now() - chrono::Duration::seconds(LOGIN_FAILURE_WINDOW_SECONDS);
        inner
            .device_login_failures
            .get(device_id)
            .map_or(0, |failures| {
                failures
                    .iter()
                    .filter(|timestamp| **timestamp > cutoff)
                    .count()
            })
            < MAX_LOGIN_ATTEMPTS
    }

    pub fn record_device_login_failure(&self, device_id: &str) {
        const LOGIN_FAILURE_WINDOW_SECONDS: i64 = 30;
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let now = Utc::now();
        let cutoff = now - chrono::Duration::seconds(LOGIN_FAILURE_WINDOW_SECONDS);
        if !inner
            .devices
            .iter()
            .any(|device| device.device_id == device_id && !device.is_revoked())
        {
            return;
        }
        inner.device_login_failures.retain(|_, failures| {
            failures.retain(|timestamp| *timestamp > cutoff);
            !failures.is_empty()
        });
        if inner.device_login_failures.len() >= MAX_TRACKED_LOGIN_FAILURES
            && !inner.device_login_failures.contains_key(device_id)
        {
            tracing::warn!("maximum tracked login-failure devices reached");
            return;
        }
        let failures = inner
            .device_login_failures
            .entry(device_id.to_owned())
            .or_default();
        failures.push_back(now);
        tracing::warn!("device authentication failed");
    }

    pub fn clear_device_login_failures(&self, device_id: &str) {
        self.inner
            .write()
            .expect("security state lock poisoned")
            .device_login_failures
            .remove(device_id);
    }
    pub fn next_send_key(&self, session_id: &str) -> Option<([u8; 32], u64)> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let session = inner
            .connections
            .iter_mut()
            .find(|session| session.session_id == session_id)?;
        if session.is_expired(Utc::now()) {
            return None;
        }
        session.send_counter = session.send_counter.checked_add(1)?;
        Some((*session.send_encryption_key, session.send_counter))
    }
    pub fn mark_device_seen(
        &self,
        device_id: &str,
        seen_at: chrono::DateTime<Utc>,
    ) -> Result<bool, StorageError> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let mut devices = inner.devices.clone();
        if let Some(device) = devices
            .iter_mut()
            .find(|device| device.device_id == device_id)
        {
            device.last_seen_at = seen_at;
            self.save_devices(&devices)?;
            inner.devices = devices;
            Ok(true)
        } else {
            Ok(false)
        }
    }
}

impl SecurityState {
    pub fn ui_snapshot(
        &self,
    ) -> (
        Option<super::PairingDisplay>,
        Vec<super::DeviceSummary>,
        usize,
    ) {
        let inner = self.inner.read().expect("security state lock poisoned");
        let pairing = inner.pairing.as_ref().and_then(|session| {
            if session.is_expired(Utc::now()) {
                return None;
            }
            Some(super::PairingDisplay {
                code: inner.pairing_code.as_ref()?.to_string(),
                expires_at: session.expires_at,
                failed_attempts: session.failed_attempts,
            })
        });
        let devices = inner
            .devices
            .iter()
            .filter(|device| !device.is_revoked())
            .map(|device| {
                let session_count = inner
                    .connections
                    .iter()
                    .filter(|session| {
                        session.device_id == device.device_id && !session.is_expired(Utc::now())
                    })
                    .count();
                super::DeviceSummary {
                    id: device.device_id.clone(),
                    name: device.display_name.clone(),
                    status: if session_count > 0 {
                        super::DeviceConnectionStatus::Connected
                    } else {
                        super::DeviceConnectionStatus::Idle
                    },
                    first_paired: device.created_at,
                    last_seen: device.last_seen_at,
                    session_count,
                }
            })
            .collect();
        let active_connections = inner
            .connections
            .iter()
            .filter(|session| !session.is_expired(Utc::now()))
            .count();
        (pairing, devices, active_connections)
    }
}

#[cfg(test)]
#[path = "../tests/security_state.rs"]
mod tests;
