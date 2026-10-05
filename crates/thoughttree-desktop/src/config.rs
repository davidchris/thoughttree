use std::{path::PathBuf, sync::Mutex};

pub use thoughttree_core::config::Config;
use thoughttree_core::config::{read, ConfigWriter};

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
        let mut writer = ConfigWriter::lock(&self.directory)?;
        if !change(&mut writer.config) {
            return Ok(());
        }
        writer.save()?;
        // Rendering reads the last committed value without waiting for a
        // different writer, disk access, or fsync. Publish before releasing
        // the file lock so concurrent updates cannot publish out of order.
        *self.value.lock().unwrap_or_else(|e| e.into_inner()) = writer.config.clone();
        Ok(())
    }
}
