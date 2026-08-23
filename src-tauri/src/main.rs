// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

#![cfg_attr(not(debug_assertions), windows_subsystem = "windows")]

fn main() {
    gganbu_app_bridge_lib::run();
}
