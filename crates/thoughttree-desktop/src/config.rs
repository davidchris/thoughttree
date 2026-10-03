use std::{collections::BTreeMap, fs, io::Write, path::PathBuf, sync::Mutex};

use serde::{Deserialize, Serialize};
use thoughttree_core::types::{AgentProvider, EffortPreferences, ModelPreferences, ProviderPaths};

/// The existing desktop config schema. Unknown keys survive native edits.
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

// Tauri reads each settings key independently and defaults a malformed value.
// A retired provider or malformed unrelated preference must not prevent native
// startup or hide otherwise valid Vault and recovery settings.
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

pub(crate) struct ConfigStore {
    directory: PathBuf,
    value: Mutex<Config>,
}

impl ConfigStore {
    pub(crate) fn open(directory: PathBuf) -> Result<Self, String> {
        let value = read_config(&directory)?;
        Ok(Self {
            directory,
            value: Mutex::new(value),
        })
    }

    pub(crate) fn get(&self) -> Config {
        self.value.lock().unwrap_or_else(|e| e.into_inner()).clone()
    }

    pub(crate) fn update(&self, change: impl FnOnce(&mut Config)) -> Result<(), String> {
        fs::create_dir_all(&self.directory).map_err(|e| e.to_string())?;
        // A stable lock serializes native windows/processes. Rereading inside
        // it also preserves unrelated config edits made by the Tauri app.
        let lock = fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(self.directory.join("config.lock"))
            .map_err(|e| e.to_string())?;
        lock.lock().map_err(|e| e.to_string())?;
        let mut next = read_config(&self.directory)?;
        change(&mut next);
        let bytes = serde_json::to_vec_pretty(&next).map_err(|e| e.to_string())?;
        let mut temp = tempfile::NamedTempFile::new_in(&self.directory)
            .map_err(|e| format!("Cannot prepare config: {e}"))?;
        temp.write_all(&bytes).map_err(|e| e.to_string())?;
        temp.as_file().sync_all().map_err(|e| e.to_string())?;
        temp.persist(self.directory.join("config.json"))
            .map_err(|e| format!("Cannot save config: {e}"))?;
        // Rendering reads the last committed value without waiting for a
        // different writer, disk access, or fsync. Publish before releasing
        // the file lock so concurrent updates cannot publish out of order.
        *self.value.lock().unwrap_or_else(|e| e.into_inner()) = next;
        Ok(())
    }
}

fn read_config(directory: &std::path::Path) -> Result<Config, String> {
    match fs::read(directory.join("config.json")) {
        Ok(bytes) => {
            serde_json::from_slice(&bytes).map_err(|e| format!("Cannot read config.json: {e}"))
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(format!("Cannot open config.json: {e}")),
    }
}
