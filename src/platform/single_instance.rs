use std::{
    fs::{self, File, TryLockError},
    path::Path,
};

use anyhow::{Context, Result};

/// Keep the returned file open for the entire application lifetime.
pub fn acquire(path: &Path) -> Result<Option<File>> {
    fs::create_dir_all(path.parent().context("Missing instance lock directory")?)
        .context("Unable to create the instance lock directory")?;
    let file = File::options()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(path)
        .context("Unable to open the instance lock")?;

    // Never delete the file: another process could still hold a lock on it.
    // The operating system releases the lock when the process exits, even after a crash.
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(TryLockError::WouldBlock) => Ok(None),
        Err(TryLockError::Error(error)) => {
            Err(error).context("Unable to acquire the instance lock")
        }
    }
}
