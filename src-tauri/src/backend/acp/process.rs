use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;
use tracing::{info, warn};

use crate::backend::types::{AgentProvider, ProviderDescriptor, ProviderPaths};

/// Find the bundled claude-code-acp sidecar binary
pub(crate) fn find_sidecar_path() -> Option<PathBuf> {
    let exe = std::env::current_exe().ok()?;
    let exe_dir = exe.parent()?;

    // Standard location: next to the main executable
    let sidecar = exe_dir.join("claude-code-acp");
    if sidecar.exists() {
        return Some(sidecar);
    }

    // Development: check src-tauri/binaries with target triple
    // Get the target triple for the current platform
    #[cfg(all(target_os = "macos", target_arch = "aarch64"))]
    let target_triple = "aarch64-apple-darwin";
    #[cfg(all(target_os = "macos", target_arch = "x86_64"))]
    let target_triple = "x86_64-apple-darwin";
    #[cfg(not(target_os = "macos"))]
    let target_triple = "";

    if !target_triple.is_empty() {
        // Try to find in development location
        // Walk up from exe to find src-tauri/binaries
        let mut current = exe_dir.to_path_buf();
        for _ in 0..10 {
            let dev_sidecar = current
                .join("src-tauri/binaries")
                .join(format!("claude-code-acp-{target_triple}"));
            if dev_sidecar.exists() {
                return Some(dev_sidecar);
            }

            // Also check Cargo build outputs in dev workflows
            let dev_target = current.join("src-tauri/target");
            let dev_debug = dev_target.join("debug/claude-code-acp");
            if dev_debug.exists() {
                return Some(dev_debug);
            }
            let dev_release = dev_target.join("release/claude-code-acp");
            if dev_release.exists() {
                return Some(dev_release);
            }

            if !current.pop() {
                break;
            }
        }
    }

    None
}

/// Ordered executable candidates for a provider. Pure: environment inputs
/// (env override value, home dir, nvm node dirs) are passed in, existence is
/// checked by the caller.
///
/// Precedence: env override > custom path > known paths > home-relative
/// paths > nvm-managed node bins.
pub(crate) fn candidate_paths(
    descriptor: &ProviderDescriptor,
    custom_path: Option<&str>,
    env_override_value: Option<&str>,
    home: Option<&Path>,
    nvm_node_dirs: &[PathBuf],
) -> Vec<PathBuf> {
    let mut candidates = Vec::new();

    if let Some(env_path) = env_override_value {
        candidates.push(PathBuf::from(env_path));
    }

    if let Some(custom) = custom_path {
        candidates.push(PathBuf::from(custom));
    }

    candidates.extend(descriptor.known_paths.iter().map(PathBuf::from));

    if let Some(home) = home {
        candidates.extend(
            descriptor
                .home_relative_paths
                .iter()
                .map(|relative| home.join(relative)),
        );
        candidates.extend(
            nvm_node_dirs
                .iter()
                .map(|node_dir| node_dir.join("bin").join(descriptor.executable_name)),
        );
    }

    candidates
}

/// Find a provider's executable: first candidate path that exists.
/// Security: Only checks known installation paths — no PATH/`which` lookup,
/// which prevents PATH injection attacks.
pub(crate) fn find_provider_executable(
    provider: &AgentProvider,
    custom_path: Option<&str>,
) -> Option<PathBuf> {
    let descriptor = provider.descriptor();

    let env_override_value = descriptor
        .env_override
        .and_then(|var| std::env::var(var).ok());

    let home = dirs::home_dir();
    // nvm-managed npm globals: iterate known Node versions (no globbing)
    let nvm_node_dirs: Vec<PathBuf> = home
        .as_deref()
        .map(|home| home.join(".nvm/versions/node"))
        .and_then(|nvm_base| std::fs::read_dir(nvm_base).ok())
        .map(|entries| entries.flatten().map(|entry| entry.path()).collect())
        .unwrap_or_default();

    let candidates = candidate_paths(
        descriptor,
        custom_path,
        env_override_value.as_deref(),
        home.as_deref(),
        &nvm_node_dirs,
    );

    let found = candidates.into_iter().find(|path| path.exists());
    match &found {
        Some(path) => {
            // Log canonical path for debugging, but return original path for execution
            // (Homebrew symlinks point to wrapper scripts that must be executed directly)
            if let Ok(canonical) = std::fs::canonicalize(path) {
                info!(
                    "Found {} executable at {:?} (resolves to: {:?})",
                    descriptor.display_name, path, canonical
                );
            } else {
                info!("Found {} executable at {:?}", descriptor.display_name, path);
            }
        }
        None => warn!(
            "{} executable not found in any known location",
            descriptor.display_name
        ),
    }
    found
}

/// Spawn the claude-code-acp sidecar
pub(crate) async fn spawn_claude_code_acp(
    notes_directory: &Path,
    custom_path: Option<&str>,
) -> anyhow::Result<tokio::process::Child> {
    let sidecar_path = find_sidecar_path().ok_or_else(|| {
        anyhow::anyhow!(
            "claude-code-acp sidecar not found.\n\
             For development: run 'bun run build:sidecar' first.\n\
             For users: the app bundle may be corrupted."
        )
    })?;

    // Find Claude Code CLI for the sidecar to use
    let descriptor = AgentProvider::ClaudeCode.descriptor();
    let claude_cli_path = find_provider_executable(&AgentProvider::ClaudeCode, custom_path)
        .ok_or_else(|| {
            anyhow::anyhow!(
                "{} not found.\n{}",
                descriptor.display_name,
                descriptor.install_hint
            )
        })?;

    info!(
        "Spawning claude-code-acp sidecar: {:?} in {:?}",
        sidecar_path, notes_directory
    );
    info!("Using Claude Code CLI at: {:?}", claude_cli_path);

    let child = Command::new(&sidecar_path)
        .current_dir(notes_directory)
        .env("CLAUDE_CODE_EXECUTABLE", &claude_cli_path)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn sidecar: {e}"))?;

    Ok(child)
}

/// Spawn Gemini CLI in ACP mode
pub(crate) async fn spawn_gemini_cli_acp(
    notes_directory: &Path,
    custom_path: Option<&str>,
    model_id: Option<&str>,
) -> anyhow::Result<tokio::process::Child> {
    let descriptor = AgentProvider::GeminiCli.descriptor();
    let gemini_path =
        find_provider_executable(&AgentProvider::GeminiCli, custom_path).ok_or_else(|| {
            anyhow::anyhow!(
                "{} not found.\n{}",
                descriptor.display_name,
                descriptor.install_hint
            )
        })?;

    // Use provided model or default to gemini-3
    let model = model_id.unwrap_or("gemini-3");

    info!(
        "Spawning Gemini CLI ACP mode: {:?} in {:?} with model {:?}",
        gemini_path, notes_directory, model
    );

    let child = Command::new(&gemini_path)
        .args(["--experimental-acp", "--model", model])
        .current_dir(notes_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn Gemini CLI: {e}"))?;

    Ok(child)
}

/// Spawn an ACP-compatible agent subprocess based on provider
pub(crate) async fn spawn_agent_subprocess(
    provider: &AgentProvider,
    notes_directory: &Path,
    paths: &ProviderPaths,
    model_id: Option<&str>,
) -> anyhow::Result<tokio::process::Child> {
    let custom_path = paths.get(provider).map(String::as_str);
    match provider {
        AgentProvider::ClaudeCode => spawn_claude_code_acp(notes_directory, custom_path).await,
        AgentProvider::GeminiCli => {
            // Gemini CLI requires model to be specified at spawn time via --model flag
            spawn_gemini_cli_acp(notes_directory, custom_path, model_id).await
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn claude_descriptor() -> &'static crate::backend::types::ProviderDescriptor {
        AgentProvider::ClaudeCode.descriptor()
    }

    #[test]
    fn test_candidate_paths_full_precedence() {
        let home = PathBuf::from("/home/tester");
        let nvm_dirs = vec![PathBuf::from("/home/tester/.nvm/versions/node/v20.0.0")];

        let paths = candidate_paths(
            claude_descriptor(),
            Some("/custom/claude"),
            Some("/env/claude"),
            Some(&home),
            &nvm_dirs,
        );

        let expected: Vec<PathBuf> = [
            "/env/claude",
            "/custom/claude",
            "/opt/homebrew/bin/claude",
            "/usr/local/bin/claude",
            "/home/tester/.claude/local/claude",
            "/home/tester/.local/bin/claude",
            "/home/tester/.bun/bin/claude",
            "/home/tester/.npm-global/bin/claude",
            "/home/tester/.nvm/versions/node/v20.0.0/bin/claude",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();

        assert_eq!(paths, expected);
    }

    #[test]
    fn test_candidate_paths_without_env_override_starts_with_custom() {
        let paths = candidate_paths(claude_descriptor(), Some("/custom/claude"), None, None, &[]);
        assert_eq!(paths[0], PathBuf::from("/custom/claude"));
        assert_eq!(paths[1], PathBuf::from("/opt/homebrew/bin/claude"));
    }

    #[test]
    fn test_candidate_paths_without_custom_starts_with_known() {
        let paths = candidate_paths(claude_descriptor(), None, None, None, &[]);
        assert_eq!(paths[0], PathBuf::from("/opt/homebrew/bin/claude"));
    }

    #[test]
    fn test_candidate_paths_without_home_skips_home_relative() {
        let paths = candidate_paths(claude_descriptor(), None, None, None, &[]);
        let known_count = claude_descriptor().known_paths.len();
        assert_eq!(paths.len(), known_count);
    }
}
