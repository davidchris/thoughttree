use std::{
    fs,
    path::{Component, Path, PathBuf},
    time::UNIX_EPOCH,
};

use thoughttree_core::vault::{
    self, ProjectDoc, ProjectEntry, RecoveryEntry, Revision, VaultError,
};
use walkdir::WalkDir;

use crate::Desktop;

impl Desktop {
    pub fn project_path(&self, path: &Path) -> Result<PathBuf, VaultError> {
        let root = self.notes_directory().map_err(config_error)?;
        validate_project_path(&root, path)
    }

    pub fn load_project(&self, path: &Path) -> Result<ProjectDoc, VaultError> {
        vault::read_project_file(self.project_path(path)?)
    }

    pub fn save_project(
        &self,
        path: &Path,
        data: &str,
        base_revision: Option<&Revision>,
    ) -> Result<Revision, VaultError> {
        vault::guarded_write_file(self.project_path(path)?, data, base_revision)
    }

    pub fn save_project_copy(
        &self,
        path: &Path,
        data: &str,
    ) -> Result<(PathBuf, Revision), VaultError> {
        vault::write_project_copy(&self.project_path(path)?, data)
    }

    pub fn snapshot_project(&self, path: Option<&Path>, data: &str) -> Result<String, VaultError> {
        let source = path.map(|path| self.project_path(path)).transpose()?;
        vault::save_recovery_snapshot(source.as_deref(), data).map(|entry| entry.id)
    }

    // Recovery intentionally remains available without a configured or mounted
    // Vault. Its storage is owned by core and shared by both desktop frontends.
    pub fn list_project_recovery(&self) -> Result<Vec<RecoveryEntry>, VaultError> {
        vault::list_recovery_snapshots()
    }

    pub fn read_project_recovery(&self, id: &str) -> Result<String, VaultError> {
        vault::read_recovery_snapshot(id).map(|snapshot| snapshot.content)
    }

    pub fn list_projects(&self) -> Result<Vec<ProjectEntry>, String> {
        self.list_projects_with_root().map(|(_, entries)| entries)
    }

    /// Return the root used by this scan so a delayed UI result can be rejected
    /// when the configured Vault changes during filesystem traversal.
    pub fn list_projects_with_root(&self) -> Result<(PathBuf, Vec<ProjectEntry>), String> {
        let root = self.notes_directory()?;
        let mut entries = Vec::new();
        for entry in vault_files(&root) {
            if entry.path().extension().and_then(|s| s.to_str()) != Some("thoughttree") {
                continue;
            }
            let modified_epoch_ms = entry
                .metadata()
                .map_err(|e| e.to_string())?
                .modified()
                .map_err(|e| e.to_string())?
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64;
            entries.push(ProjectEntry {
                relative_path: relative_name(&root, entry.path()),
                modified_epoch_ms,
            });
        }
        entries.sort_by(|a, b| {
            b.modified_epoch_ms
                .cmp(&a.modified_epoch_ms)
                .then_with(|| a.relative_path.cmp(&b.relative_path))
        });
        Ok((root, entries))
    }

    pub fn search_files(&self, query: &str, limit: usize) -> Result<Vec<String>, String> {
        let root = self.notes_directory()?;
        let query = query.chars().take(100).collect::<String>().to_lowercase();
        Ok(vault_files(&root)
            .map(|entry| relative_name(&root, entry.path()))
            .filter(|path| path.to_lowercase().contains(&query))
            .take(limit)
            .collect())
    }

    pub fn add_recent_project(&self, path: &Path) -> Result<(), String> {
        let path = self
            .project_path(path)
            .map_err(|e| e.to_string())?
            .to_string_lossy()
            .into_owned();
        self.0.config.update(|config| {
            config.recent_projects.retain(|existing| existing != &path);
            config.recent_projects.insert(0, path);
            config.recent_projects.truncate(10);
        })
    }

    pub fn remove_recent_project(&self, path: &str) -> Result<(), String> {
        self.0
            .config
            .update(|config| config.recent_projects.retain(|existing| existing != path))
    }

    /// Match the existing chooser: drop unreadable entries without deleting
    /// their files. Perform this alongside listing on a background executor.
    pub fn prune_recent_projects(&self) -> Result<(), String> {
        let unreadable: Vec<_> = self
            .config()
            .recent_projects
            .into_iter()
            .filter(|path| self.load_project(Path::new(path)).is_err())
            .collect();
        if unreadable.is_empty() {
            return Ok(());
        }
        self.0.config.update(|config| {
            config
                .recent_projects
                .retain(|path| !unreadable.contains(path));
        })
    }

    /// Called only with a path returned by the user's native save dialog.
    pub fn export_markdown(&self, path: &Path, content: &str) -> Result<(), String> {
        fs::write(path, content).map_err(|e| format!("Failed to export Markdown: {e}"))
    }
}

fn config_error(message: String) -> VaultError {
    std::io::Error::other(message).into()
}

fn vault_files(root: &Path) -> impl Iterator<Item = walkdir::DirEntry> {
    WalkDir::new(root)
        .follow_links(false)
        .max_depth(20)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
}

fn relative_name(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .unwrap_or(path)
        .to_string_lossy()
        .replace('\\', "/")
}

/// Match the existing desktop boundary for absolute picker paths and allow
/// Vault-relative paths from the sidebar. Canonicalization rejects symlink
/// escapes, including a symlinked parent of a new Project file.
fn validate_project_path(root: &Path, supplied: &Path) -> Result<PathBuf, VaultError> {
    if supplied.as_os_str().is_empty()
        || supplied
            .components()
            .any(|part| matches!(part, Component::ParentDir))
    {
        return Err(VaultError::InvalidPath);
    }
    let root = fs::canonicalize(root)?;
    let path = if supplied.is_absolute() {
        supplied.to_owned()
    } else {
        root.join(supplied)
    };
    let path = if path.exists() {
        fs::canonicalize(path)?
    } else {
        let parent = path.parent().ok_or(VaultError::InvalidPath)?;
        let name = path.file_name().ok_or(VaultError::InvalidPath)?;
        fs::canonicalize(parent)?.join(name)
    };
    if !path.starts_with(&root) || path == root {
        return Err(VaultError::InvalidPath);
    }
    Ok(path)
}
