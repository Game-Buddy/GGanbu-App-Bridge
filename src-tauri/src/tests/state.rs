// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::{sync::mpsc, thread, time::Duration};

use super::*;

#[test]
fn pairing_transition_guard_serializes_concurrent_operations() {
    let bridge = SharedBridgeState::new(Vec::new());
    let guard = bridge.pairing_transition();
    let (ready_sender, ready_receiver) = mpsc::channel();
    let (done_sender, done_receiver) = mpsc::channel();
    let concurrent_bridge = bridge.clone();

    thread::spawn(move || {
        ready_sender.send(()).unwrap();
        let _guard = concurrent_bridge.pairing_transition();
        done_sender.send(()).unwrap();
    });

    ready_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("concurrent operation should start");
    assert!(
        done_receiver
            .recv_timeout(Duration::from_millis(25))
            .is_err(),
        "pairing publication must remain blocked during another transition"
    );

    drop(guard);
    done_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("concurrent operation should finish after the transition");
}
