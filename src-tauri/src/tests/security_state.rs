// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

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
