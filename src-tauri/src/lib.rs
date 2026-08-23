// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

pub mod actions;
pub mod config;
pub mod keyboard;
#[cfg(test)]
mod opaque_spike;
pub mod protocol;
pub mod security;
pub mod server;
pub mod state;

mod app;
mod commands;
mod mappings;

pub fn run() {
    app::run();
}
