use std::path::Path;

use thoughttree_core::vault::files::{
    self, AttachmentLimits, FilePreviewResponse, FileStat, OpenedVaultFile, VaultFileError,
};

use crate::Desktop;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VaultFileStatus {
    Ok {
        stat: FileStat,
        mime_type: String,
        name: String,
    },
    Missing,
    Invalid,
}

impl Desktop {
    pub fn resolve_dropped_file(&self, absolute: &Path) -> Result<String, String> {
        files::relativize_vault_path(&self.notes_directory()?, absolute).map_err(
            |error| match error {
                VaultFileError::InvalidPath
                | VaultFileError::NotFound
                | VaultFileError::NotAFile => "File must be inside the notes directory".to_string(),
                other => other.to_string(),
            },
        )
    }

    pub fn stat_vault_file(&self, relative: &str) -> Result<VaultFileStatus, String> {
        let root = self.notes_directory()?;
        let status = OpenedVaultFile::open(&root, relative).and_then(|opened| {
            Ok(VaultFileStatus::Ok {
                stat: opened.stat().clone(),
                mime_type: opened.mime_type()?,
                name: opened.name(),
            })
        });
        match status {
            Ok(status) => Ok(status),
            Err(VaultFileError::NotFound) => Ok(VaultFileStatus::Missing),
            Err(VaultFileError::InvalidPath | VaultFileError::NotAFile) => {
                Ok(VaultFileStatus::Invalid)
            }
            Err(other) => Err(other.to_string()),
        }
    }

    pub fn read_vault_file_preview(&self, relative: &str) -> Result<FilePreviewResponse, String> {
        self.0
            .preview_cache
            .preview(&self.notes_directory()?, relative)
            .map_err(preview_error)
    }

    /// The checks a prompt applies, without decoding or retaining a preview.
    /// Errors read like those of [`Self::read_vault_file_preview`].
    pub fn check_vault_file(&self, relative: &str) -> Result<(), String> {
        files::check_vault_file(&self.notes_directory()?, relative).map_err(preview_error)
    }

    pub fn attachment_limits(&self) -> AttachmentLimits {
        files::limits::attachment_limits()
    }
}

fn preview_error(error: VaultFileError) -> String {
    match error {
        VaultFileError::TooLarge { limit } => {
            format!("too_large: file exceeds the attachment limit ({limit})")
        }
        VaultFileError::NotFound => "missing: file not found".to_string(),
        VaultFileError::InvalidPath => "invalid: path is outside the notes directory".to_string(),
        VaultFileError::NotAFile => "invalid: path is not a regular file".to_string(),
        VaultFileError::Io(error) => format!("io: {error}"),
    }
}
