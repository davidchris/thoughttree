use std::path::{Path, PathBuf};
use std::process::Stdio;

use tokio::process::Command;
use tracing::{info, warn};

use crate::backend::types::{AgentProvider, ProviderPaths};

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
                .join(format!("claude-code-acp-{}", target_triple));
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

/// Find the Claude Code CLI executable
/// Security: Only checks known installation paths
/// If custom_path is provided, it's checked first (after env var)
pub(crate) fn find_claude_code_executable(custom_path: Option<&str>) -> Option<PathBuf> {
    // Highest priority: explicit override via environment variable
    if let Ok(env_path) = std::env::var("CLAUDE_CODE_EXECUTABLE") {
        let candidate = PathBuf::from(env_path);
        if candidate.exists() {
            if let Ok(canonical) = std::fs::canonicalize(&candidate) {
                info!(
                    "Using CLAUDE_CODE_EXECUTABLE override at {:?} (resolves to: {:?})",
                    candidate, canonical
                );
            } else {
                info!("Using CLAUDE_CODE_EXECUTABLE override at {:?}", candidate);
            }
            return Some(candidate);
        } else {
            warn!(
                "CLAUDE_CODE_EXECUTABLE override does not exist at {:?}",
                candidate
            );
        }
    }

    // Second priority: user-configured custom path from settings
    if let Some(custom) = custom_path {
        let candidate = PathBuf::from(custom);
        if candidate.exists() {
            if let Ok(canonical) = std::fs::canonicalize(&candidate) {
                info!(
                    "Using custom Claude CLI path at {:?} (resolves to: {:?})",
                    candidate, canonical
                );
            } else {
                info!("Using custom Claude CLI path at {:?}", candidate);
            }
            return Some(candidate);
        } else {
            warn!("Custom Claude CLI path does not exist at {:?}", candidate);
        }
    }

    // Known installation paths (in order of preference)
    let known_paths = [
        // Homebrew on Apple Silicon
        "/opt/homebrew/bin/claude",
        // Homebrew on Intel Mac
        "/usr/local/bin/claude",
    ];

    for path_str in known_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            // Log canonical path for debugging, but return original path for execution
            // (Homebrew symlinks point to wrapper scripts that must be executed directly)
            if let Ok(canonical) = std::fs::canonicalize(&path) {
                info!(
                    "Found Claude CLI at {:?} (resolves to: {:?})",
                    path, canonical
                );
            } else {
                info!("Found Claude CLI at {:?}", path);
            }
            return Some(path);
        }
    }

    // Native install script location and common user-local installs
    // Use dirs crate pattern for home directory (more reliable than HOME env var)
    if let Some(home) = dirs::home_dir() {
        let native_install = home.join(".claude/local/claude");
        let local_bin = home.join(".local/bin/claude"); // XDG-style local bin
        let bun_install = home.join(".bun/bin/claude");
        let npm_global = home.join(".npm-global/bin/claude");

        for path in [native_install, local_bin, bun_install, npm_global] {
            if path.exists() {
                if let Ok(canonical) = std::fs::canonicalize(&path) {
                    info!(
                        "Found Claude CLI at {:?} (resolves to: {:?})",
                        path, canonical
                    );
                } else {
                    info!("Found Claude CLI at {:?}", path);
                }
                return Some(path);
            }
        }

        // nvm-managed npm globals: iterate known Node versions (no globbing)
        let nvm_base = home.join(".nvm/versions/node");
        if let Ok(entries) = std::fs::read_dir(&nvm_base) {
            for entry in entries.flatten() {
                let candidate = entry.path().join("bin/claude");
                if candidate.exists() {
                    if let Ok(canonical) = std::fs::canonicalize(&candidate) {
                        info!(
                            "Found Claude CLI in nvm path {:?} (resolves to: {:?})",
                            candidate, canonical
                        );
                    } else {
                        info!("Found Claude CLI in nvm path {:?}", candidate);
                    }
                    return Some(candidate);
                }
            }
        }
    }

    // Security: We intentionally do NOT fall back to PATH lookup via `which`
    // This prevents PATH injection attacks where a malicious binary could be executed
    warn!("Claude Code CLI not found in any known location");
    None
}

/// Find the Gemini CLI executable
/// Security: Only checks known installation paths
/// If custom_path is provided, it's checked first
pub(crate) fn find_gemini_cli_executable(custom_path: Option<&str>) -> Option<PathBuf> {
    // First priority: user-configured custom path from settings
    if let Some(custom) = custom_path {
        let candidate = PathBuf::from(custom);
        if candidate.exists() {
            if let Ok(canonical) = std::fs::canonicalize(&candidate) {
                info!(
                    "Using custom Gemini CLI path at {:?} (resolves to: {:?})",
                    candidate, canonical
                );
            } else {
                info!("Using custom Gemini CLI path at {:?}", candidate);
            }
            return Some(candidate);
        } else {
            warn!("Custom Gemini CLI path does not exist at {:?}", candidate);
        }
    }

    // Known installation paths (in order of preference)
    let known_paths = [
        // Homebrew on Apple Silicon
        "/opt/homebrew/bin/gemini",
        // Homebrew on Intel Mac
        "/usr/local/bin/gemini",
    ];

    for path_str in known_paths {
        let path = PathBuf::from(path_str);
        if path.exists() {
            // Log canonical path for debugging, but return original path for execution
            // (Homebrew symlinks point to wrapper scripts that must be executed directly)
            if let Ok(canonical) = std::fs::canonicalize(&path) {
                info!(
                    "Found Gemini CLI at {:?} (resolves to: {:?})",
                    path, canonical
                );
            } else {
                info!("Found Gemini CLI at {:?}", path);
            }
            return Some(path);
        }
    }

    // Check user-local installation paths
    if let Some(home) = dirs::home_dir() {
        let user_paths = [
            // bun global install
            home.join(".bun/bin/gemini"),
            // npm global install (standard location)
            home.join(".npm-global/bin/gemini"),
            // nvm-managed npm global
            home.join(".nvm/versions/node").join("*/bin/gemini"),
        ];

        for path in user_paths {
            // Skip glob patterns (nvm path) - would need expansion
            if path.to_string_lossy().contains('*') {
                continue;
            }
            if path.exists() {
                if let Ok(canonical) = std::fs::canonicalize(&path) {
                    info!(
                        "Found Gemini CLI at {:?} (resolves to: {:?})",
                        path, canonical
                    );
                } else {
                    info!("Found Gemini CLI at {:?}", path);
                }
                return Some(path);
            }
        }
    }

    // Security: We intentionally do NOT fall back to PATH lookup via `which`
    // This prevents PATH injection attacks where a malicious binary could be executed
    warn!("Gemini CLI not found in any known location");
    None
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
    let claude_cli_path = find_claude_code_executable(custom_path).ok_or_else(|| {
        anyhow::anyhow!(
            "Claude Code CLI not found.\n\
             Please install it: brew install --cask claude-code\n\
             Or: npm install -g @anthropic-ai/claude-code"
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
        .map_err(|e| anyhow::anyhow!("Failed to spawn sidecar: {}", e))?;

    Ok(child)
}

/// Spawn Gemini CLI in ACP mode
pub(crate) async fn spawn_gemini_cli_acp(
    notes_directory: &Path,
    custom_path: Option<&str>,
    model_id: Option<&str>,
) -> anyhow::Result<tokio::process::Child> {
    let gemini_path = find_gemini_cli_executable(custom_path).ok_or_else(|| {
        anyhow::anyhow!(
            "Gemini CLI not found.\n\
             Install via: brew install gemini-cli\n\
             Or: bun install -g @google/gemini-cli"
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
        .map_err(|e| anyhow::anyhow!("Failed to spawn Gemini CLI: {}", e))?;

    Ok(child)
}

/// Spawn an ACP-compatible agent subprocess based on provider
pub(crate) async fn spawn_agent_subprocess(
    provider: &AgentProvider,
    notes_directory: &Path,
    paths: &ProviderPaths,
    model_id: Option<&str>,
) -> anyhow::Result<tokio::process::Child> {
    match provider {
        AgentProvider::ClaudeCode => {
            spawn_claude_code_acp(notes_directory, paths.claude_code.as_deref()).await
        }
        AgentProvider::GeminiCli => {
            // Gemini CLI requires model to be specified at spawn time via --model flag
            spawn_gemini_cli_acp(notes_directory, paths.gemini_cli.as_deref(), model_id).await
        }
    }
}
