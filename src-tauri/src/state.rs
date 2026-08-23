// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::sync::{Arc, RwLock};

use serde::Serialize;

use crate::{
    actions::{ResolvedShortcut, TargetPlatform},
    security::{DeviceSummary, PairingDisplay, SecurityState},
};

pub const ENDPOINT: &str = "http://0.0.0.0:53177";
pub const BRIDGE_STATE_EVENT: &str = "bridge-state-changed";

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ApplicationStatus {
    Running,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ServerStatus {
    Starting,
    #[serde(rename = "active")]
    Running,
    Error,
    Stopped,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ReceivedMessage {
    pub text: String,
    pub received_at: String,
    pub sequence: u64,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ActionValidation {
    Accepted,
    Rejected,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionAttempt {
    pub action: String,
    pub request_id: String,
    pub label: Option<String>,
    pub platform: TargetPlatform,
    pub shortcut: Option<ResolvedShortcut>,
    pub validation: ActionValidation,
    pub validation_message: String,
    pub executed: bool,
    pub received_at: String,
    pub sequence: u64,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BridgeSnapshot {
    pub revision: u64,
    pub application_status: ApplicationStatus,
    pub server_status: ServerStatus,
    pub endpoint: &'static str,
    pub server_error: Option<String>,
    pub allowed_origins: Vec<String>,
    pub last_message: Option<ReceivedMessage>,
    pub last_action: Option<ActionAttempt>,
    pub action_history: Vec<ActionAttempt>,
    pub active_pairing: Option<PairingDisplay>,
    pub devices: Vec<DeviceSummary>,
    pub active_connection_count: usize,
}

#[derive(Debug)]
struct BridgeStateInner {
    snapshot: BridgeSnapshot,
    next_sequence: u64,
    next_action_sequence: u64,
}

#[derive(Clone, Debug)]
pub struct SharedBridgeState {
    inner: Arc<RwLock<BridgeStateInner>>,
}

pub type StatePublisher = Arc<dyn Fn(BridgeSnapshot) + Send + Sync + 'static>;

impl SharedBridgeState {
    pub fn new(allowed_origins: Vec<String>) -> Self {
        Self {
            inner: Arc::new(RwLock::new(BridgeStateInner {
                snapshot: BridgeSnapshot {
                    revision: 0,
                    application_status: ApplicationStatus::Running,
                    server_status: ServerStatus::Stopped,
                    endpoint: ENDPOINT,
                    server_error: None,
                    allowed_origins,
                    last_message: None,
                    last_action: None,
                    action_history: Vec::new(),
                    active_pairing: None,
                    devices: Vec::new(),
                    active_connection_count: 0,
                },
                next_sequence: 1,
                next_action_sequence: 1,
            })),
        }
    }

    pub fn snapshot(&self) -> BridgeSnapshot {
        self.inner
            .read()
            .expect("bridge state lock poisoned")
            .snapshot
            .clone()
    }

    pub fn set_allowed_origins(&self, allowed_origins: Vec<String>, publisher: &StatePublisher) {
        let snapshot = {
            let mut inner = self.inner.write().expect("bridge state lock poisoned");
            if inner.snapshot.allowed_origins == allowed_origins {
                return;
            }
            inner.snapshot.allowed_origins = allowed_origins;
            inner.snapshot.revision += 1;
            inner.snapshot.clone()
        };
        publisher(snapshot);
    }

    pub fn set_server_status(
        &self,
        status: ServerStatus,
        error: Option<String>,
        publisher: &StatePublisher,
    ) {
        let snapshot = {
            let mut inner = self.inner.write().expect("bridge state lock poisoned");
            if inner.snapshot.server_status == status && inner.snapshot.server_error == error {
                return;
            }
            inner.snapshot.server_status = status;
            inner.snapshot.server_error = error;
            inner.snapshot.revision += 1;
            inner.snapshot.clone()
        };
        publisher(snapshot);
    }

    pub fn set_security_snapshot(&self, security: &SecurityState, publisher: &StatePublisher) {
        let (active_pairing, devices, active_connection_count) = security.ui_snapshot();
        let snapshot = {
            let mut inner = self.inner.write().expect("bridge state lock poisoned");
            if inner.snapshot.active_pairing == active_pairing
                && inner.snapshot.devices == devices
                && inner.snapshot.active_connection_count == active_connection_count
            {
                return;
            }
            inner.snapshot.active_pairing = active_pairing;
            inner.snapshot.devices = devices;
            inner.snapshot.active_connection_count = active_connection_count;
            inner.snapshot.revision += 1;
            inner.snapshot.clone()
        };
        publisher(snapshot);
    }

    pub fn accept_message(
        &self,
        text: String,
        received_at: String,
        publisher: &StatePublisher,
    ) -> ReceivedMessage {
        let (message, snapshot) = {
            let mut inner = self.inner.write().expect("bridge state lock poisoned");
            let message = ReceivedMessage {
                text,
                received_at,
                sequence: inner.next_sequence,
            };
            inner.next_sequence += 1;
            inner.snapshot.last_message = Some(message.clone());
            inner.snapshot.revision += 1;
            (message, inner.snapshot.clone())
        };
        publisher(snapshot);
        message
    }

    pub fn record_action_attempt(
        &self,
        attempt: ActionAttempt,
        publisher: &StatePublisher,
    ) -> ActionAttempt {
        self.record_action_attempts(vec![attempt], publisher)
            .into_iter()
            .next()
            .expect("a single action attempt was supplied")
    }

    pub fn record_action_attempts(
        &self,
        mut attempts: Vec<ActionAttempt>,
        publisher: &StatePublisher,
    ) -> Vec<ActionAttempt> {
        if attempts.is_empty() {
            return attempts;
        }
        let snapshot = {
            let mut inner = self.inner.write().expect("bridge state lock poisoned");
            for attempt in &mut attempts {
                attempt.sequence = inner.next_action_sequence;
                inner.next_action_sequence += 1;
            }
            inner.snapshot.last_action = attempts.last().cloned();
            inner.snapshot.revision += 1;
            inner.snapshot.clone()
        };
        publisher(snapshot);
        attempts
    }

    pub fn record_execute_attempt(
        &self,
        mut attempt: ActionAttempt,
        publisher: &StatePublisher,
    ) -> ActionAttempt {
        const MAX_ACTION_HISTORY: usize = 200;
        let (attempt, snapshot) = {
            let mut inner = self.inner.write().expect("bridge state lock poisoned");
            attempt.sequence = inner.next_action_sequence;
            inner.next_action_sequence += 1;
            inner.snapshot.last_action = Some(attempt.clone());
            inner.snapshot.action_history.push(attempt.clone());
            if inner.snapshot.action_history.len() > MAX_ACTION_HISTORY {
                inner.snapshot.action_history.remove(0);
            }
            inner.snapshot.revision += 1;
            (attempt, inner.snapshot.clone())
        };
        publisher(snapshot);
        attempt
    }
}

pub fn noop_publisher() -> StatePublisher {
    Arc::new(|_| {})
}
