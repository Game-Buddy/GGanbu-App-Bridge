// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use std::{sync::mpsc, thread, time::Duration};

#[test]
fn failed_current_generation_allows_restart() {
    let started = AtomicBool::new(true);
    let generation = AtomicU64::new(4);

    assert!(finish_server_generation(&started, &generation, 4));
    assert!(!started.load(Ordering::Acquire));
}

#[test]
fn old_server_completion_cannot_clear_a_new_start() {
    let started = AtomicBool::new(true);
    let generation = AtomicU64::new(5);

    assert!(!finish_server_generation(&started, &generation, 4));
    assert!(started.load(Ordering::Acquire));
}

#[test]
fn stop_waits_for_an_in_progress_start_transition() {
    let control = Arc::new(ServerControl::new());
    let start_guard = control.begin_start().expect("start should acquire control");
    let (done_sender, done_receiver) = mpsc::channel();
    let stop_control = Arc::clone(&control);

    thread::spawn(move || {
        stop_control.stop();
        done_sender.send(()).unwrap();
    });

    assert!(
        done_receiver
            .recv_timeout(Duration::from_millis(25))
            .is_err()
    );
    drop(start_guard);
    done_receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("stop should finish after start releases control");
    assert!(!control.started.load(Ordering::Acquire));
}
