// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::{ConnectionSession, DeviceRecord, PairingSession};
use super::{DeviceStore, OpaqueProtocol, StorageError};
use chrono::Utc;
use std::{
    collections::{HashMap, VecDeque},
    sync::{Arc, RwLock},
};

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
        state: Vec<u8>,
        device_id: String,
        display_name: String,
    ) -> bool {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let Some(pairing) = inner.pairing.as_mut() else {
            return false;
        };
        pairing.opaque_registration_state = zeroize::Zeroizing::new(state);
        pairing.pending_device_id = Some(device_id);
        pairing.pending_display_name = Some(display_name);
        true
    }
    pub fn take_registration_state(&self) -> Option<(Vec<u8>, String, String)> {
        let mut inner = self.inner.write().expect("security state lock poisoned");
        let pairing = inner.pairing.as_mut()?;
        if pairing.opaque_registration_state.is_empty() {
            return None;
        }
        Some((
            std::mem::take(&mut pairing.opaque_registration_state).to_vec(),
            pairing.pending_device_id.take()?,
            pairing.pending_display_name.take()?,
        ))
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
        const MAX_TRACKED_DEVICES: usize = 1024;
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
        if inner.device_login_failures.len() >= MAX_TRACKED_DEVICES
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
mod tests {
    use super::*;
    use chrono::Duration;

    #[test]
    fn only_one_unexpired_pairing_is_allowed() {
        let state = SecurityState::default();
        let now = Utc::now();
        let first = PairingSession::new("first".into(), vec![1], now + Duration::minutes(2));
        let second = PairingSession::new("second".into(), vec![2], now + Duration::minutes(2));
        assert!(state.start_pairing(first).is_ok());
        assert!(state.start_pairing(second).is_err());
        state.clear_pairing();
        assert!(state.pairing().is_none());
    }

    #[test]
    fn pairing_is_cancelled_after_five_failures() {
        let state = SecurityState::default();
        let now = Utc::now();
        let pairing = PairingSession::new("pairing".into(), vec![1], now + Duration::minutes(2));
        assert!(
            state
                .start_pairing_with_code(pairing, "12345678".into())
                .is_ok()
        );
        for _ in 0..4 {
            assert!(state.record_pairing_failure());
        }
        assert!(!state.record_pairing_failure());
        assert!(state.pairing().is_none());
        assert!(state.pairing_code().is_none());
    }

    #[test]
    fn replay_counters_and_device_session_replacement_are_enforced() {
        let state = SecurityState::default();
        let now = Utc::now();
        state
            .add_device(DeviceRecord::new(
                "device".into(),
                "Browser".into(),
                vec![1],
                now,
            ))
            .unwrap();
        for index in 0..3 {
            let connection = ConnectionSession::new(
                format!("session-{index}"),
                "device".into(),
                [index as u8; 32],
                now + Duration::hours(1),
            );
            assert!(state.add_connection(connection).is_ok());
        }
        assert!(
            state
                .add_connection(ConnectionSession::new(
                    "session-4".into(),
                    "device".into(),
                    [4; 32],
                    now + Duration::hours(1),
                ))
                .is_err()
        );
        assert!(state.receive_key("session-0", 1).is_some());
        assert!(state.receive_key("session-0", 1).is_some());
        assert!(state.commit_receive_counter("session-0", 1));
        assert!(state.receive_key("session-0", 1).is_none());
        assert!(state.receive_key("session-0", 3).is_none());
        assert!(state.receive_key("session-0", 2).is_some());
        assert!(state.reserve_request_id("session-0", "request-1"));
        assert!(!state.reserve_request_id("session-0", "request-1"));
        for index in 0..1024 {
            assert!(state.reserve_request_id("session-0", &format!("bulk-{index}")));
        }
        assert!(state.reserve_request_id("session-0", "request-1024"));
        assert!(!state.reserve_request_id("session-0", "request-1024"));
        assert!(state.remove_device("device").unwrap());
        assert!(state.receive_key("session-0", 2).is_none());
        let (_, devices, active_connections) = state.ui_snapshot();
        assert!(devices.is_empty());
        assert_eq!(active_connections, 0);
    }

    #[test]
    fn device_login_failures_are_rate_limited_and_clear_after_success() {
        let state = SecurityState::default();
        state
            .add_device(DeviceRecord::new(
                "device-a".into(),
                "Device A".into(),
                vec![1],
                Utc::now(),
            ))
            .unwrap();
        assert!(state.allow_device_login("device-a"));
        for _ in 0..10 {
            state.record_device_login_failure("device-a");
        }
        assert!(!state.allow_device_login("device-a"));
        assert!(state.allow_device_login("device-b"));
        state.clear_device_login_failures("device-a");
        assert!(state.allow_device_login("device-a"));
    }

    #[test]
    fn device_login_failure_tracking_is_bounded() {
        let state = SecurityState::default();
        let inner = state.inner.read().unwrap();
        assert!(inner.device_login_failures.is_empty());
    }

    #[test]
    fn revoked_devices_cannot_receive_connection_sessions() {
        let state = SecurityState::default();
        let now = Utc::now();
        let mut device = DeviceRecord::new("device".into(), "Browser".into(), vec![1], now);
        device.revoked_at = Some(now);
        state.add_device(device).unwrap();
        let connection = ConnectionSession::new(
            "session".into(),
            "device".into(),
            [0; 32],
            now + Duration::hours(1),
        );
        assert!(state.add_connection(connection).is_err());
    }

    #[test]
    fn temporary_session_rejects_hijacks_replays_and_expiry() {
        let state = SecurityState::default();
        let now = Utc::now();
        let pairing = PairingSession::new("pairing".into(), vec![1], now + Duration::minutes(1));
        assert!(state.start_pairing(pairing).is_ok());
        assert!(state.set_temporary_session("temporary".into(), vec![7; 64]));

        assert!(state.temporary_receive_key("stolen-id", 1).is_none());
        assert!(state.temporary_receive_key("temporary", 2).is_none());
        assert!(state.temporary_receive_key("temporary", 1).is_some());
        assert!(state.commit_temporary_receive_counter("temporary", 1));
        assert!(state.temporary_receive_key("temporary", 1).is_none());

        state.clear_pairing();
        let expired = PairingSession::new("expired".into(), vec![1], now - Duration::seconds(1));
        assert!(state.start_pairing(expired).is_ok());
        assert!(state.set_temporary_session("expired-session".into(), vec![7; 64]));
        assert!(state.temporary_receive_key("expired-session", 1).is_none());
    }

    #[test]
    fn persisted_mutations_survive_state_reloads() {
        let root = std::env::temp_dir().join(format!(
            "gganbu-state-persistence-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        let store = DeviceStore::new(root.join("security.json"));
        let state =
            SecurityState::with_store_and_protocol(store.clone(), OpaqueProtocol::new()).unwrap();
        let now = Utc::now();
        state
            .add_device(DeviceRecord::new(
                "device".into(),
                "Browser".into(),
                vec![1, 2, 3],
                now,
            ))
            .unwrap();
        state.rename_device("device", "Tablet".into()).unwrap();
        let seen_at = now + Duration::seconds(5);
        state.mark_device_seen("device", seen_at).unwrap();

        let reloaded =
            SecurityState::with_store_and_protocol(store.clone(), OpaqueProtocol::new()).unwrap();
        let device = reloaded.find_device("device").unwrap();
        assert_eq!(device.display_name, "Tablet");
        assert_eq!(device.last_seen_at, seen_at);
        reloaded.remove_device("device").unwrap();

        let revoked = store.load_devices().unwrap();
        assert!(revoked[0].is_revoked());
        let _ = std::fs::remove_dir_all(root);
    }

    #[test]
    fn failed_persistence_does_not_mutate_memory() {
        let root = std::env::temp_dir().join(format!(
            "gganbu-state-save-error-{}-{}",
            std::process::id(),
            Utc::now().timestamp_nanos_opt().unwrap()
        ));
        std::fs::write(&root, b"blocks directory creation").unwrap();
        let store = DeviceStore::new(root.join("security.json"));
        let state = SecurityState::with_store_and_protocol(store, OpaqueProtocol::new()).unwrap();

        let result = state.add_device(DeviceRecord::new(
            "device".into(),
            "Browser".into(),
            vec![1],
            Utc::now(),
        ));
        assert!(result.is_err());
        assert!(state.devices().is_empty());
        let _ = std::fs::remove_file(root);
    }
}
