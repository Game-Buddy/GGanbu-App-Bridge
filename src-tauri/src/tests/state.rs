// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    sync::{TryLockError, mpsc},
    thread,
};

use super::*;

#[test]
fn pairing_transition_guard_serializes_concurrent_operations() {
    let bridge = SharedBridgeState::new(Vec::new());
    let guard = bridge.pairing_transition();
    let (ready_sender, ready_receiver) = mpsc::channel();
    let (done_sender, done_receiver) = mpsc::channel();
    let concurrent_bridge = bridge.clone();

    thread::spawn(move || {
        let lock_result = concurrent_bridge.pairing_transition.try_lock();
        let would_block = matches!(&lock_result, Err(TryLockError::WouldBlock));
        ready_sender.send(would_block).unwrap();
        drop(lock_result);
        let guard = concurrent_bridge.pairing_transition();
        drop(guard);
        done_sender.send(()).unwrap();
    });

    assert!(
        ready_receiver
            .recv()
            .expect("concurrent operation should probe the lock"),
        "concurrent operation must observe the held pairing transition"
    );
    assert!(matches!(
        bridge.pairing_transition.try_lock(),
        Err(TryLockError::WouldBlock)
    ));

    drop(guard);
    done_receiver
        .recv()
        .expect("concurrent operation should finish");
    let probe = bridge
        .pairing_transition
        .try_lock()
        .expect("pairing transition should be released");
    drop(probe);
}

#[test]
fn activity_events_are_small_and_full_history_remains_queryable() {
    let bridge = SharedBridgeState::new(Vec::new());
    let last_payload = std::sync::Arc::new(std::sync::Mutex::new(Vec::new()));
    let capture = last_payload.clone();
    let publisher: StatePublisher = std::sync::Arc::new(move |snapshot| {
        *capture.lock().unwrap() = serde_json::to_vec(&snapshot).unwrap();
    });
    for index in 0..200 {
        bridge.record_execute_attempt(
            ActionAttempt {
                action: "ID_TOGGLE_ENGINE".into(),
                request_id: format!("request-{index}"),
                label: Some("Toggle engine".into()),
                platform: TargetPlatform::Linux,
                shortcut: None,
                validation: ActionValidation::Accepted,
                validation_message: "Keyboard shortcut executed successfully.".into(),
                executed: true,
                received_at: "2026-11-10T00:00:00Z".into(),
                sequence: 0,
            },
            &publisher,
        );
    }
    let event_bytes = last_payload.lock().unwrap().len();
    let snapshot = bridge.snapshot();
    assert_eq!(snapshot.action_history.len(), 200);
    let snapshot_bytes = serde_json::to_vec(&snapshot).unwrap().len();
    assert!(event_bytes < 1500);
    assert!(snapshot_bytes > event_bytes * 40);
    println!("activity snapshot={snapshot_bytes} bytes; event={event_bytes} bytes");
}
