//! Vault file commands for File nodes. Every path the frontend hands us is
//! Vault-relative and resolved by `thoughttree_core::vault::files`; no
//! frontend-supplied absolute path is opened directly. Filesystem work runs
//! on the blocking pool so the async runtime is never stalled by disk I/O.

use std::path::Path;
use std::sync::Arc;

use tauri::{AppHandle, State};
use tauri_plugin_dialog::DialogExt;
use thoughttree_core::vault::files::{
    self, AttachmentLimits, FilePreviewResponse, FileStat, OpenedVaultFile, PreviewCache,
    VaultFileError,
};

use crate::backend::config;
use crate::backend::state::AppState;

const OUTSIDE_VAULT: &str = "File must be inside the notes directory";

/// Stat result for the UI. Missing and invalid files are statuses, not errors,
/// so a File node can render its broken state.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "status", rename_all = "snake_case")]
pub(crate) enum VaultFileStatus {
    Ok {
        stat: FileStat,
        mime_type: String,
        name: String,
    },
    Missing,
    Invalid,
}

fn relativize(root: &Path, absolute: &Path) -> Result<String, String> {
    files::relativize_vault_path(root, absolute).map_err(|err| match err {
        VaultFileError::InvalidPath | VaultFileError::NotFound | VaultFileError::NotAFile => {
            OUTSIDE_VAULT.to_string()
        }
        other => other.to_string(),
    })
}

/// One open: stat, mime and name all come from the same handle.
fn status_for(root: &Path, relative: &str) -> Result<VaultFileStatus, String> {
    let status = OpenedVaultFile::open(root, relative).and_then(|opened| {
        Ok(VaultFileStatus::Ok {
            stat: opened.stat().clone(),
            mime_type: opened.mime_type()?,
            name: opened.name(),
        })
    });
    match status {
        Ok(status) => Ok(status),
        Err(VaultFileError::NotFound) => Ok(VaultFileStatus::Missing),
        Err(VaultFileError::InvalidPath | VaultFileError::NotAFile) => Ok(VaultFileStatus::Invalid),
        Err(other) => Err(other.to_string()),
    }
}

/// Stable prefixes (`too_large:`, `missing:`, `invalid:`) let the UI branch
/// without parsing free text.
fn preview_error(err: VaultFileError) -> String {
    match err {
        VaultFileError::TooLarge { limit } => {
            format!("too_large: file exceeds the attachment limit ({limit})")
        }
        VaultFileError::NotFound => "missing: file not found".to_string(),
        VaultFileError::InvalidPath => "invalid: path is outside the notes directory".to_string(),
        VaultFileError::NotAFile => "invalid: path is not a regular file".to_string(),
        VaultFileError::Io(io) => format!("io: {io}"),
    }
}

async fn blocking<T, F>(work: F) -> Result<T, String>
where
    T: Send + 'static,
    F: FnOnce() -> Result<T, String> + Send + 'static,
{
    tauri::async_runtime::spawn_blocking(work)
        .await
        .map_err(|err| format!("Blocking task failed: {err}"))?
}

#[tauri::command]
pub(crate) async fn pick_vault_file(app: AppHandle) -> Result<Option<String>, String> {
    let root = config::get_notes_directory_required(&app)?;
    blocking(move || {
        let picked = app
            .dialog()
            .file()
            .set_title("Add File from Notes")
            .set_directory(&root)
            .blocking_pick_file();
        match picked {
            None => Ok(None),
            Some(file) => {
                let absolute = file.into_path().map_err(|err| err.to_string())?;
                relativize(&root, &absolute).map(Some)
            }
        }
    })
    .await
}

#[tauri::command]
pub(crate) async fn resolve_dropped_file(
    app: AppHandle,
    absolute_path: String,
) -> Result<String, String> {
    let root = config::get_notes_directory_required(&app)?;
    blocking(move || relativize(&root, Path::new(&absolute_path))).await
}

#[tauri::command]
pub(crate) async fn stat_vault_file(
    app: AppHandle,
    path: String,
) -> Result<VaultFileStatus, String> {
    let root = config::get_notes_directory_required(&app)?;
    blocking(move || status_for(&root, &path)).await
}

#[tauri::command]
pub(crate) async fn read_vault_file_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    path: String,
) -> Result<FilePreviewResponse, String> {
    let root = config::get_notes_directory_required(&app)?;
    let cache: Arc<PreviewCache> = Arc::clone(&state.preview_cache);
    blocking(move || cache.preview(&root, &path).map_err(preview_error)).await
}

#[tauri::command]
pub(crate) async fn get_attachment_limits() -> AttachmentLimits {
    files::limits::attachment_limits()
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::{preview_error, relativize, status_for, VaultFileStatus, OUTSIDE_VAULT};
    use thoughttree_core::vault::files::VaultFileError;

    #[test]
    fn status_is_ok_with_stat_mime_and_name_for_a_vault_file() {
        let vault = tempfile::tempdir().unwrap();
        fs::write(vault.path().join("note.md"), "hello").unwrap();

        let status = status_for(vault.path(), "note.md").unwrap();

        match status {
            VaultFileStatus::Ok {
                stat,
                mime_type,
                name,
            } => {
                assert_eq!(stat.size, 5);
                assert_eq!(mime_type, "text/markdown");
                assert_eq!(name, "note.md");
            }
            other => panic!("expected ok status, got {other:?}"),
        }
    }

    #[test]
    fn status_sniffs_image_mime_from_the_opened_file() {
        let vault = tempfile::tempdir().unwrap();
        fs::write(vault.path().join("fake.png"), "not a png").unwrap();

        match status_for(vault.path(), "fake.png").unwrap() {
            VaultFileStatus::Ok { mime_type, .. } => {
                assert_eq!(mime_type, "application/octet-stream");
            }
            other => panic!("expected ok status, got {other:?}"),
        }
    }

    #[test]
    fn status_reports_missing_and_invalid_without_erroring() {
        let vault = tempfile::tempdir().unwrap();
        fs::create_dir_all(vault.path().join("dir")).unwrap();

        assert_eq!(
            status_for(vault.path(), "nope.md").unwrap(),
            VaultFileStatus::Missing
        );
        assert_eq!(
            status_for(vault.path(), "../nope.md").unwrap(),
            VaultFileStatus::Invalid
        );
        assert_eq!(
            status_for(vault.path(), "dir").unwrap(),
            VaultFileStatus::Invalid
        );
    }

    #[test]
    fn status_serializes_with_a_snake_case_tag() {
        assert_eq!(
            serde_json::to_value(VaultFileStatus::Missing).unwrap(),
            serde_json::json!({ "status": "missing" })
        );
    }

    #[test]
    fn dropped_files_outside_the_vault_get_the_user_facing_message() {
        let vault = tempfile::tempdir().unwrap();
        let outside = tempfile::tempdir().unwrap();
        fs::write(outside.path().join("a.md"), "x").unwrap();
        fs::write(vault.path().join("b.md"), "x").unwrap();

        assert_eq!(
            relativize(vault.path(), &outside.path().join("a.md")).unwrap_err(),
            OUTSIDE_VAULT
        );
        assert_eq!(
            relativize(vault.path(), &vault.path().join("b.md")).unwrap(),
            "b.md"
        );
    }

    #[test]
    fn preview_errors_carry_stable_prefixes() {
        assert!(preview_error(VaultFileError::TooLarge { limit: 5 }).starts_with("too_large:"));
        assert!(preview_error(VaultFileError::NotFound).starts_with("missing:"));
        assert!(preview_error(VaultFileError::InvalidPath).starts_with("invalid:"));
        assert!(preview_error(VaultFileError::NotAFile).starts_with("invalid:"));
    }
}
