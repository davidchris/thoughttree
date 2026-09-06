//! Machine-local state shared by every ThoughtTree filesystem writer.
use std::{fs, io, path::PathBuf};

use super::VaultError;

pub(super) fn root() -> Result<PathBuf, VaultError> {
    // All processes must use the same directory. The override also isolates tests.
    let root = match std::env::var_os("THOUGHTTREE_LOCAL_STATE_DIR") {
        Some(path) => PathBuf::from(path),
        None => dirs::data_local_dir()
            .ok_or_else(|| io::Error::other("Local application data directory is unavailable"))?
            .join("thoughttree")
            .join("project-state-v1"),
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

/// One stable inode for all local Project writes, independent of path aliases.
/// Never unlink it: existing waiters must continue to share the same inode.
pub(super) fn lock_project_writes() -> Result<fs::File, VaultError> {
    let file = fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(root()?.join("project-writes.lock"))?;
    fs2::FileExt::lock_exclusive(&file)?;
    // Closing this independently opened handle releases the OS advisory lock.
    Ok(file)
}
