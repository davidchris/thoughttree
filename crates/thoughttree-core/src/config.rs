//! The desktop config file shared by the Tauri and native frontends.
//!
//! Both frontends may run at once against the same `config.json`. Every write
//! takes `config.lock`, rereads the file, applies one change and replaces the
//! file atomically, so neither writer can erase the other's settings.

use std::{
    collections::BTreeMap,
    fs::{self, File},
    io::Write,
    path::{Path, PathBuf},
};

use serde::{Deserialize, Serialize};

use crate::types::{AgentProvider, EffortPreferences, ModelPreferences, ProviderPaths};

/// The existing desktop config schema. Unknown keys survive edits.
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
#[serde(default)]
pub struct Config {
    #[serde(deserialize_with = "default_on_invalid")]
    pub notes_directory: Option<PathBuf>,
    #[serde(deserialize_with = "default_on_invalid")]
    pub default_provider: AgentProvider,
    #[serde(deserialize_with = "default_on_invalid")]
    pub model_preferences: ModelPreferences,
    #[serde(deserialize_with = "default_on_invalid")]
    pub effort_preferences: EffortPreferences,
    #[serde(deserialize_with = "default_on_invalid")]
    pub provider_paths: ProviderPaths,
    #[serde(deserialize_with = "recent_paths")]
    pub recent_projects: Vec<String>,
    #[serde(flatten)]
    pub extra: BTreeMap<String, serde_json::Value>,
}

// Each settings key defaults independently when malformed. A retired provider
// or malformed unrelated preference must not prevent startup or hide otherwise
// valid Vault and recovery settings.
fn default_on_invalid<'de, D, T>(deserializer: D) -> Result<T, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned + Default,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).unwrap_or_default())
}

fn recent_paths<'de, D>(deserializer: D) -> Result<Vec<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(value
        .as_array()
        .into_iter()
        .flatten()
        .filter_map(|value| value.as_str().map(str::to_owned))
        .collect())
}

/// Reads the last committed config. A missing file is the default config.
pub fn read(directory: &Path) -> Result<Config, String> {
    match fs::read(directory.join("config.json")) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read config.json: {e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("Cannot open config.json: {e}")),
    }
}

/// Applies one change to the current file contents and commits it.
pub fn update(directory: &Path, change: impl FnOnce(&mut Config)) -> Result<Config, String> {
    let mut writer = ConfigWriter::lock(directory)?;
    change(&mut writer.config);
    writer.save()?;
    Ok(writer.config)
}

/// Exclusive access to the config file until dropped. `config` holds the file
/// contents read after the lock was acquired.
pub struct ConfigWriter {
    directory: PathBuf,
    _lock: File,
    pub config: Config,
}

impl ConfigWriter {
    pub fn lock(directory: &Path) -> Result<Self, String> {
        fs::create_dir_all(directory).map_err(|e| e.to_string())?;
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("config.lock"))
            .map_err(|e| e.to_string())?;
        lock.lock().map_err(|e| e.to_string())?;
        Ok(Self {
            directory: directory.to_owned(),
            config: read(directory)?,
            _lock: lock,
        })
    }

    pub fn save(&self) -> Result<(), String> {
        let bytes = serde_json::to_vec_pretty(&self.config).map_err(|e| e.to_string())?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.directory)
            .map_err(|e| format!("Cannot prepare config: {e}"))?;
        temp.write_all(&bytes).map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist(self.directory.join("config.json"))
            .map_err(|e| format!("Cannot save config: {e}"))?;
        Ok(())
    }
}
