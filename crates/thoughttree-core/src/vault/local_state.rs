//! Machine-local state shared by every ThoughtTree filesystem writer.
use std::{fs, io, path::PathBuf};

use super::VaultError;

pub(super) fn root() -> Result<PathBuf, VaultError> {
    // All processes must use the same directory. The override also isolates tests.
    let root = match std::env::var_os("THOUGHTTREE_LOCAL_STATE_DIR") {
        Some(path) => PathBuf::from(path),
        None => default_root()?,
    };
    if !root.is_absolute() {
        return Err(io::Error::other("ThoughtTree local state directory must be absolute").into());
    }
    let mut builder = fs::DirBuilder::new();
    builder.recursive(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::DirBuilderExt;
        builder.mode(0o700);
    }
    builder.create(&root)?;
    Ok(root)
}

#[cfg(not(test))]
fn default_root() -> Result<PathBuf, VaultError> {
    Ok(dirs::data_local_dir()
        .ok_or_else(|| io::Error::other("Local application data directory is unavailable"))?
        .join("thoughttree")
        .join("project-state-v1"))
}

/// Unit tests never write into the user's real Recovery store.
#[cfg(test)]
fn default_root() -> Result<PathBuf, VaultError> {
    static ROOT: std::sync::OnceLock<tempfile::TempDir> = std::sync::OnceLock::new();
    Ok(ROOT
        .get_or_init(|| tempfile::tempdir().expect("test local state directory"))
        .path()
        .to_path_buf())
}

/// One stable inode for all local Project writes, independent of path aliases.
/// Never unlink it: existing waiters must continue to share the same inode.
pub(super) fn lock_project_writes() -> Result<fs::File, VaultError> {
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root()?.join("project-writes.lock"))?;
    file.lock()?;
    // Closing this independently opened handle releases the OS advisory lock.
    Ok(file)
}

#[cfg(test)]
mod tests {
    #[test]
    fn unit_tests_do_not_use_the_real_local_state_directory() {
        if std::env::var_os("THOUGHTTREE_LOCAL_STATE_DIR").is_some() {
            return;
        }
        let real = dirs::data_local_dir().unwrap().join("thoughttree");
        assert!(!super::root().unwrap().starts_with(real));
    }
}
