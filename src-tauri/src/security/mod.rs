// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

mod models;
mod opaque;
mod state;
mod storage;
pub use models::{
    ActivePairingSummary, ConnectionSession, DeviceConnectionStatus, DeviceRecord, DeviceSummary,
    PairingDisplay, PairingSession, PairingStatus,
};

pub use opaque::OpaqueProtocol;
pub use state::SecurityState;
pub use storage::{DeviceStore, StorageError};
