// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use chrono::{DateTime, Utc};
use std::collections::VecDeque;
use zeroize::Zeroizing;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PairingStatus {
    Active,
    Cancelled,
    Expired,
    Completed,
}

#[derive(Clone)]
pub struct PairingSession {
    pub pairing_id: String,
    pub opaque_password_file: Zeroizing<Vec<u8>>,
    pub expires_at: DateTime<Utc>,
    pub failed_attempts: u8,
    pub opaque_login_state: Zeroizing<Vec<u8>>,
    pub opaque_registration_state: Zeroizing<Vec<u8>>,
    pub temporary_session_id: Option<String>,
    pub temporary_session_key: Option<Zeroizing<Vec<u8>>>,
    pub temporary_receive_counter: u64,
    pub temporary_send_counter: u64,
    pub pending_device_id: Option<String>,
    pub pending_display_name: Option<String>,
    pub status: PairingStatus,
}
impl PairingSession {
    pub fn new(
        pairing_id: String,
        opaque_password_file: Vec<u8>,
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            pairing_id,
            opaque_password_file: Zeroizing::new(opaque_password_file),
            expires_at,
            failed_attempts: 0,
            opaque_login_state: Zeroizing::new(Vec::new()),
            opaque_registration_state: Zeroizing::new(Vec::new()),
            temporary_session_id: None,
            temporary_session_key: None,
            temporary_receive_counter: 0,
            temporary_send_counter: 0,
            pending_device_id: None,
            pending_display_name: None,
            status: PairingStatus::Active,
        }
    }
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        self.status == PairingStatus::Active && now >= self.expires_at
    }
}

#[derive(Clone)]
pub struct DeviceRecord {
    pub device_id: String,
    pub display_name: String,
    pub opaque_password_file: Zeroizing<Vec<u8>>,
    pub created_at: DateTime<Utc>,
    pub last_seen_at: DateTime<Utc>,
    pub revoked_at: Option<DateTime<Utc>>,
}
impl DeviceRecord {
    pub fn new(
        device_id: String,
        display_name: String,
        opaque_password_file: Vec<u8>,
        now: DateTime<Utc>,
    ) -> Self {
        Self {
            device_id,
            display_name,
            opaque_password_file: Zeroizing::new(opaque_password_file),
            created_at: now,
            last_seen_at: now,
            revoked_at: None,
        }
    }
    pub fn is_revoked(&self) -> bool {
        self.revoked_at.is_some()
    }
}

#[derive(Clone)]
pub struct ConnectionSession {
    pub session_id: String,
    pub device_id: String,
    pub encryption_key: Zeroizing<[u8; 32]>,
    pub send_encryption_key: Zeroizing<[u8; 32]>,
    pub receive_counter: u64,
    pub send_counter: u64,
    pub expires_at: DateTime<Utc>,
    pub processed_request_ids: VecDeque<String>,
}
impl ConnectionSession {
    pub fn new(
        session_id: String,
        device_id: String,
        encryption_key: [u8; 32],
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            session_id,
            device_id,
            encryption_key: Zeroizing::new(encryption_key),
            send_encryption_key: Zeroizing::new(encryption_key),
            receive_counter: 0,
            send_counter: 0,
            expires_at,
            processed_request_ids: VecDeque::new(),
        }
    }
    pub fn is_expired(&self, now: DateTime<Utc>) -> bool {
        now >= self.expires_at
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "lowercase")]
pub enum DeviceConnectionStatus {
    Connected,
    Idle,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceSummary {
    pub id: String,
    pub name: String,
    pub status: DeviceConnectionStatus,
    pub first_paired: DateTime<Utc>,
    pub last_seen: DateTime<Utc>,
    pub session_count: usize,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActivePairingSummary {
    pub pairing_id: String,
    pub expires_at: DateTime<Utc>,
    pub failed_attempts: u8,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PairingDisplay {
    pub code: String,
    pub expires_at: DateTime<Utc>,
    pub failed_attempts: u8,
}

impl ConnectionSession {
    pub fn new_with_keys(
        session_id: String,
        device_id: String,
        receive_key: [u8; 32],
        send_key: [u8; 32],
        expires_at: DateTime<Utc>,
    ) -> Self {
        Self {
            session_id,
            device_id,
            encryption_key: Zeroizing::new(receive_key),
            send_encryption_key: Zeroizing::new(send_key),
            receive_counter: 0,
            send_counter: 0,
            expires_at,
            processed_request_ids: VecDeque::new(),
        }
    }
}
