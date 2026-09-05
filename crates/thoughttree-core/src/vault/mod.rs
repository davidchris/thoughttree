//! Guarded project writes per ADR 0004:
//! `docs/adr/0004-guarded-project-writes.md`.
//! Local files write to a temp sibling and atomically rename into place after
//! comparing the caller's last-read revision with the current file content hash.
//! The compare and the rename run under an exclusive advisory lock on a sibling
//! `.<name>.lock` file so a concurrent guarded writer cannot slip in between.

use std::fs;
use std::io::Write;
use std::path::{Component, Path, PathBuf};
use std::time::UNIX_EPOCH;

use async_trait::async_trait;
use sha2::{Digest, Sha256};
use uuid::Uuid;

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct Revision(pub String);

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectDoc {
    pub content: String,
    pub revision: Revision,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
pub struct ProjectEntry {
    pub relative_path: String,
    pub modified_epoch_ms: u64,
}

#[derive(Debug, thiserror::Error)]
pub enum VaultError {
    #[error("project not found")]
    NotFound,
    #[error("stale revision: file changed since read")]
    Stale { current: Revision },
    #[error("path escapes vault root")]
    InvalidPath,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

#[async_trait]
pub trait VaultStorage: Send + Sync {
    async fn list(&self) -> Result<Vec<ProjectEntry>, VaultError>;
    async fn read(&self, relative_path: &str) -> Result<ProjectDoc, VaultError>;
    async fn write(
        &self,
        relative_path: &str,
        content: &str,
        expected: Option<&Revision>,
    ) -> Result<Revision, VaultError>;
}

pub struct LocalFsVault {
    root: PathBuf,
}

impl LocalFsVault {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait]
impl VaultStorage for LocalFsVault {
    async fn list(&self) -> Result<Vec<ProjectEntry>, VaultError> {
        list_projects_under_root(&self.root)
    }

    async fn read(&self, relative_path: &str) -> Result<ProjectDoc, VaultError> {
        let path = validate_relative_path(&self.root, relative_path)?;
        read_project_file(&path)
    }

    async fn write(
        &self,
        relative_path: &str,
        content: &str,
        expected: Option<&Revision>,
    ) -> Result<Revision, VaultError> {
        let path = validate_relative_path(&self.root, relative_path)?;
        guarded_write_file(&path, content, expected)
    }
}

pub fn read_project_file(path: impl AsRef<Path>) -> Result<ProjectDoc, VaultError> {
    let path = validate_absolute_file_path(path.as_ref())?;
    let content = fs::read_to_string(&path).map_err(map_not_found)?;
    Ok(ProjectDoc {
        revision: revision_for_content(content.as_bytes()),
        content,
    })
}

pub fn guarded_write_file(
    path: impl AsRef<Path>,
    content: &str,
    expected: Option<&Revision>,
) -> Result<Revision, VaultError> {
    guarded_write_file_inner(path.as_ref(), content, expected, |_temp, _target| Ok(()))
}

fn guarded_write_file_inner<F>(
    path: &Path,
    content: &str,
    expected: Option<&Revision>,
    before_rename: F,
) -> Result<Revision, VaultError>
where
    F: FnOnce(&Path, &Path) -> Result<(), std::io::Error>,
{
    let path = validate_absolute_file_path(path)?;
    let parent = path.parent().ok_or(VaultError::InvalidPath)?;
    // Held until this function returns: revision check and replacement are one
    // critical section, so a concurrent guarded writer waits and then sees the
    // new revision instead of overwriting it.
    let _write_lock = WriteLock::acquire(parent, &path)?;
    let current = read_existing_file(&path)?;

    if let Some(expected) = expected {
        let current = current.ok_or(VaultError::NotFound)?;
        if current.revision != *expected {
            return Err(VaultError::Stale {
                current: current.revision,
            });
        }
    }

    let temp_path = temp_sibling_path(parent, &path);
    let write_result = (|| -> Result<(), VaultError> {
        let mut temp_file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp_path)?;
        temp_file.write_all(content.as_bytes())?;
        temp_file.sync_all()?;
        drop(temp_file);

        before_rename(&temp_path, &path)?;
        fs::rename(&temp_path, &path)?;
        Ok(())
    })();

    if let Err(err) = write_result {
        let _ = fs::remove_file(&temp_path);
        return Err(err);
    }

    Ok(revision_for_content(content.as_bytes()))
}

/// Exclusive advisory lock on a persistent sibling lock file. The lock file is
/// never removed: unlinking it would let a third writer create a fresh inode
/// and bypass a waiter still blocked on the old one.
struct WriteLock {
    file: fs::File,
}

impl WriteLock {
    fn acquire(parent: &Path, target: &Path) -> Result<Self, VaultError> {
        let file = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(lock_sibling_path(parent, target))?;
        file.lock()?;
        Ok(Self { file })
    }
}

impl Drop for WriteLock {
    fn drop(&mut self) {
        let _ = self.file.unlock();
    }
}

fn read_existing_file(path: &Path) -> Result<Option<ProjectDoc>, VaultError> {
    match fs::read_to_string(path) {
        Ok(content) => Ok(Some(ProjectDoc {
            revision: revision_for_content(content.as_bytes()),
            content,
        })),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(err) => Err(VaultError::Io(err)),
    }
}

fn list_projects_under_root(root: &Path) -> Result<Vec<ProjectEntry>, VaultError> {
    let canonical_root = fs::canonicalize(root).map_err(map_not_found)?;
    let mut stack = vec![canonical_root.clone()];
    let mut entries = Vec::new();

    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(&dir)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            if file_type.is_symlink() {
                continue;
            }

            let path = entry.path();
            if file_type.is_dir() {
                stack.push(path);
                continue;
            }
            if !file_type.is_file()
                || path.extension().and_then(|ext| ext.to_str()) != Some("thoughttree")
            {
                continue;
            }

            let metadata = entry.metadata()?;
            let modified_epoch_ms = metadata
                .modified()
                .ok()
                .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
                .map(|duration| duration.as_millis() as u64)
                .unwrap_or(0);
            let relative_path = path
                .strip_prefix(&canonical_root)
                .map_err(|_| VaultError::InvalidPath)?
                .to_string_lossy()
                .replace('\\', "/");
            entries.push(ProjectEntry {
                relative_path,
                modified_epoch_ms,
            });
        }
    }

    entries.sort_by(|a, b| a.relative_path.cmp(&b.relative_path));
    Ok(entries)
}

fn validate_relative_path(root: &Path, relative_path: &str) -> Result<PathBuf, VaultError> {
    let relative = Path::new(relative_path);
    if relative.as_os_str().is_empty() || relative.is_absolute() {
        return Err(VaultError::InvalidPath);
    }
    if relative
        .components()
        .any(|component| !matches!(component, Component::Normal(_)))
    {
        return Err(VaultError::InvalidPath);
    }

    let canonical_root = fs::canonicalize(root).map_err(map_not_found)?;
    let parent = relative.parent().unwrap_or_else(|| Path::new(""));
    let file_name = relative.file_name().ok_or(VaultError::InvalidPath)?;
    let canonical_parent = fs::canonicalize(canonical_root.join(parent)).map_err(map_not_found)?;
    if !canonical_parent.starts_with(&canonical_root) {
        return Err(VaultError::InvalidPath);
    }

    Ok(canonical_parent.join(file_name))
}

fn validate_absolute_file_path(path: &Path) -> Result<PathBuf, VaultError> {
    if !path.is_absolute() {
        return Err(VaultError::InvalidPath);
    }

    if path.exists() {
        return fs::canonicalize(path).map_err(map_not_found);
    }

    let parent = path.parent().ok_or(VaultError::InvalidPath)?;
    let file_name = path.file_name().ok_or(VaultError::InvalidPath)?;
    let canonical_parent = fs::canonicalize(parent).map_err(map_not_found)?;
    Ok(canonical_parent.join(file_name))
}

fn sibling_file_name(target: &Path) -> &str {
    target
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("project.thoughttree")
}

fn temp_sibling_path(parent: &Path, target: &Path) -> PathBuf {
    parent.join(format!(
        ".{}.{}.tmp",
        sibling_file_name(target),
        Uuid::new_v4()
    ))
}

fn lock_sibling_path(parent: &Path, target: &Path) -> PathBuf {
    parent.join(format!(".{}.lock", sibling_file_name(target)))
}

fn revision_for_content(content: &[u8]) -> Revision {
    let digest = Sha256::digest(content);
    let mut hex = String::with_capacity(digest.len() * 2);
    for byte in digest {
        hex.push_str(&format!("{byte:02x}"));
    }
    Revision(hex)
}

fn map_not_found(err: std::io::Error) -> VaultError {
    if err.kind() == std::io::ErrorKind::NotFound {
        VaultError::NotFound
    } else {
        VaultError::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::sync::mpsc;
    use std::thread;
    use std::time::Duration;

    use tempfile::tempdir;
    use tokio::runtime::Builder;

    use super::{
        guarded_write_file_inner, read_project_file, LocalFsVault, Revision, VaultError,
        VaultStorage,
    };

    #[test]
    fn guarded_write_succeeds_on_matching_revision() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("project.thoughttree");
        fs::write(&path, "before").unwrap();

        let initial = read_project_file(&path).unwrap();
        let next = super::guarded_write_file(&path, "after", Some(&initial.revision)).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "after");
        assert_eq!(read_project_file(&path).unwrap().revision, next);
    }

    #[test]
    fn stale_write_returns_current_revision_and_leaves_file_untouched() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("project.thoughttree");
        fs::write(&path, "before").unwrap();
        let initial = read_project_file(&path).unwrap();
        fs::write(&path, "concurrent").unwrap();

        let err = super::guarded_write_file(&path, "after", Some(&initial.revision)).unwrap_err();

        match err {
            VaultError::Stale { current } => {
                assert_eq!(current, read_project_file(&path).unwrap().revision);
            }
            other => panic!("expected stale revision error, got {other:?}"),
        }
        assert_eq!(fs::read_to_string(&path).unwrap(), "concurrent");
    }

    #[test]
    fn concurrent_guarded_writer_blocks_until_replacement_and_then_sees_stale() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("project.thoughttree");
        fs::write(&path, "before").unwrap();
        let initial = read_project_file(&path).unwrap();

        // Force the interleaving: a second guarded writer starts after the first
        // writer has validated the revision but before it renames into place.
        let (started_tx, started_rx) = mpsc::channel();
        let (done_tx, done_rx) = mpsc::channel();
        let mut concurrent = None;
        let concurrent_slot = &mut concurrent;
        let done_rx = &done_rx;
        let expected = initial.revision.clone();
        let first = guarded_write_file_inner(
            &path,
            "after",
            Some(&initial.revision),
            move |_temp_path, target_path| {
                let target = target_path.to_path_buf();
                let handle = thread::spawn(move || {
                    started_tx.send(()).unwrap();
                    let result = super::guarded_write_file(&target, "concurrent", Some(&expected));
                    let _ = done_tx.send(());
                    result
                });
                started_rx.recv().unwrap();
                // The second writer must be blocked on the lock, not finished.
                assert!(done_rx.recv_timeout(Duration::from_millis(200)).is_err());
                assert_eq!(fs::read_to_string(target_path)?, "before");
                *concurrent_slot = Some(handle);
                Ok(())
            },
        )
        .unwrap();

        let second = concurrent.unwrap().join().unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "after");
        assert_eq!(read_project_file(&path).unwrap().revision, first);
        match second {
            Err(VaultError::Stale { current }) => assert_eq!(current, first),
            other => panic!("expected stale revision error, got {other:?}"),
        }
    }

    #[test]
    fn concurrent_guarded_writers_with_reload_never_lose_an_append() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("project.thoughttree");
        fs::write(&path, "0").unwrap();

        // Each writer follows the reload-and-reapply flow from ADR 0004.
        let handles: Vec<_> = (1..=8)
            .map(|i| {
                let path = path.clone();
                thread::spawn(move || loop {
                    let current = read_project_file(&path).unwrap();
                    let next = format!("{}{i}", current.content);
                    match super::guarded_write_file(&path, &next, Some(&current.revision)) {
                        Ok(_) => break,
                        Err(VaultError::Stale { .. }) => continue,
                        Err(other) => panic!("unexpected error: {other:?}"),
                    }
                })
            })
            .collect();
        for handle in handles {
            handle.join().unwrap();
        }

        // Every writer appended exactly once; no append was overwritten.
        let content = fs::read_to_string(&path).unwrap();
        let mut digits: Vec<char> = content.chars().collect();
        digits.sort_unstable();
        assert_eq!(digits.into_iter().collect::<String>(), "012345678");
    }

    #[test]
    fn guarded_write_uses_temp_file_then_rename() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("project.thoughttree");
        fs::write(&path, "before").unwrap();

        let revision = guarded_write_file_inner(&path, "after", None, |temp_path, target_path| {
            assert_eq!(fs::read_to_string(target_path)?, "before");
            assert_eq!(fs::read_to_string(temp_path)?, "after");
            Ok(())
        })
        .unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "after");
        assert_eq!(read_project_file(&path).unwrap().revision, revision);
    }

    #[test]
    fn local_fs_vault_rejects_traversal_paths() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        let vault = LocalFsVault::new(dir.path().to_path_buf());
        let runtime = Builder::new_current_thread().enable_all().build().unwrap();

        let err = runtime
            .block_on(async { vault.read("../escape.thoughttree").await })
            .unwrap_err();

        assert!(matches!(err, VaultError::InvalidPath));
    }

    #[test]
    fn local_fs_vault_lists_project_files_under_root() {
        let dir = tempdir().unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        fs::write(dir.path().join("nested/project.thoughttree"), "ok").unwrap();
        fs::write(dir.path().join("nested/ignore.txt"), "nope").unwrap();
        let vault = LocalFsVault::new(dir.path().to_path_buf());
        let runtime = Builder::new_current_thread().enable_all().build().unwrap();

        let entries = runtime.block_on(async { vault.list().await }).unwrap();

        assert_eq!(entries.len(), 1);
        assert_eq!(entries[0].relative_path, "nested/project.thoughttree");
        assert!(entries[0].modified_epoch_ms > 0);
    }

    #[test]
    fn guarded_write_can_create_new_file_unconditionally() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("new-project.thoughttree");

        let revision = super::guarded_write_file(&path, "fresh", None).unwrap();

        assert_eq!(fs::read_to_string(&path).unwrap(), "fresh");
        assert_eq!(read_project_file(&path).unwrap().revision, revision);
    }

    #[test]
    fn guarded_write_on_missing_file_with_expected_revision_returns_not_found() {
        let dir = tempdir().unwrap();
        let path = dir.path().join("missing.thoughttree");

        let err =
            super::guarded_write_file(&path, "fresh", Some(&Revision("old".into()))).unwrap_err();

        assert!(matches!(err, VaultError::NotFound));
    }
}
