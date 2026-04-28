use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use tokio::process::Command;

use crate::backend::acp::process::{
    find_claude_code_executable, find_gemini_cli_executable, find_sidecar_path,
};
use crate::backend::acp::sessions::run_model_discovery_session;
use crate::backend::config;
use crate::backend::runtime::run_localset_blocking;
use crate::backend::types::{
    AgentProvider, ModelInfo, ModelPreferences, ProviderPaths, ProviderStatus,
};

fn check_provider_availability(provider: &AgentProvider, paths: &ProviderPaths) -> ProviderStatus {
    match provider {
        AgentProvider::ClaudeCode => {
            let sidecar_available = find_sidecar_path().is_some();
            let custom_path = paths.claude_code.as_deref();
            let cli_available = find_claude_code_executable(custom_path).is_some();

            ProviderStatus {
                provider: provider.clone(),
                available: sidecar_available && cli_available,
                error_message: if !sidecar_available {
                    Some(
                        "claude-code-acp sidecar not found (dev: run bun run build:sidecar)"
                            .to_string(),
                    )
                } else if !cli_available {
                    Some(
                        "Claude Code CLI not found. Install via: brew install --cask claude-code"
                            .to_string(),
                    )
                } else {
                    None
                },
            }
        }
        AgentProvider::GeminiCli => {
            let custom_path = paths.gemini_cli.as_deref();
            let cli_available = find_gemini_cli_executable(custom_path).is_some();

            ProviderStatus {
                provider: provider.clone(),
                available: cli_available,
                error_message: if !cli_available {
                    Some("Gemini CLI not found. Install via: brew install gemini-cli".to_string())
                } else {
                    None
                },
            }
        }
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
        .map_err(|e| format!("Failed to execute: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    let expected_pattern = match provider {
        AgentProvider::ClaudeCode => "claude",
        AgentProvider::GeminiCli => "gemini",
    };

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
pub async fn get_available_providers(app: AppHandle) -> Result<Vec<ProviderStatus>, String> {
    let paths = config::get_provider_paths(&app)?;

    Ok(vec![
        check_provider_availability(&AgentProvider::ClaudeCode, &paths),
        check_provider_availability(&AgentProvider::GeminiCli, &paths),
    ])
}

#[tauri::command]
pub async fn get_default_provider(app: AppHandle) -> Result<AgentProvider, String> {
    config::get_default_provider(&app)
}

#[tauri::command]
pub async fn set_default_provider(app: AppHandle, provider: AgentProvider) -> Result<(), String> {
    config::set_default_provider(&app, &provider)?;
    tracing::info!("Default provider set to: {:?}", provider);
    Ok(())
}

#[tauri::command]
pub async fn get_model_preferences(app: AppHandle) -> Result<ModelPreferences, String> {
    config::get_model_preferences(&app)
}

#[tauri::command]
pub async fn set_model_preference(
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
pub async fn get_provider_paths(app: AppHandle) -> Result<ProviderPaths, String> {
    config::get_provider_paths(&app)
}

#[tauri::command]
pub async fn set_provider_path(
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
pub async fn validate_provider_path(
    provider: AgentProvider,
    path: String,
) -> Result<String, String> {
    validate_executable(&PathBuf::from(path), &provider).await
}

#[tauri::command]
pub async fn pick_provider_executable(
    app: AppHandle,
    provider: AgentProvider,
) -> Result<Option<String>, String> {
    let title = format!("Select {} Executable", provider.display_name());

    let path = app.dialog().file().set_title(&title).blocking_pick_file();

    Ok(path.map(|p| p.to_string()))
}

#[tauri::command]
pub async fn get_available_models(
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
