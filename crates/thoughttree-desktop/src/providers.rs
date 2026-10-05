use std::{collections::HashMap, path::Path, sync::Mutex, time::Duration};

use thoughttree_core::{
    acp::{
        process::{adapter_command, find_provider_executable, find_sidecar_path},
        sessions::run_model_discovery_session,
    },
    runtime::run_localset_blocking,
    types::{AgentProvider, ProviderPaths, ProviderStatus},
};

use crate::{Desktop, DesktopEvent};

impl Desktop {
    pub fn available_providers(&self) -> Vec<ProviderStatus> {
        let paths = self.config().provider_paths;
        AgentProvider::ALL
            .iter()
            .map(|provider| availability(provider, &paths))
            .collect()
    }

    pub fn check_acp_available(&self) -> bool {
        find_sidecar_path().is_some()
    }

    pub fn discover_models(&self, provider: AgentProvider) -> Result<(), String> {
        let root = self.notes_directory()?;
        let paths = self.config().provider_paths;
        let provider_path = paths.get(&provider).cloned();
        let sink = self.0.sink.clone();
        self.runtime().spawn(async move {
            let active_provider = provider.clone();
            let result = run_localset_blocking(move || async move {
                run_model_discovery_session(root, active_provider, paths).await
            })
            .await;
            sink.emit(DesktopEvent::ModelsDiscovered {
                provider,
                provider_path,
                result,
            });
        });
        Ok(())
    }

    /// Validate the user's selected executable off the UI thread, then persist
    /// it only on success. Completion (including reset) arrives as an event.
    /// A newer choice or Reset for the same provider supersedes this request:
    /// once superseded, it no longer persists and reports as not `current`.
    pub fn set_provider_path(&self, provider: AgentProvider, path: Option<String>) {
        let generation = self.0.provider_path_requests.begin(&provider);
        let desktop = self.clone();
        self.runtime().spawn(async move {
            let result = desktop
                .apply_provider_path(provider.clone(), path, generation)
                .await;
            let current = desktop
                .0
                .provider_path_requests
                .is_current(&provider, generation);
            desktop.0.sink.emit(DesktopEvent::ProviderPathValidated {
                provider,
                result,
                current,
            });
        });
    }

    async fn apply_provider_path(
        &self,
        provider: AgentProvider,
        path: Option<String>,
        generation: u64,
    ) -> Result<String, String> {
        let version = match &path {
            Some(path) => validate_executable(Path::new(path), &provider).await?,
            None => "Using automatic discovery".to_string(),
        };
        let desktop = self.clone();
        self.runtime()
            .spawn_blocking(move || {
                // Decided once, under the config lock: a request that begins
                // after this check commits after it, and file and cache agree.
                desktop.0.config.update_if(
                    |_| {
                        desktop
                            .0
                            .provider_path_requests
                            .is_current(&provider, generation)
                    },
                    |config| config.provider_paths.set(&provider, path.clone()),
                )
            })
            .await
            .map_err(|error| format!("Cannot save provider path: {error}"))??;
        Ok(version)
    }
}

/// The latest provider-path request per provider. Validation may take up to
/// 15 seconds while Reset skips it, so requests can finish out of order.
#[derive(Default)]
pub(crate) struct ProviderPathRequests(Mutex<HashMap<&'static str, u64>>);

impl ProviderPathRequests {
    fn begin(&self, provider: &AgentProvider) -> u64 {
        let mut latest = self.0.lock().unwrap_or_else(|e| e.into_inner());
        let generation = latest.entry(provider.descriptor().id).or_default();
        *generation += 1;
        *generation
    }

    fn is_current(&self, provider: &AgentProvider, generation: u64) -> bool {
        let latest = self.0.lock().unwrap_or_else(|e| e.into_inner());
        latest.get(provider.descriptor().id) == Some(&generation)
    }
}

fn availability(provider: &AgentProvider, paths: &ProviderPaths) -> ProviderStatus {
    let descriptor = provider.descriptor();
    if descriptor.requires_sidecar && find_sidecar_path().is_none() {
        return ProviderStatus {
            provider: provider.clone(),
            available: false,
            error_message: Some(
                "claude-code-acp sidecar not found (dev: run bun run build:sidecar)".into(),
            ),
        };
    }
    let available =
        find_provider_executable(provider, paths.get(provider).map(String::as_str)).is_some();
    ProviderStatus {
        provider: provider.clone(),
        available,
        error_message: (!available).then(|| {
            format!(
                "{} not found. {}",
                descriptor.display_name,
                descriptor.install_hint.lines().next().unwrap_or_default()
            )
        }),
    }
}

async fn validate_executable(path: &Path, provider: &AgentProvider) -> Result<String, String> {
    if !path.is_absolute() || !path.is_file() {
        return Err("Select an executable file with an absolute path".into());
    }
    let mut command = adapter_command(path);
    command.arg("--version").kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(15), command.output())
        .await
        .map_err(|_| "Executable version check timed out".to_string())?
        .map_err(|e| format!("Failed to execute: {e}"))?;
    interpret_version_probe(
        &String::from_utf8_lossy(&output.stdout),
        &String::from_utf8_lossy(&output.stderr),
        provider,
    )
}

fn interpret_version_probe(
    stdout: &str,
    stderr: &str,
    provider: &AgentProvider,
) -> Result<String, String> {
    let combined = format!("{stdout}{stderr}");
    if !combined
        .to_lowercase()
        .contains(provider.descriptor().version_pattern)
    {
        return Err(format!(
            "Not a valid {} executable (output: {})",
            provider.display_name(),
            combined.chars().take(100).collect::<String>()
        ));
    }
    let version = stdout
        .lines()
        .next()
        .or_else(|| stderr.lines().next())
        .unwrap_or("Unknown version")
        .trim();
    if version.to_lowercase().starts_with("error") {
        return Ok(combined
            .lines()
            .map(str::trim)
            .find(|line| line.starts_with("Usage:"))
            .unwrap_or("Recognized executable")
            .to_owned());
    }
    Ok(version.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn version_probe_accepts_codex_usage_and_claude_version_but_not_other_programs() {
        let codex = interpret_version_probe(
            "",
            "error: unexpected --version\nUsage: codex-acp [OPTIONS]\n",
            &AgentProvider::Codex,
        )
        .unwrap();
        assert_eq!(codex, "Usage: codex-acp [OPTIONS]");
        assert_eq!(
            interpret_version_probe("1.0.35 (Claude Code)\n", "", &AgentProvider::ClaudeCode)
                .unwrap(),
            "1.0.35 (Claude Code)"
        );
        assert!(
            interpret_version_probe("git version 2.44.0\n", "", &AgentProvider::Codex).is_err()
        );
    }
}
