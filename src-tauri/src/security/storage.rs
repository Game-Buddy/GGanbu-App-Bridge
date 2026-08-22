// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use super::DeviceRecord;
use base64::{Engine as _, engine::general_purpose::STANDARD};
use rand::{RngCore, rngs::OsRng};
use serde::{Deserialize, Serialize};
use std::{
    fs::{self, OpenOptions},
    io::{self, Write},
    path::{Path, PathBuf},
};
use zeroize::Zeroizing;

const SCHEMA_VERSION: u32 = 1;
const KEYRING_SERVICE: &str = "gganbu-bridge";
const KEYRING_USER: &str = "opaque-server-master-secret";

#[derive(Debug)]
pub enum StorageError {
    Io { path: PathBuf, source: io::Error },
    Corrupt { path: PathBuf, reason: String },
    Keyring(String),
    SecretEncoding(String),
}
impl std::fmt::Display for StorageError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io { path, source } => write!(
                f,
                "unable to access security data at {}: {source}",
                path.display()
            ),
            Self::Corrupt { path, reason } => write!(
                f,
                "security data at {} is corrupt ({reason}); remove it and pair devices again",
                path.display()
            ),
            Self::Keyring(reason) => {
                write!(f, "operating-system credential store unavailable: {reason}")
            }
            Self::SecretEncoding(reason) => {
                write!(f, "credential-store secret has invalid encoding: {reason}")
            }
        }
    }
}
impl std::error::Error for StorageError {}

#[derive(Serialize, Deserialize)]
struct FileFormat {
    schema_version: u32,
    devices: Vec<PersistedDevice>,
}
#[derive(Serialize, Deserialize)]
struct PersistedDevice {
    device_id: String,
    display_name: String,
    opaque_password_file: String,
    created_at: chrono::DateTime<chrono::Utc>,
    last_seen_at: chrono::DateTime<chrono::Utc>,
    revoked_at: Option<chrono::DateTime<chrono::Utc>>,
}

#[derive(Clone)]
pub struct DeviceStore {
    path: PathBuf,
}
impl DeviceStore {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }
    pub fn path(&self) -> &Path {
        &self.path
    }
    pub fn load_devices(&self) -> Result<Vec<DeviceRecord>, StorageError> {
        if !self.path.exists() {
            return Ok(Vec::new());
        }
        let bytes = fs::read(&self.path).map_err(|source| StorageError::Io {
            path: self.path.clone(),
            source,
        })?;
        let file: FileFormat =
            serde_json::from_slice(&bytes).map_err(|error| StorageError::Corrupt {
                path: self.path.clone(),
                reason: error.to_string(),
            })?;
        if file.schema_version != SCHEMA_VERSION {
            return Err(StorageError::Corrupt {
                path: self.path.clone(),
                reason: format!("unsupported schema version {}", file.schema_version),
            });
        }
        file.devices
            .into_iter()
            .map(|device| {
                let secret = STANDARD
                    .decode(device.opaque_password_file)
                    .map_err(|error| StorageError::Corrupt {
                        path: self.path.clone(),
                        reason: error.to_string(),
                    })?;
                Ok(DeviceRecord {
                    device_id: device.device_id,
                    display_name: device.display_name,
                    opaque_password_file: Zeroizing::new(secret),
                    created_at: device.created_at,
                    last_seen_at: device.last_seen_at,
                    revoked_at: device.revoked_at,
                })
            })
            .collect()
    }
    pub fn save_devices(&self, devices: &[DeviceRecord]) -> Result<(), StorageError> {
        let file = FileFormat {
            schema_version: SCHEMA_VERSION,
            devices: devices
                .iter()
                .map(|device| PersistedDevice {
                    device_id: device.device_id.clone(),
                    display_name: device.display_name.clone(),
                    opaque_password_file: STANDARD.encode(&*device.opaque_password_file),
                    created_at: device.created_at,
                    last_seen_at: device.last_seen_at,
                    revoked_at: device.revoked_at,
                })
                .collect(),
        };
        let bytes =
            serde_json::to_vec_pretty(&file).expect("device metadata serialization cannot fail");
        let parent = self.path.parent().unwrap_or_else(|| Path::new("."));
        fs::create_dir_all(parent).map_err(|source| StorageError::Io {
            path: parent.to_path_buf(),
            source,
        })?;
        let temp = self.path.with_extension("json.tmp");
        let mut options = OpenOptions::new();
        options.write(true).create(true).truncate(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let mut file = options.open(&temp).map_err(|source| StorageError::Io {
            path: temp.clone(),
            source,
        })?;
        file.write_all(&bytes).map_err(|source| StorageError::Io {
            path: temp.clone(),
            source,
        })?;
        file.sync_all().map_err(|source| StorageError::Io {
            path: temp.clone(),
            source,
        })?;
        fs::rename(&temp, &self.path).map_err(|source| StorageError::Io {
            path: self.path.clone(),
            source,
        })
    }
    pub fn load_or_create_server_secret(&self) -> Result<Zeroizing<Vec<u8>>, StorageError> {
        let entry = keyring::Entry::new(KEYRING_SERVICE, KEYRING_USER)
            .map_err(|error| StorageError::Keyring(error.to_string()))?;
        match entry.get_password() {
            Ok(value) => {
                let secret = STANDARD
                    .decode(value)
                    .map_err(|error| StorageError::SecretEncoding(error.to_string()))?;
                if secret.len() != 32 {
                    return Err(StorageError::SecretEncoding(
                        "expected a 32-byte master secret".to_owned(),
                    ));
                }
                Ok(Zeroizing::new(secret))
            }
            Err(keyring::Error::NoEntry) => {
                let mut secret = Zeroizing::new(vec![0_u8; 32]);
                OsRng.fill_bytes(&mut secret);
                entry
                    .set_password(&STANDARD.encode(&*secret))
                    .map_err(|error| StorageError::Keyring(error.to_string()))?;
                Ok(secret)
            }
            Err(error) => Err(StorageError::Keyring(error.to_string())),
        }
    }
}

#[cfg(test)]
mod tests {
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
}
