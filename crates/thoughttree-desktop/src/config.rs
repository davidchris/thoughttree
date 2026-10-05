use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

pub use thoughttree_core::config::Config;
use thoughttree_core::config::{read, ConfigWriter};

/// The config this session uses. Other frontends may commit to the same file
/// at any time. A local write applies only this session's change to the cache;
/// everything else the other frontend committed arrives through `reload`, the
/// one place the workspace reconciles it. The Vault arrives only through
/// `adopt_vault`, so the workspace can run its guarded Vault transition.
pub(crate) struct ConfigStore {
    directory: PathBuf,
    value: Mutex<Config>,
}

impl ConfigStore {
    pub(crate) fn open(directory: PathBuf) -> Result<Self, String> {
        let value = read(&directory)?;
        Ok(Self {
            directory,
            value: Mutex::new(value),
        })
    }

    pub(crate) fn get(&self) -> Config {
        self.value.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    /// `change` runs twice: on the file contents and on the cache.
    pub(crate) fn update(&self, change: impl Fn(&mut Config)) -> Result<(), String> {
        self.update_if(|_| true, change).map(drop)
    }

    /// Commits `change` only when `condition` holds, and reports whether it
    /// did. The decision runs once, under the file lock, so it is ordered
    /// against every other writer and the file and cache always agree.
    pub(crate) fn update_if(
        &self,
        condition: impl FnOnce(&Config) -> bool,
        change: impl Fn(&mut Config),
    ) -> Result<bool, String> {
        let mut writer = ConfigWriter::lock(&self.directory)?;
        if !condition(&writer.config) {
            return Ok(false);
        }
        change(&mut writer.config);
        writer.save()?;
        // Rendering reads the last committed value without waiting for a
        // different writer, disk access, or fsync. Publish before releasing
        // the file lock so concurrent updates cannot publish out of order.
        change(&mut self.value.lock().unwrap_or_else(|e| e.into_inner()));
        Ok(true)
    }

    pub(crate) fn set_vault(&self, path: PathBuf) -> Result<(), String> {
        self.update(|config| config.notes_directory = Some(path.clone()))
    }

    /// Adopts the Vault another frontend committed without writing it back,
    /// so a newer choice made meanwhile is never reverted.
    pub(crate) fn adopt_vault(&self, expected: &Path) -> Result<(), String> {
        let writer = ConfigWriter::lock(&self.directory)?;
        if writer.config.notes_directory.as_deref() != Some(expected) {
            return Err("The notes directory changed again in the other app. \
                 Switch back to this window to use it."
                .into());
        }
        self.value
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .notes_directory = Some(expected.to_owned());
        Ok(())
    }

    /// Adopts what other frontends committed and returns the file contents,
    /// including a Vault this session has not adopted yet.
    pub(crate) fn reload(&self) -> Result<Config, String> {
        // Read under the lock so a concurrent local commit cannot be replaced
        // by an older read.
        let writer = ConfigWriter::lock(&self.directory)?;
        let mut value = self.value.lock().unwrap_or_else(|e| e.into_inner());
        let vault = value.notes_directory.take();
        *value = Config {
            notes_directory: vault,
            ..writer.config.clone()
        };
        Ok(writer.config)
    }
}
