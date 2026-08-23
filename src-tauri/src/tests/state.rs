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
