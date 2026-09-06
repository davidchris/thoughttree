//! Independent, immutable snapshots of content observed from ThoughtTree edits.
use std::{
    fs,
    io::Write,
    path::{Path, PathBuf},
    time::{SystemTime, UNIX_EPOCH},
};

use super::{local_state, revision_for_content, temp_sibling_path, VaultError};

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoveryEntry {
    pub id: String,
    pub source_path: Option<PathBuf>,
    pub created_epoch_ms: u64,
}

#[derive(serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RecoverySnapshot {
    #[serde(flatten)]
    pub entry: RecoveryEntry,
    pub content: String,
}

fn directory() -> Result<PathBuf, VaultError> {
    let path = local_state::root()?.join("recovery");
    fs::create_dir_all(&path)?;
    Ok(path)
}

pub fn save_recovery_snapshot(
    source: Option<&Path>,
    content: &str,
) -> Result<RecoveryEntry, VaultError> {
    // Identical content from the same source shares a snapshot. No pruning: an
    // external overwrite cannot remove an earlier completed ThoughtTree edit.
    let identity = serde_json::to_vec(&(source, content)).map_err(std::io::Error::other)?;
    let id = revision_for_content(&identity).0;
    let path = directory()?.join(format!("{id}.json"));
    if path.exists() {
        return Ok(read_recovery_snapshot(&id)?.entry);
    }
    let snapshot = RecoverySnapshot {
        entry: RecoveryEntry {
            id,
            source_path: source.map(Path::to_path_buf),
            created_epoch_ms: SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .unwrap_or_default()
                .as_millis() as u64,
        },
        content: content.to_owned(),
    };
    let bytes = serde_json::to_vec(&snapshot).map_err(std::io::Error::other)?;
    let temp = temp_sibling_path(&path);
    let result = (|| -> Result<(), VaultError> {
        let mut file = fs::OpenOptions::new()
            .create_new(true)
            .write(true)
            .open(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, &path)?;
        #[cfg(unix)]
        fs::File::open(path.parent().unwrap())?.sync_all()?;
        Ok(())
    })();
    if let Err(err) = result {
        let _ = fs::remove_file(temp);
        return Err(err);
    }
    Ok(snapshot.entry)
}

pub fn read_recovery_snapshot(id: &str) -> Result<RecoverySnapshot, VaultError> {
    if id.len() != 64 || !id.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(VaultError::InvalidPath);
    }
    let bytes = fs::read(directory()?.join(format!("{id}.json")))?;
    let snapshot: RecoverySnapshot =
        serde_json::from_slice(&bytes).map_err(std::io::Error::other)?;
    let identity = serde_json::to_vec(&(&snapshot.entry.source_path, &snapshot.content))
        .map_err(std::io::Error::other)?;
    if snapshot.entry.id != id || revision_for_content(&identity).0 != id {
        return Err(std::io::Error::other("Recovery snapshot failed its content check").into());
    }
    Ok(snapshot)
}

pub fn list_recovery_snapshots() -> Result<Vec<RecoveryEntry>, VaultError> {
    let mut entries = Vec::new();
    for file in fs::read_dir(directory()?)? {
        let file = file?;
        if !file.file_type()?.is_file() {
            continue;
        }
        let path = file.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("json") {
            continue;
        }
        if let Some(id) = path.file_stem().and_then(|stem| stem.to_str()) {
            // A damaged snapshot must not hide the other recoverable versions.
            if let Ok(snapshot) = read_recovery_snapshot(id) {
                entries.push(snapshot.entry);
            }
        }
    }
    entries.sort_by(|a, b| {
        b.created_epoch_ms
            .cmp(&a.created_epoch_ms)
            .then_with(|| a.id.cmp(&b.id))
    });
    Ok(entries)
}
