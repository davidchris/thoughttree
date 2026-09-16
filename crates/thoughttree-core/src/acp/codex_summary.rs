use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::time::Duration;

use tokio::io::AsyncWriteExt;

use super::process::{adapter_command, find_provider_executable};
use crate::types::{AgentProvider, ProviderPaths};

fn cli_path(paths: &ProviderPaths) -> anyhow::Result<PathBuf> {
    // Match the adapter's explicit CLI override. Otherwise prefer a CLI in
    // the same installation as the configured adapter, including nvm installs.
    if let Some(path) = std::env::var_os("CODEX_PATH") {
        return Ok(path.into());
    }
    let adapter = find_provider_executable(
        &AgentProvider::Codex,
        paths.get(&AgentProvider::Codex).map(String::as_str),
    );
    let mut candidates = Vec::new();
    if let Some(parent) = adapter.as_deref().and_then(Path::parent) {
        candidates.push(parent.join("codex"));
    }
    // npm adapters can carry their own Codex dependency without a global CLI.
    if let Some(package) = adapter
        .as_deref()
        .and_then(|path| path.canonicalize().ok())
        .and_then(|path| path.parent()?.parent().map(Path::to_path_buf))
    {
        candidates.push(package.join("node_modules/@openai/codex/bin/codex.js"));
    }
    candidates.extend(["/opt/homebrew/bin/codex", "/usr/local/bin/codex"].map(PathBuf::from));
    if let Some(home) = dirs::home_dir() {
        candidates.extend(
            [
                ".local/bin/codex",
                ".bun/bin/codex",
                ".npm-global/bin/codex",
            ]
            .map(|p| home.join(p)),
        );
    }
    candidates.into_iter().find(|path| path.is_file()).ok_or_else(|| {
        anyhow::anyhow!("Codex CLI not found for ephemeral summaries. Install @openai/codex or set CODEX_PATH.")
    })
}

/// ACP session/new creates a persistent Codex thread. Housekeeping must use
/// exec's explicit ephemeral mode instead; never fall back to a saved session.
pub(super) async fn run(
    prompt: &str,
    directory: &Path,
    paths: &ProviderPaths,
) -> anyhow::Result<String> {
    let mut command = adapter_command(&cli_path(paths)?);
    command
        .args([
            "exec",
            "--ephemeral",
            "--skip-git-repo-check",
            "--sandbox",
            "read-only",
            "--color",
            "never",
            "--model",
            "gpt-5.6-luna",
            "-c",
            "model_reasoning_effort=low",
            "-c",
            "approval_policy=never",
            "-",
        ])
        .current_dir(directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    let output = tokio::time::timeout(Duration::from_secs(90), async {
        let mut child = command.spawn()?;
        let mut stdin = child.stdin.take().expect("stdin is piped");
        stdin.write_all(prompt.as_bytes()).await?;
        drop(stdin);
        child.wait_with_output().await
    })
    .await
    .map_err(|_| anyhow::anyhow!("Codex summary timed out after 90s"))??;
    if !output.status.success() {
        anyhow::bail!(
            "Codex summary failed ({}): {}",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim()
        );
    }
    // exec prints progress to stderr and only the final message to stdout.
    let heading = String::from_utf8(output.stdout)?;
    anyhow::ensure!(
        !heading.trim().is_empty(),
        "Codex returned an empty summary"
    );
    Ok(heading)
}
