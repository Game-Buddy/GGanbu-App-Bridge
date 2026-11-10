// SPDX-FileCopyrightText: 2026 Game Buddy
// SPDX-License-Identifier: AGPL-3.0-only

use std::{
    fs::{self, File, OpenOptions},
    io::{self, Write},
    path::Path,
};

pub(crate) fn atomic_write(path: &Path, bytes: &[u8]) -> io::Result<()> {
    fs::create_dir_all(path.parent().unwrap_or(Path::new(".")))?;
    let temporary = path.with_extension("tmp");
    let mut options = OpenOptions::new();
    options.write(true).create(true).truncate(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        options.mode(0o600);
    }
    let mut file = options.open(&temporary)?;
    file.write_all(bytes)?;
    file.sync_all()?;
    drop(file);
    fs::rename(&temporary, path)
}

pub(crate) fn lock_instance(directory: &Path) -> io::Result<File> {
    fs::create_dir_all(directory)?;
    let file = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(directory.join("instance.lock"))?;
    file.try_lock().map_err(io::Error::other)?;
    Ok(file)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_second_instance_cannot_lock_the_same_directory() {
        let root = std::env::temp_dir().join(format!("gganbu-instance-{}", std::process::id()));
        let first = lock_instance(&root).unwrap();
        assert!(lock_instance(&root).is_err());
        drop(first);
        assert!(lock_instance(&root).is_ok());
        let _ = fs::remove_dir_all(root);
    }
}
