use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::schema::v1::{
    ContentBlock, Implementation, InitializeRequest, InitializeResponse, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SessionNotification, TextContent,
};
use agent_client_protocol::schema::ProtocolVersion;
use agent_client_protocol::{
    on_receive_notification, on_receive_request, Agent, ByteStreams, Client, ConnectionTo,
};
use chrono::Local;
use tokio::task::JoinHandle;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{info, warn};

use crate::acp::attachments::build_prompt_blocks;
use crate::acp::clients::{ModelDiscoveryClient, SessionClient, StreamingClient, SummaryClient};
use crate::acp::process::spawn_agent_subprocess;
use crate::acp::provenance::TurnOutcome;
use crate::acp::session_setup::{new_session, set_model, SessionSetup};
use crate::events::SessionEventSink;
use crate::permissions::PermissionBroker;
use crate::types::{AgentProvider, Message, ModelInfo, ProviderPaths, ReasoningEffort};

/// Effort for housekeeping sessions (summaries, model discovery): the lowest
/// on the scale, universally supported — these calls must stay fast and cheap
/// and are deliberately not user-configurable.
const HOUSEKEEPING_EFFORT: ReasoningEffort = ReasoningEffort::Low;

/// How long to wait for the agent subprocess to answer `initialize` before
/// giving up. A broken sidecar otherwise hangs the request forever.
const INIT_TIMEOUT: Duration = Duration::from_secs(15);

/// How long to wait for the subprocess to exit on its own after stdin closes,
/// before killing it.
const EXIT_TIMEOUT: Duration = Duration::from_secs(2);

fn should_set_session_model(
    model_id: Option<&str>,
    agent_offers_model_switch: bool,
    provider: &AgentProvider,
) -> bool {
    model_id.is_some() && agent_offers_model_switch && matches!(provider, AgentProvider::ClaudeCode)
}

/// Run `main_fn` over an ACP connection to `child`'s stdio, with `client`
/// handling agent-to-client traffic, then shut the subprocess down.
///
/// Returning from `main_fn` tears down the connection (closing the child's
/// stdin); the child is then given [`EXIT_TIMEOUT`] to exit before it is
/// killed. On early-error paths `kill_on_drop(true)` still terminates it.
async fn connect_agent<T>(
    mut child: tokio::process::Child,
    client: Arc<impl SessionClient>,
    tag: &'static str,
    main_fn: impl AsyncFnOnce(ConnectionTo<Agent>) -> anyhow::Result<T>,
) -> anyhow::Result<T> {
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to get stdin handle"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to get stdout handle"))?;

    let stderr_task = child.stderr.take().map(|stderr| {
        tokio::task::spawn_local(async move {
            use tokio::io::AsyncBufReadExt;
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!("[{} stderr] {}", tag, line);
            }
        })
    });

    // Handlers run inside the connection's dispatch loop, which processes one
    // incoming message at a time.
    let notifier = client.clone();
    let permitter = client;
    let result = Client
        .builder()
        .name(tag)
        .on_receive_notification(
            async move |notification: SessionNotification, _cx: ConnectionTo<Agent>| {
                notifier.session_notification(notification).await
            },
            on_receive_notification!(),
        )
        .on_receive_request(
            async move |request: RequestPermissionRequest, responder, cx: ConnectionTo<Agent>| {
                // Permission prompts can wait on the user; answer from a
                // spawned task so streaming updates keep flowing meanwhile.
                // A failing spawned task would tear down the whole connection,
                // so failures degrade to a denial instead.
                let client = permitter.clone();
                cx.spawn(async move {
                    let response = match client.request_permission(request).await {
                        Ok(response) => response,
                        Err(e) => {
                            warn!("[{tag}] permission handler failed, denying: {e:?}");
                            RequestPermissionResponse::new(RequestPermissionOutcome::Cancelled)
                        }
                    };
                    if let Err(e) = responder.respond(response) {
                        warn!("[{tag}] could not answer permission request: {e:?}");
                    }
                    Ok(())
                })
            },
            on_receive_request!(),
        )
        .connect_with(
            ByteStreams::new(stdin.compat_write(), stdout.compat()),
            async |cx| Ok(main_fn(cx).await),
        )
        .await;

    shutdown_agent(child, stderr_task, tag).await;

    result.map_err(|e| anyhow::anyhow!("[{tag}] ACP connection failed: {e:?}"))?
}

/// The connection (and with it the child's stdin) is already gone: wait for
/// exit, kill on timeout, and drain the stderr logger.
async fn shutdown_agent(
    mut child: tokio::process::Child,
    stderr_task: Option<JoinHandle<()>>,
    tag: &str,
) {
    match tokio::time::timeout(EXIT_TIMEOUT, child.wait()).await {
        Ok(Ok(status)) => info!("[{}] subprocess exited: {}", tag, status),
        Ok(Err(e)) => warn!("[{}] failed waiting on subprocess: {}", tag, e),
        Err(_) => {
            warn!(
                "[{}] subprocess did not exit after stdin close; killing",
                tag
            );
            if let Err(e) = child.kill().await {
                warn!("[{}] failed to kill subprocess: {}", tag, e);
            }
        }
    }
    if let Some(task) = stderr_task {
        let _ = task.await;
    }
}

/// Run `initialize` with a timeout so a wedged subprocess can't hang the UI.
async fn initialize_with_timeout(
    cx: &ConnectionTo<Agent>,
    client_info: Implementation,
) -> anyhow::Result<InitializeResponse> {
    tokio::time::timeout(
        INIT_TIMEOUT,
        cx.send_request(InitializeRequest::new(ProtocolVersion::V1).client_info(client_info))
            .block_task(),
    )
    .await
    .map_err(|_| {
        anyhow::anyhow!(
            "Agent did not respond to initialize within {}s",
            INIT_TIMEOUT.as_secs()
        )
    })?
    .map_err(|e| anyhow::anyhow!("Failed to initialize: {e:?}"))
}

/// Parameters for [`run_prompt_session`]
pub struct PromptSessionParams<S> {
    pub sink: S,
    pub node_id: String,
    pub turn_id: String,
    pub messages: Vec<Message>,
    pub broker: PermissionBroker,
    pub notes_directory: PathBuf,
    pub provider: AgentProvider,
    pub model_id: Option<String>,
    pub effort: Option<ReasoningEffort>,
    pub provider_paths: ProviderPaths,
}

/// Run a prompt session with ACP
pub async fn run_prompt_session<S: SessionEventSink>(
    params: PromptSessionParams<S>,
) -> anyhow::Result<String> {
    let PromptSessionParams {
        sink,
        node_id,
        turn_id,
        messages,
        broker,
        notes_directory,
        provider,
        model_id,
        effort,
        provider_paths,
    } = params;

    // Build content blocks before spawning anything: images, then file
    // pointers, then the text. Attachment errors (missing file, over-limit
    // image) surface here, so no adapter process is started for a prompt that
    // cannot be sent.
    let current_date = Local::now().format("%B %d, %Y").to_string();
    let date_prefix = format!("Current date: {current_date}\n\n");
    let content_blocks = build_prompt_blocks(&notes_directory, &messages, &date_prefix)?;

    // Spawn the ACP subprocess in the notes directory so skills are loaded
    let child = spawn_agent_subprocess(
        &provider,
        &notes_directory,
        &provider_paths,
        model_id.as_deref(),
        effort,
    )
    .await?;

    // Create client with notes directory for permission filtering
    let client = Arc::new(StreamingClient::new(
        sink,
        node_id.clone(),
        turn_id.clone(),
        broker,
        notes_directory.clone(),
    ));

    info!("Creating ACP connection...");
    let tag = provider.descriptor().id;
    let result = connect_agent(child, client.clone(), tag, async |cx| {
        info!("Initializing connection...");
        let init_response = initialize_with_timeout(
            &cx,
            Implementation::new("thoughttree", env!("CARGO_PKG_VERSION")).title("ThoughtTree"),
        )
        .await?;

        info!(
            "Connected to agent: {:?} (protocol: {})",
            init_response.agent_info, init_response.protocol_version
        );

        // Create session with notes directory as cwd
        info!("Creating session with cwd: {:?}", notes_directory);
        let session = new_session(&cx, &notes_directory).await?;

        info!(%node_id, %turn_id, session_id = %session.session_id, "Prompt session created");

        // Switch model if specified and this provider's model selection belongs
        // to ACP session state. Codex is configured at spawn time.
        if let Some(ref model) = model_id {
            if should_set_session_model(
                model_id.as_deref(),
                session.offers_model_switch(),
                &provider,
            ) {
                info!("Switching to model: {}", model);
                set_model(&cx, &session, model).await?;
            } else {
                info!(
                    "Skipping session model switch for {:?}; {model} was applied at spawn",
                    provider
                );
            }
        }

        // Send prompt
        info!(
            "Sending prompt with {} content blocks ({} images, {} resource links)...",
            content_blocks.len(),
            content_blocks
                .iter()
                .filter(|b| matches!(b, ContentBlock::Image(_)))
                .count(),
            content_blocks
                .iter()
                .filter(|b| matches!(b, ContentBlock::ResourceLink(_)))
                .count()
        );
        let prompt_response = cx
            .send_request(PromptRequest::new(session.session_id, content_blocks))
            .block_task()
            .await
            .map_err(|e| anyhow::anyhow!("Failed to send prompt: {e:?}"))?;

        info!("Stop reason: {:?}", prompt_response.stop_reason);
        Ok(format!("{:?}", prompt_response.stop_reason))
    })
    .await;
    // Provenance is emitted on every outcome, so a cancelled or failed Turn
    // still records what the agent did before it stopped.
    client.close_turn(if result.is_ok() {
        TurnOutcome::Finished
    } else {
        TurnOutcome::Aborted
    });
    result
}

pub async fn run_model_discovery_session(
    notes_directory: PathBuf,
    provider: AgentProvider,
    provider_paths: ProviderPaths,
) -> Result<Vec<ModelInfo>, String> {
    // Providers whose adapters report no models over ACP are served straight
    // from the curated fallback list — no point spawning a subprocess and
    // running the handshake just to learn nothing.
    if !provider.descriptor().models_via_acp {
        return Ok(with_fallback_models(&provider, vec![]));
    }

    // Spawn the ACP subprocess (model_id is None for discovery - we're just fetching available models)
    let child = spawn_agent_subprocess(
        &provider,
        &notes_directory,
        &provider_paths,
        None,
        Some(HOUSEKEEPING_EFFORT),
    )
    .await
    .map_err(|e| format!("Failed to spawn agent: {e}"))?;

    // Create minimal client
    let client = Arc::new(ModelDiscoveryClient);

    let session = connect_agent(child, client, "model-discovery", async |cx| {
        let _init_response = initialize_with_timeout(
            &cx,
            Implementation::new("thoughttree", env!("CARGO_PKG_VERSION")).title("ThoughtTree"),
        )
        .await?;

        // Create session to get models
        new_session(&cx, &notes_directory).await
    })
    .await
    .map_err(|e| e.to_string())?;

    let models = with_fallback_models(&provider, session.available_models(&provider));

    info!(
        "Discovered {} models for {:?}: {:?}",
        models.len(),
        provider,
        models.iter().map(|m| &m.model_id).collect::<Vec<_>>()
    );

    Ok(models)
}

/// Use the curated catalog when an older adapter exposes no models.
fn with_fallback_models(provider: &AgentProvider, models: Vec<ModelInfo>) -> Vec<ModelInfo> {
    let fallback = provider.descriptor().fallback_models;
    if !models.is_empty() || fallback.is_empty() {
        return models;
    }

    info!(
        "{} returned no models via ACP, using fallback model list",
        provider.display_name()
    );
    fallback
        .iter()
        .map(|(model_id, display_name)| ModelInfo {
            model_id: model_id.to_string(),
            display_name: display_name.to_string(),
        })
        .collect()
}

/// Cheapest model to switch a summary session to, when the agent offers one.
fn summary_model_switch(provider: &AgentProvider, session: &SessionSetup) -> Option<String> {
    session
        .available_models(provider)
        .into_iter()
        .map(|model| model.model_id)
        .find(|id| id.to_lowercase().contains("haiku"))
}

/// Generate a short heading with the selected provider.
pub async fn run_summary_session(
    content: String,
    notes_directory: PathBuf,
    provider: AgentProvider,
    provider_paths: ProviderPaths,
) -> anyhow::Result<String> {
    let truncated_content: String = content.chars().take(2000).collect();
    let suffix = if truncated_content.len() < content.len() {
        "..."
    } else {
        ""
    };
    let prompt_text = format!(
        "Write a 3-5 word heading that describes what this text is about. \
         Be specific and concise. Do not call any tools. Return ONLY the heading, nothing else:\n\n{truncated_content}{suffix}"
    );
    if matches!(provider, AgentProvider::Codex) {
        let result =
            super::codex_summary::run(&prompt_text, &notes_directory, &provider_paths).await?;
        return Ok(clean_summary(&result));
    }
    let child = spawn_agent_subprocess(
        &provider,
        &notes_directory,
        &provider_paths,
        None,
        Some(HOUSEKEEPING_EFFORT),
    )
    .await?;

    let client = Arc::new(SummaryClient::new());
    let response_text = client.response_text.clone();

    connect_agent(child, client, "summary-acp", async |cx| {
        // Initialize. This doubles as the readiness handshake: stdin writes are
        // buffered by the pipe, so no startup delay is needed.
        info!("Summary session: initializing connection...");
        let init_response = initialize_with_timeout(
            &cx,
            Implementation::new("thoughttree-summarizer", env!("CARGO_PKG_VERSION")),
        )
        .await?;

        info!(
            "Summary session connected to: {:?}",
            init_response.agent_info
        );

        // Create session
        let session = new_session(&cx, &notes_directory).await?;

        // Try to switch to Haiku if available
        match summary_model_switch(&provider, &session)
            .filter(|_| matches!(provider, AgentProvider::ClaudeCode))
        {
            Some(haiku) => {
                info!("Switching to Haiku model: {}", haiku);
                if let Err(e) = set_model(&cx, &session, &haiku).await {
                    warn!("{e}");
                }
            }
            None => {
                if let Some(current) = session.current_model() {
                    info!("Summary session model: {}", current);
                }
            }
        }

        // Send prompt and wait for completion
        let prompt_result = cx
            .send_request(PromptRequest::new(
                session.session_id,
                vec![ContentBlock::Text(TextContent::new(prompt_text))],
            ))
            .block_task()
            .await;

        if let Err(e) = prompt_result {
            warn!("Summary prompt failed: {:?}", e);
        }
        Ok(())
    })
    .await?;

    // Get result and clean it up
    let result = clean_summary(&response_text.lock().await);
    Ok(result)
}

fn clean_summary(result: &str) -> String {
    // Remove any quotes the model might have added
    let result = result.trim().trim_matches('"').trim_matches('\'').trim();

    // Truncate if too long (aim for ~40 chars max)
    if result.chars().count() > 40 {
        format!("{}…", result.chars().take(37).collect::<String>())
    } else {
        result.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(unix)]
    #[test]
    fn codex_summary_uses_ephemeral_exec_instead_of_acp() {
        use std::os::unix::fs::PermissionsExt;
        let dir = tempfile::tempdir().unwrap();
        let script = r#"#!/bin/sh
printf '%s\n' "$@" > args
case " $* " in
  *' --ephemeral '*) ;;
  *) exit 42 ;;
esac
cat > prompt
printf '%s\n' 'A useful short heading'
"#;
        for name in ["codex", "codex-acp"] {
            let path = dir.path().join(name);
            std::fs::write(&path, script).unwrap();
            std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        }
        let mut paths = ProviderPaths::default();
        paths.set(
            &AgentProvider::Codex,
            Some(dir.path().join("codex-acp").to_string_lossy().into_owned()),
        );
        let result = tokio::runtime::Runtime::new().unwrap().block_on(
            tokio::task::LocalSet::new().run_until(run_summary_session(
                format!("Content to summarize {}", "é".repeat(2000)),
                dir.path().into(),
                AgentProvider::Codex,
                paths.clone(),
            )),
        );
        assert_eq!(result.unwrap(), "A useful short heading");
        let args = std::fs::read_to_string(dir.path().join("args")).unwrap();
        assert!(args.starts_with("exec\n"));
        assert!(args.lines().any(|arg| arg == "--ephemeral"));
        assert!(args.lines().any(|arg| arg == "gpt-5.6-luna"));
        assert!(args.lines().any(|arg| arg == "read-only"));
        assert!(args.lines().any(|arg| arg == "--skip-git-repo-check"));
        assert!(std::fs::read_to_string(dir.path().join("prompt"))
            .unwrap()
            .contains("Content to summarize"));

        for (script, expected) in [
            (
                "#!/bin/sh\ncat >/dev/null\necho partial\necho failed >&2\nexit 1\n",
                "Codex summary failed",
            ),
            ("#!/bin/sh\ncat >/dev/null\n", "empty summary"),
        ] {
            std::fs::write(dir.path().join("codex"), script).unwrap();
            let result = tokio::runtime::Runtime::new().unwrap().block_on(
                tokio::task::LocalSet::new().run_until(run_summary_session(
                    "Content".into(),
                    dir.path().into(),
                    AgentProvider::Codex,
                    paths.clone(),
                )),
            );
            assert!(result.unwrap_err().to_string().contains(expected));
        }
    }

    #[test]
    fn summary_cleanup_handles_multibyte_headings() {
        assert_eq!(clean_summary("  \"A short heading\"\n"), "A short heading");
        assert_eq!(
            clean_summary(&"é".repeat(41)),
            format!("{}…", "é".repeat(37))
        );
    }

    fn setup(json: serde_json::Value) -> SessionSetup {
        SessionSetup::parse(json).unwrap()
    }

    #[test]
    fn summary_switches_to_haiku_when_offered() {
        // claude-code-acp 0.16 still lists models on the legacy field.
        let session = setup(serde_json::json!({
            "sessionId": "test",
            "models": {
                "currentModelId": "claude-opus-4-5",
                "availableModels": [
                    {"modelId": "claude-opus-4-5", "name": "Opus"},
                    {"modelId": "claude-haiku-4-5", "name": "Haiku"}
                ]
            }
        }));
        let provider = AgentProvider::ClaudeCode;
        assert_eq!(
            summary_model_switch(&provider, &session).as_deref(),
            Some("claude-haiku-4-5")
        );
        assert_eq!(
            summary_model_switch(&provider, &setup(serde_json::json!({"sessionId": "t"}))),
            None
        );
    }

    #[test]
    fn test_fallback_models_come_from_descriptor_when_discovery_is_empty() {
        // Older adapters without discovery still offer the fallback catalog.
        let provider = AgentProvider::Codex;
        let models = with_fallback_models(&provider, vec![]);

        let expected: Vec<(String, String)> = provider
            .descriptor()
            .fallback_models
            .iter()
            .map(|(id, name)| (id.to_string(), name.to_string()))
            .collect();
        let actual: Vec<(String, String)> = models
            .iter()
            .map(|m| (m.model_id.clone(), m.display_name.clone()))
            .collect();

        assert!(
            !models.is_empty(),
            "{provider:?} must offer fallback models"
        );
        assert_eq!(actual, expected);
    }

    #[test]
    fn test_discovered_models_are_kept_when_present() {
        let discovered = vec![ModelInfo {
            model_id: "gpt-future".to_string(),
            display_name: "Future model".to_string(),
        }];

        let models = with_fallback_models(&AgentProvider::Codex, discovered.clone());

        assert_eq!(models.len(), 1);
        assert_eq!(models[0].model_id, "gpt-future");
    }

    #[test]
    fn test_provider_without_fallbacks_returns_empty_when_discovery_is_empty() {
        let models = with_fallback_models(&AgentProvider::ClaudeCode, vec![]);
        assert!(models.is_empty());
    }

    #[test]
    fn test_codex_model_is_not_set_via_session_even_when_adapter_advertises_models() {
        assert!(!should_set_session_model(
            Some("gpt-5.5"),
            true,
            &AgentProvider::Codex
        ));
    }

    #[test]
    fn test_claude_model_is_set_via_session_when_adapter_advertises_models() {
        assert!(should_set_session_model(
            Some("sonnet"),
            true,
            &AgentProvider::ClaudeCode
        ));
    }

    #[test]
    fn test_session_model_is_not_set_without_model_or_model_state() {
        assert!(!should_set_session_model(
            None,
            true,
            &AgentProvider::ClaudeCode
        ));
        assert!(!should_set_session_model(
            Some("sonnet"),
            false,
            &AgentProvider::ClaudeCode
        ));
    }
}
