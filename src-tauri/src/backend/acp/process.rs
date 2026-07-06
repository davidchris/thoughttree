use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;
use tracing::{info, warn};

use crate::backend::types::{AgentProvider, ProviderDescriptor, ProviderPaths, ReasoningEffort};

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

    let found = candidate_paths(
        descriptor,
        custom_path,
        env_override_value.as_deref(),
        home.as_deref(),
        &[],
    )
    .into_iter()
    .find(|path| path.exists())
    .or_else(|| {
        // nvm-managed npm globals: iterate known Node versions (no globbing).
        // Listing the versions dir costs a read_dir, so only scan it after
        // every other candidate missed.
        std::fs::read_dir(home?.join(".nvm/versions/node"))
            .ok()?
            .flatten()
            .map(|entry| entry.path().join("bin").join(descriptor.executable_name))
            .find(|path| path.exists())
    });
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
    effort: Option<ReasoningEffort>,
) -> anyhow::Result<tokio::process::Child> {
    let sidecar_path = find_sidecar_path().ok_or_else(|| {
        anyhow::anyhow!(
            "claude-code-acp sidecar not found.\n\
             For development: run 'bun run build:sidecar' first.\n\
             For users: the app bundle may be corrupted."
        )
    })?;

    // Find Claude Code CLI for the sidecar to use
    let claude_cli_path = resolve_adapter_path(&AgentProvider::ClaudeCode, custom_path)?;

    info!(
        "Spawning claude-code-acp sidecar: {:?} in {:?}",
        sidecar_path, notes_directory
    );
    info!("Using Claude Code CLI at: {:?}", claude_cli_path);

    let mut command = Command::new(&sidecar_path);
    command
        .current_dir(notes_directory)
        .env("CLAUDE_CODE_EXECUTABLE", &claude_cli_path);
    if let Some(effort) = effort {
        command.env("CLAUDE_CODE_EFFORT_LEVEL", effort.as_str());
    }

    let child = command
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn sidecar: {e}"))?;

    Ok(child)
}

/// Resolve a provider's adapter executable, or fail with its install hint
fn resolve_adapter_path(
    provider: &AgentProvider,
    custom_path: Option<&str>,
) -> anyhow::Result<PathBuf> {
    let descriptor = provider.descriptor();
    find_provider_executable(provider, custom_path).ok_or_else(|| {
        anyhow::anyhow!(
            "{} not found.\n{}",
            descriptor.display_name,
            descriptor.install_hint
        )
    })
}

/// Args putting Gemini CLI in ACP mode. The model must be picked at spawn
/// time; absent a preference, default to the descriptor's first fallback
/// model — the same entry the selector offers first.
fn gemini_cli_args(model_id: Option<&str>) -> Vec<String> {
    let default_model = AgentProvider::GeminiCli
        .descriptor()
        .fallback_models
        .first()
        .map(|(id, _)| *id)
        .unwrap_or_default();

    vec![
        "--experimental-acp".to_string(),
        "--model".to_string(),
        model_id.unwrap_or(default_model).to_string(),
    ]
}

/// Args selecting Codex config via the adapter's standard config-override
/// flag (`codex-acp -c key=value`). No preference → no flag, so the user's
/// own codex config default applies.
fn codex_config_args(model_id: Option<&str>, effort: Option<ReasoningEffort>) -> Vec<String> {
    let mut args = Vec::new();

    if let Some(id) = model_id {
        args.extend(["-c".to_string(), format!("model={id}")]);
    }

    if let Some(effort) = effort {
        args.extend([
            "-c".to_string(),
            format!("model_reasoning_effort={}", effort.as_str()),
        ]);
    }

    args
}

/// Spawn a provider's ACP adapter over stdio — no sidecar. Extra args carry
/// per-spawn config such as the model override.
pub(crate) async fn spawn_plain_adapter(
    provider: &AgentProvider,
    notes_directory: &Path,
    custom_path: Option<&str>,
    args: &[String],
) -> anyhow::Result<tokio::process::Child> {
    let descriptor = provider.descriptor();
    let adapter_path = resolve_adapter_path(provider, custom_path)?;

    info!(
        "Spawning {} adapter: {:?} in {:?} with args {:?}",
        descriptor.display_name, adapter_path, notes_directory, args
    );

    let child = Command::new(&adapter_path)
        .args(args)
        .current_dir(notes_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!("Failed to spawn {}: {e}", descriptor.display_name))?;

    Ok(child)
}

/// Spawn an ACP-compatible agent subprocess based on provider
pub(crate) async fn spawn_agent_subprocess(
    provider: &AgentProvider,
    notes_directory: &Path,
    paths: &ProviderPaths,
    model_id: Option<&str>,
    effort: Option<ReasoningEffort>,
) -> anyhow::Result<tokio::process::Child> {
    let custom_path = paths.get(provider).map(String::as_str);
    match provider {
        AgentProvider::ClaudeCode => {
            spawn_claude_code_acp(notes_directory, custom_path, effort).await
        }
        AgentProvider::GeminiCli => {
            // Gemini CLI requires model to be specified at spawn time via --model flag
            spawn_plain_adapter(
                provider,
                notes_directory,
                custom_path,
                &gemini_cli_args(model_id),
            )
            .await
        }
        AgentProvider::Codex => {
            // Codex model is applied at spawn via `-c model=<id>`
            spawn_plain_adapter(
                provider,
                notes_directory,
                custom_path,
                &codex_config_args(model_id, effort),
            )
            .await
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
    fn test_candidate_paths_without_custom_or_home_uses_known_paths_only() {
        let paths = candidate_paths(claude_descriptor(), None, None, None, &[]);
        assert_eq!(paths[0], PathBuf::from("/opt/homebrew/bin/claude"));
        assert_eq!(paths.len(), claude_descriptor().known_paths.len());
    }

    #[test]
    fn test_gemini_args_default_model_comes_from_descriptor() {
        let (default_id, _) = AgentProvider::GeminiCli.descriptor().fallback_models[0];
        assert_eq!(
            gemini_cli_args(None),
            vec![
                "--experimental-acp".to_string(),
                "--model".to_string(),
                default_id.to_string()
            ]
        );
    }

    #[test]
    fn test_gemini_args_pass_model_preference() {
        assert_eq!(
            gemini_cli_args(Some("gemini-2.5")),
            vec![
                "--experimental-acp".to_string(),
                "--model".to_string(),
                "gemini-2.5".to_string()
            ]
        );
    }

    #[test]
    fn test_codex_args_pass_model_as_config_override() {
        let args = codex_config_args(Some("gpt-5.5"), None);
        assert_eq!(args, vec!["-c".to_string(), "model=gpt-5.5".to_string()]);
    }

    #[test]
    fn test_codex_args_pass_model_and_effort_as_config_overrides() {
        let args = codex_config_args(Some("gpt-5.5"), Some(ReasoningEffort::XHigh));
        assert_eq!(
            args,
            vec![
                "-c".to_string(),
                "model=gpt-5.5".to_string(),
                "-c".to_string(),
                "model_reasoning_effort=xhigh".to_string(),
            ]
        );
    }

    #[test]
    fn test_codex_args_pass_effort_without_model_as_config_override() {
        let args = codex_config_args(None, Some(ReasoningEffort::Medium));
        assert_eq!(
            args,
            vec![
                "-c".to_string(),
                "model_reasoning_effort=medium".to_string(),
            ]
        );
    }

    #[test]
    fn test_codex_args_empty_without_model_so_adapter_default_applies() {
        assert!(codex_config_args(None, None).is_empty());
    }

    #[test]
    fn test_codex_candidate_paths_target_adapter_binary() {
        let home = PathBuf::from("/home/tester");
        let nvm_dirs = vec![PathBuf::from("/home/tester/.nvm/versions/node/v20.0.0")];

        let paths = candidate_paths(
            AgentProvider::Codex.descriptor(),
            None,
            None,
            Some(&home),
            &nvm_dirs,
        );

        let expected: Vec<PathBuf> = [
            "/opt/homebrew/bin/codex-acp",
            "/usr/local/bin/codex-acp",
            "/home/tester/.bun/bin/codex-acp",
            "/home/tester/.npm-global/bin/codex-acp",
            "/home/tester/.nvm/versions/node/v20.0.0/bin/codex-acp",
        ]
        .iter()
        .map(PathBuf::from)
        .collect();

        assert_eq!(paths, expected);
    }
}
