use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use tokio::process::Command;

use crate::backend::acp::process::{find_provider_executable, find_sidecar_path};
use crate::backend::acp::sessions::run_model_discovery_session;
use crate::backend::config;
use crate::backend::runtime::run_localset_blocking;
use crate::backend::types::{
    AgentProvider, ModelInfo, ModelPreferences, ProviderPaths, ProviderStatus,
};

fn check_provider_availability(provider: &AgentProvider, paths: &ProviderPaths) -> ProviderStatus {
    // Claude Code additionally ships a bundled ACP sidecar (see ADR-0001);
    // without it the CLI alone can't serve sessions
    if matches!(provider, AgentProvider::ClaudeCode) && find_sidecar_path().is_none() {
        return ProviderStatus {
            provider: provider.clone(),
            available: false,
            error_message: Some(
                "claude-code-acp sidecar not found (dev: run bun run build:sidecar)".to_string(),
            ),
        };
    }

    let descriptor = provider.descriptor();
    let custom_path = paths.get(provider).map(String::as_str);
    let cli_available = find_provider_executable(provider, custom_path).is_some();

    ProviderStatus {
        provider: provider.clone(),
        available: cli_available,
        error_message: (!cli_available).then(|| {
            format!(
                "{} not found. {}",
                descriptor.display_name,
                descriptor.install_hint.lines().next().unwrap_or_default()
            )
        }),
    }
}

async fn validate_executable(path: &Path, provider: &AgentProvider) -> Result<String, String> {
    if !path.exists() {
        return Err("File does not exist".to_string());
    }

    if !path.is_file() {
        return Err("Path is not a file".to_string());
    }

    let output = Command::new(path)
        .arg("--version")
        .output()
        .await
        .map_err(|e| format!("Failed to execute: {e}"))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{stdout}{stderr}");

    let expected_pattern = provider.descriptor().version_pattern;

    if combined.to_lowercase().contains(expected_pattern) {
        let version_line = stdout
            .lines()
            .next()
            .or_else(|| stderr.lines().next())
            .unwrap_or("Unknown version")
            .trim();
        Ok(version_line.to_string())
    } else {
        Err(format!(
            "Not a valid {} executable (output: {})",
            provider.display_name(),
            combined.chars().take(100).collect::<String>()
        ))
    }
}

#[tauri::command]
pub(crate) async fn get_available_providers(app: AppHandle) -> Result<Vec<ProviderStatus>, String> {
    let paths = config::get_provider_paths(&app)?;

    Ok(AgentProvider::ALL
        .iter()
        .map(|provider| check_provider_availability(provider, &paths))
        .collect())
}

#[tauri::command]
pub(crate) async fn get_default_provider(app: AppHandle) -> Result<AgentProvider, String> {
    config::get_default_provider(&app)
}

#[tauri::command]
pub(crate) async fn set_default_provider(
    app: AppHandle,
    provider: AgentProvider,
) -> Result<(), String> {
    config::set_default_provider(&app, &provider)?;
    tracing::info!("Default provider set to: {:?}", provider);
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_model_preferences(app: AppHandle) -> Result<ModelPreferences, String> {
    config::get_model_preferences(&app)
}

#[tauri::command]
pub(crate) async fn set_model_preference(
    app: AppHandle,
    provider: AgentProvider,
    model_id: Option<String>,
) -> Result<(), String> {
    let mut preferences = config::get_model_preferences(&app)?;
    preferences.set(&provider, model_id.clone());
    config::set_model_preferences(&app, &preferences)?;

    tracing::info!("Model preference for {:?} set to: {:?}", provider, model_id);
    Ok(())
}

#[tauri::command]
pub(crate) async fn get_provider_paths(app: AppHandle) -> Result<ProviderPaths, String> {
    config::get_provider_paths(&app)
}

#[tauri::command]
pub(crate) async fn set_provider_path(
    app: AppHandle,
    provider: AgentProvider,
    path: Option<String>,
) -> Result<(), String> {
    if let Some(ref candidate_path) = path {
        validate_executable(&PathBuf::from(candidate_path), &provider).await?;
    }

    let mut paths = config::get_provider_paths(&app)?;
    paths.set(&provider, path.clone());
    config::set_provider_paths(&app, &paths)?;

    tracing::info!("Provider path for {:?} set to: {:?}", provider, path);
    Ok(())
}

#[tauri::command]
pub(crate) async fn validate_provider_path(
    provider: AgentProvider,
    path: String,
) -> Result<String, String> {
    validate_executable(&PathBuf::from(path), &provider).await
}

#[tauri::command]
pub(crate) async fn pick_provider_executable(
    app: AppHandle,
    provider: AgentProvider,
) -> Result<Option<String>, String> {
    let title = format!("Select {} Executable", provider.display_name());

    let path = app.dialog().file().set_title(&title).blocking_pick_file();

    Ok(path.map(|p| p.to_string()))
}

#[tauri::command]
pub(crate) async fn get_available_models(
    app: AppHandle,
    provider: AgentProvider,
) -> Result<Vec<ModelInfo>, String> {
    let notes_directory = config::get_notes_directory_required(&app)?;
    let provider_paths = config::get_provider_paths(&app)?;

    run_localset_blocking(move || async move {
        run_model_discovery_session(notes_directory, provider, provider_paths).await
    })
    .await
}
