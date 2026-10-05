use std::path::PathBuf;

use tauri::{AppHandle, Manager};

use thoughttree_core::config::{self as shared, Config};
use thoughttree_core::types::{AgentProvider, EffortPreferences, ModelPreferences, ProviderPaths};

// Both desktop frontends share this file and its lock. Without the override
// for isolated development and parity fixtures, Tauri keeps its existing
// application data location.
fn config_directory(app: &AppHandle) -> Result<PathBuf, String> {
    match std::env::var_os("THOUGHTTREE_CONFIG_DIR") {
        Some(directory) => Ok(PathBuf::from(directory)),
        None => app
            .path()
            .app_data_dir()
            .map_err(|e| format!("Failed to locate config directory: {e}")),
    }
}

// Read fresh on every call: the native frontend may have committed since.
fn read(app: &AppHandle) -> Result<Config, String> {
    shared::read(&config_directory(app)?)
}

// Read-modify-write under the shared lock, so neither frontend can erase the
// other's settings.
pub(crate) fn update(app: &AppHandle, change: impl FnOnce(&mut Config)) -> Result<(), String> {
    shared::update(&config_directory(app)?, change)
        .map(drop)
        .map_err(|e| format!("Failed to save config: {e}"))
}

pub(crate) fn get_notes_directory_optional(app: &AppHandle) -> Result<Option<String>, String> {
    Ok(read(app)?
        .notes_directory
        .map(|path| path.to_string_lossy().into_owned()))
}

pub(crate) fn get_notes_directory_required(app: &AppHandle) -> Result<PathBuf, String> {
    read(app)?
        .notes_directory
        .ok_or_else(|| "Notes directory not configured. Please set it in settings.".to_string())
}

pub(crate) fn get_default_provider(app: &AppHandle) -> Result<AgentProvider, String> {
    Ok(read(app)?.default_provider)
}

pub(crate) fn get_model_preferences(app: &AppHandle) -> Result<ModelPreferences, String> {
    Ok(read(app)?.model_preferences)
}

pub(crate) fn get_effort_preferences(app: &AppHandle) -> Result<EffortPreferences, String> {
    Ok(read(app)?.effort_preferences)
}

pub(crate) fn get_provider_paths(app: &AppHandle) -> Result<ProviderPaths, String> {
    Ok(read(app)?.provider_paths)
}

pub(crate) fn get_recent_projects(app: &AppHandle) -> Result<Vec<String>, String> {
    Ok(read(app)?.recent_projects)
}
