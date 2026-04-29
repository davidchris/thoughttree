use std::path::PathBuf;

use serde::Serialize;
use tauri::AppHandle;
use tauri_plugin_store::StoreExt;

use crate::backend::types::{AgentProvider, ModelPreferences, ProviderPaths};

const CONFIG_STORE: &str = "config.json";

fn save_serialized_value<T: Serialize + ?Sized>(
    app: &AppHandle,
    key: &str,
    value: &T,
) -> Result<(), String> {
    let store = app
        .store(CONFIG_STORE)
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    let json_value =
        serde_json::to_value(value).map_err(|e| format!("Failed to serialize {}: {}", key, e))?;

    store.set(key, json_value);
    store
        .save()
        .map_err(|e| format!("Failed to save config: {}", e))
}

pub(crate) fn get_notes_directory_optional(app: &AppHandle) -> Result<Option<String>, String> {
    let store = app
        .store(CONFIG_STORE)
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    Ok(store
        .get("notes_directory")
        .and_then(|v| v.as_str().map(String::from)))
}

pub(crate) fn get_notes_directory_required(app: &AppHandle) -> Result<PathBuf, String> {
    get_notes_directory_optional(app)?
        .map(PathBuf::from)
        .ok_or_else(|| "Notes directory not configured. Please set it in settings.".to_string())
}

pub(crate) fn set_notes_directory(app: &AppHandle, path: &str) -> Result<(), String> {
    save_serialized_value(app, "notes_directory", &path)
}

pub(crate) fn get_default_provider(app: &AppHandle) -> Result<AgentProvider, String> {
    let store = app
        .store(CONFIG_STORE)
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    Ok(store
        .get("default_provider")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default())
}

pub(crate) fn set_default_provider(
    app: &AppHandle,
    provider: &AgentProvider,
) -> Result<(), String> {
    save_serialized_value(app, "default_provider", provider)
}

pub(crate) fn get_model_preferences(app: &AppHandle) -> Result<ModelPreferences, String> {
    let store = app
        .store(CONFIG_STORE)
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    Ok(store
        .get("model_preferences")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default())
}

pub(crate) fn set_model_preferences(
    app: &AppHandle,
    preferences: &ModelPreferences,
) -> Result<(), String> {
    save_serialized_value(app, "model_preferences", preferences)
}

pub(crate) fn get_provider_paths(app: &AppHandle) -> Result<ProviderPaths, String> {
    let store = app
        .store(CONFIG_STORE)
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    Ok(store
        .get("provider_paths")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default())
}

pub(crate) fn set_provider_paths(app: &AppHandle, paths: &ProviderPaths) -> Result<(), String> {
    save_serialized_value(app, "provider_paths", paths)
}

pub(crate) fn get_recent_projects(app: &AppHandle) -> Result<Vec<String>, String> {
    let store = app
        .store(CONFIG_STORE)
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    Ok(store
        .get("recent_projects")
        .and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|value| value.as_str().map(|s| s.to_string()))
                    .collect()
            })
        })
        .unwrap_or_default())
}

pub(crate) fn set_recent_projects(app: &AppHandle, projects: &[String]) -> Result<(), String> {
    save_serialized_value(app, "recent_projects", projects)
}
