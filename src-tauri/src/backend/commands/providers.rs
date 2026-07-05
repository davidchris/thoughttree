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
    let descriptor = provider.descriptor();

    // Some providers additionally ship a bundled ACP sidecar (see ADR-0001);
    // without it the CLI alone can't serve sessions
    if descriptor.requires_sidecar && find_sidecar_path().is_none() {
        return ProviderStatus {
            provider: provider.clone(),
            available: false,
            error_message: Some(
                "claude-code-acp sidecar not found (dev: run bun run build:sidecar)".to_string(),
            ),
        };
    }

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

    interpret_version_probe(&stdout, &stderr, provider)
}

/// Judge a `--version` probe's output. Adapters without a `--version` flag
/// (codex-acp) still validate: their usage error names the binary, so the
/// pattern matches and the `Usage:` line stands in for the version string.
fn interpret_version_probe(
    stdout: &str,
    stderr: &str,
    provider: &AgentProvider,
) -> Result<String, String> {
    let combined = format!("{stdout}{stderr}");
    let expected_pattern = provider.descriptor().version_pattern;

    if !combined.to_lowercase().contains(expected_pattern) {
        return Err(format!(
            "Not a valid {} executable (output: {})",
            provider.display_name(),
            combined.chars().take(100).collect::<String>()
        ));
    }

    let version_line = stdout
        .lines()
        .next()
        .or_else(|| stderr.lines().next())
        .unwrap_or("Unknown version")
        .trim();

    if version_line.to_lowercase().starts_with("error") {
        let usage_line = combined
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with("Usage:"))
            .unwrap_or("Recognized executable");
        return Ok(usage_line.to_string());
    }

    Ok(version_line.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_version_probe_accepts_adapter_without_version_flag() {
        // Real codex-acp output: `--version` is unrecognized, usage goes to
        // stderr — validation must still pass because usage names the binary
        let stderr = "error: unexpected argument '--version' found\n\n\
                      Usage: codex-acp [OPTIONS]\n\n\
                      For more information, try '--help'.\n";

        let result = interpret_version_probe("", stderr, &AgentProvider::Codex);

        let displayed = result.expect("codex-acp usage output should validate");
        assert!(
            !displayed.to_lowercase().starts_with("error"),
            "settings should not display an error line as the version: {displayed}"
        );
        assert!(displayed.contains("codex-acp"));
    }

    #[test]
    fn test_version_probe_accepts_normal_version_output() {
        let result =
            interpret_version_probe("1.0.35 (Claude Code)\n", "", &AgentProvider::ClaudeCode);
        assert_eq!(result.unwrap(), "1.0.35 (Claude Code)");
    }

    #[test]
    fn test_version_probe_rejects_wrong_executable() {
        let result = interpret_version_probe("git version 2.44.0\n", "", &AgentProvider::Codex);
        assert!(result.is_err());
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
