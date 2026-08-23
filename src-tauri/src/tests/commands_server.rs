// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;

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
