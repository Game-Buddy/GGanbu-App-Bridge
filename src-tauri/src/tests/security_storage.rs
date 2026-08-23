// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::*;
use chrono::Utc;

#[test]
fn device_round_trip_preserves_metadata_and_secret_record() {
    let root = std::env::temp_dir().join(format!("gganbu-storage-{}", std::process::id()));
    let _ = fs::remove_dir_all(&root);
    let store = DeviceStore::new(root.join("security.json"));
    let device = DeviceRecord::new(
        "device-1".into(),
        "Browser".into(),
        vec![1, 2, 3],
        Utc::now(),
    );
    store.save_devices(std::slice::from_ref(&device)).unwrap();
    let loaded = store.load_devices().unwrap();
    assert_eq!(loaded.len(), 1);
    assert_eq!(loaded[0].device_id, device.device_id);
    assert_eq!(&*loaded[0].opaque_password_file, &[1, 2, 3]);
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        assert_eq!(
            fs::metadata(store.path()).unwrap().permissions().mode() & 0o777,
            0o600
        );
    }
    let _ = fs::remove_dir_all(root);
}

#[test]
fn malformed_state_returns_actionable_corruption_error() {
    let root = std::env::temp_dir().join(format!("gganbu-corrupt-{}", std::process::id()));
    let _ = fs::create_dir_all(&root);
    let path = root.join("security.json");
    fs::write(&path, b"not-json").unwrap();
    let error = match DeviceStore::new(&path).load_devices() {
        Err(error) => error,
        Ok(_) => panic!("corrupt state loaded"),
    };
    assert!(error.to_string().contains("corrupt"));
    assert!(error.to_string().contains("pair devices again"));
    let _ = fs::remove_dir_all(root);
}
