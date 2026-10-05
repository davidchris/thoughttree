use std::{
    path::{Path, PathBuf},
    sync::Mutex,
};

pub use thoughttree_core::config::Config;
use thoughttree_core::config::{read, ConfigWriter};

/// The config this session uses. Other frontends may commit to the same file
/// at any time; their settings arrive on the next write or `reload`. The Vault
/// is the exception: it changes only through `set_vault` or `adopt_vault`, so
/// the workspace can run its guarded Vault transition.
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

    pub(crate) fn update(&self, change: impl FnOnce(&mut Config)) -> Result<(), String> {
        self.update_if(|config| {
            change(config);
            true
        })
    }

    /// Commits only when `change` returns true. The decision runs under the
    /// file lock, so it is ordered against every other writer.
    pub(crate) fn update_if(&self, change: impl FnOnce(&mut Config) -> bool) -> Result<(), String> {
        self.commit(change, false)
    }

    pub(crate) fn set_vault(&self, path: PathBuf) -> Result<(), String> {
        self.commit(
            |config| {
                config.notes_directory = Some(path);
                true
            },
            true,
        )
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
        self.publish(writer.config.clone(), true);
        Ok(())
    }

    /// Adopts what other frontends committed and returns the file contents,
    /// including a Vault this session has not adopted yet.
    pub(crate) fn reload(&self) -> Result<Config, String> {
        // Read under the lock so a concurrent local commit cannot be replaced
        // by an older read.
        let writer = ConfigWriter::lock(&self.directory)?;
        self.publish(writer.config.clone(), false);
        Ok(writer.config)
    }

    fn commit(
        &self,
        change: impl FnOnce(&mut Config) -> bool,
        adopt_vault: bool,
    ) -> Result<(), String> {
        let mut writer = ConfigWriter::lock(&self.directory)?;
        if !change(&mut writer.config) {
            return Ok(());
        }
        writer.save()?;
        // Rendering reads the last committed value without waiting for a
        // different writer, disk access, or fsync. Publish before releasing
        // the file lock so concurrent updates cannot publish out of order.
        self.publish(writer.config.clone(), adopt_vault);
        Ok(())
    }

    fn publish(&self, mut next: Config, adopt_vault: bool) {
        let mut value = self.value.lock().unwrap_or_else(|e| e.into_inner());
        if !adopt_vault {
            next.notes_directory = value.notes_directory.take();
        }
        *value = next;
    }
}
