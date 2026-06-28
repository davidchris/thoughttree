use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use agent_client_protocol::{
    Agent, Client, ClientSideConnection, ContentBlock, ImageContent, Implementation,
    InitializeRequest, InitializeResponse, NewSessionRequest, PromptRequest, ProtocolVersion,
    SetSessionModelRequest, TextContent,
};
use chrono::Local;
use futures::lock::Mutex;
use tokio::sync::oneshot;
use tokio::task::JoinHandle;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{error, info, warn};

use crate::backend::acp::clients::{ModelDiscoveryClient, StreamingClient, SummaryClient};
use crate::backend::acp::process::{spawn_agent_subprocess, spawn_claude_code_acp};
use crate::backend::types::{AgentProvider, Message, ModelInfo, ProviderPaths};

/// How long to wait for the agent subprocess to answer `initialize` before
/// giving up. A broken sidecar otherwise hangs the request forever.
const INIT_TIMEOUT: Duration = Duration::from_secs(15);

/// How long to wait for the subprocess to exit on its own after stdin closes,
/// before killing it.
const EXIT_TIMEOUT: Duration = Duration::from_secs(2);

/// An ACP agent subprocess together with its stderr-logging and connection
/// I/O tasks, so teardown can wait for all of them instead of leaking.
struct AgentProcess {
    child: tokio::process::Child,
    stderr_task: Option<JoinHandle<()>>,
    io_task: JoinHandle<()>,
}

impl AgentProcess {
    /// Gracefully shut down: the caller must drop the connection first (which
    /// closes the subprocess's stdin), then this waits for exit and drains the
    /// I/O and stderr tasks. Kills the process if it doesn't exit in time.
    /// On early-error paths where this isn't reached, `kill_on_drop(true)`
    /// still terminates the subprocess.
    async fn shutdown(mut self, tag: &str) {
        match tokio::time::timeout(EXIT_TIMEOUT, self.child.wait()).await {
            Ok(Ok(status)) => info!("[{}] subprocess exited: {}", tag, status),
            Ok(Err(e)) => warn!("[{}] failed waiting on subprocess: {}", tag, e),
            Err(_) => {
                warn!(
                    "[{}] subprocess did not exit after stdin close; killing",
                    tag
                );
                if let Err(e) = self.child.kill().await {
                    warn!("[{}] failed to kill subprocess: {}", tag, e);
                }
            }
        }
        let _ = self.io_task.await;
        if let Some(task) = self.stderr_task {
            let _ = task.await;
        }
    }
}

/// Wire up an ACP connection over the child's stdio and start the stderr
/// logger and connection I/O tasks.
fn connect_agent(
    mut child: tokio::process::Child,
    client: Arc<impl Client + 'static>,
    tag: &'static str,
) -> anyhow::Result<(ClientSideConnection, AgentProcess)> {
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

    let (connection, io_future) =
        ClientSideConnection::new(client, stdin.compat_write(), stdout.compat(), |f| {
            tokio::task::spawn_local(f);
        });

    let io_task = tokio::task::spawn_local(async move {
        if let Err(e) = io_future.await {
            error!("[{}] I/O error: {:?}", tag, e);
        }
    });

    Ok((
        connection,
        AgentProcess {
            child,
            stderr_task,
            io_task,
        },
    ))
}

/// Run `initialize` with a timeout so a wedged subprocess can't hang the UI.
async fn initialize_with_timeout(
    connection: &ClientSideConnection,
    client_info: Implementation,
) -> anyhow::Result<InitializeResponse> {
    tokio::time::timeout(
        INIT_TIMEOUT,
        connection
            .initialize(InitializeRequest::new(ProtocolVersion::LATEST).client_info(client_info)),
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
pub(crate) struct PromptSessionParams {
    pub app_handle: tauri::AppHandle,
    pub node_id: String,
    pub messages: Vec<Message>,
    pub pending_permissions: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
    pub notes_directory: PathBuf,
    pub provider: AgentProvider,
    pub model_id: Option<String>,
    pub provider_paths: ProviderPaths,
}

/// Run a prompt session with ACP
pub(crate) async fn run_prompt_session(params: PromptSessionParams) -> anyhow::Result<String> {
    let PromptSessionParams {
        app_handle,
        node_id,
        messages,
        pending_permissions,
        notes_directory,
        provider,
        model_id,
        provider_paths,
    } = params;
    // Spawn the ACP subprocess in the notes directory so skills are loaded
    // For Gemini, model_id is passed at spawn time via --model flag
    let child = spawn_agent_subprocess(
        &provider,
        &notes_directory,
        &provider_paths,
        model_id.as_deref(),
    )
    .await?;

    // Create client with notes directory for permission filtering
    let client = Arc::new(StreamingClient::new(
        app_handle,
        node_id,
        pending_permissions,
        notes_directory.clone(),
    ));

    info!("Creating ACP connection...");
    let (connection, process) = connect_agent(child, client, "claude-code-acp")?;

    // Initialize
    info!("Initializing connection...");
    let init_response = initialize_with_timeout(
        &connection,
        Implementation::new("thoughttree", env!("CARGO_PKG_VERSION")).title("ThoughtTree"),
    )
    .await?;

    info!(
        "Connected to agent: {:?} (protocol: {})",
        init_response.agent_info, init_response.protocol_version
    );

    // Create session with notes directory as cwd
    info!("Creating session with cwd: {:?}", notes_directory);
    let session_response = connection
        .new_session(NewSessionRequest::new(notes_directory))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create session: {e:?}"))?;

    info!("Session created: {}", session_response.session_id);

    // Switch model if specified
    if let Some(ref model) = model_id {
        info!("Switching to model: {}", model);
        connection
            .set_session_model(SetSessionModelRequest::new(
                session_response.session_id.clone(),
                agent_client_protocol::ModelId::new(model.clone()),
            ))
            .await
            .map_err(|e| anyhow::anyhow!("Failed to set model: {e:?}"))?;
    }

    // Get current date and format it
    let current_date = Local::now().format("%B %d, %Y").to_string();
    let date_prefix = format!("Current date: {current_date}\n\n");

    // Build prompt from conversation messages
    let prompt_text = messages
        .iter()
        .map(|msg| format!("{}: {}", msg.role, msg.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    // Prepend current date to the prompt
    let prompt_text = format!("{date_prefix}{prompt_text}");

    // Build content blocks: images first, then text
    // Claude processes images before text for better understanding
    let mut content_blocks: Vec<ContentBlock> = Vec::new();

    // Add all images from all messages
    for msg in &messages {
        if let Some(images) = &msg.images {
            for img in images {
                info!("Adding image: mime_type={}", img.mime_type);
                content_blocks.push(ContentBlock::Image(ImageContent::new(
                    img.data.clone(),
                    img.mime_type.clone(),
                )));
            }
        }
    }

    // Validate we have content to send
    if prompt_text.trim().is_empty() && content_blocks.is_empty() {
        return Err(anyhow::anyhow!("Cannot send empty prompt"));
    }

    // Add text content if present
    if !prompt_text.trim().is_empty() {
        content_blocks.push(ContentBlock::Text(TextContent::new(prompt_text)));
    }

    // Send prompt
    info!(
        "Sending prompt with {} content blocks ({} images)...",
        content_blocks.len(),
        content_blocks
            .iter()
            .filter(|b| matches!(b, ContentBlock::Image(_)))
            .count()
    );
    let prompt_response = connection
        .prompt(PromptRequest::new(
            session_response.session_id,
            content_blocks,
        ))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send prompt: {e:?}"))?;

    info!("Stop reason: {:?}", prompt_response.stop_reason);

    // Dropping the connection closes the subprocess's stdin; shutdown then
    // waits for exit and drains the I/O and stderr tasks.
    drop(connection);
    process.shutdown("claude-code-acp").await;

    Ok(format!("{:?}", prompt_response.stop_reason))
}

/// Derive a display name from a model ID
fn model_id_to_display_name(model_id: &str) -> String {
    // Common patterns: "claude-opus-4-5-20251101" -> "Opus 4.5"
    // "claude-sonnet-4-5-20250929" -> "Sonnet 4.5"
    // "gemini-2.5-pro" -> "Gemini 2.5 Pro"
    let id_lower = model_id.to_lowercase();

    if id_lower.contains("opus") {
        if id_lower.contains("4-5") || id_lower.contains("4.5") {
            "Opus 4.5".to_string()
        } else {
            "Opus".to_string()
        }
    } else if id_lower.contains("sonnet") {
        if id_lower.contains("4-5") || id_lower.contains("4.5") {
            "Sonnet 4.5".to_string()
        } else if id_lower.contains("4-") || id_lower.contains("4.") {
            "Sonnet 4".to_string()
        } else {
            "Sonnet".to_string()
        }
    } else if id_lower.contains("haiku") {
        if id_lower.contains("4-5") || id_lower.contains("4.5") {
            "Haiku 4.5".to_string()
        } else {
            "Haiku".to_string()
        }
    } else if id_lower.contains("gemini") {
        // Handle Gemini models: gemini-2.5-pro, gemini-2.5-flash
        let mut name = String::new();
        if id_lower.contains("2.5") || id_lower.contains("2-5") {
            name.push_str("Gemini 2.5 ");
        } else if id_lower.contains("2.0") || id_lower.contains("2-0") {
            name.push_str("Gemini 2.0 ");
        } else {
            name.push_str("Gemini ");
        }
        if id_lower.contains("pro") {
            name.push_str("Pro");
        } else if id_lower.contains("flash") {
            name.push_str("Flash");
        }
        if name.ends_with(' ') {
            name.pop();
        }
        name
    } else {
        // Fallback: just return the model_id
        model_id.to_string()
    }
}

pub(crate) async fn run_model_discovery_session(
    notes_directory: PathBuf,
    provider: AgentProvider,
    provider_paths: ProviderPaths,
) -> Result<Vec<ModelInfo>, String> {
    // Spawn the ACP subprocess (model_id is None for discovery - we're just fetching available models)
    let child = spawn_agent_subprocess(&provider, &notes_directory, &provider_paths, None)
        .await
        .map_err(|e| format!("Failed to spawn agent: {e}"))?;

    // Create minimal client
    let client = Arc::new(ModelDiscoveryClient);

    let (connection, process) =
        connect_agent(child, client, "model-discovery").map_err(|e| e.to_string())?;

    // Initialize
    let _init_response = initialize_with_timeout(
        &connection,
        Implementation::new("thoughttree", env!("CARGO_PKG_VERSION")).title("ThoughtTree"),
    )
    .await
    .map_err(|e| e.to_string())?;

    // Create session to get models
    let session_response = connection
        .new_session(NewSessionRequest::new(&notes_directory))
        .await
        .map_err(|e| format!("Failed to create session: {e:?}"))?;

    // Extract models from response
    let models: Vec<ModelInfo> = session_response
        .models
        .map(|m| {
            m.available_models
                .into_iter()
                .map(|model| ModelInfo {
                    display_name: model_id_to_display_name(&model.model_id.0),
                    model_id: model.model_id.0.to_string(),
                })
                .collect()
        })
        .unwrap_or_default();

    // Gemini CLI doesn't expose models via ACP, so provide fallback options
    // These correspond to the --model flag values for `gemini` CLI
    let models = if models.is_empty() && matches!(provider, AgentProvider::GeminiCli) {
        info!("Gemini CLI returned no models via ACP, using fallback model list");
        vec![
            ModelInfo {
                model_id: "gemini-3".to_string(),
                display_name: "Gemini 3 (Auto)".to_string(),
            },
            ModelInfo {
                model_id: "gemini-2.5".to_string(),
                display_name: "Gemini 2.5 (Auto)".to_string(),
            },
        ]
    } else {
        models
    };

    info!(
        "Discovered {} models for {:?}: {:?}",
        models.len(),
        provider,
        models.iter().map(|m| &m.model_id).collect::<Vec<_>>()
    );

    drop(connection);
    process.shutdown("model-discovery").await;

    Ok(models)
}

/// Run a summarization session with Haiku model
pub(crate) async fn run_summary_session(
    content: String,
    notes_directory: PathBuf,
    custom_path: Option<String>,
) -> anyhow::Result<String> {
    // Spawn ACP subprocess
    let child = spawn_claude_code_acp(&notes_directory, custom_path.as_deref()).await?;

    let client = Arc::new(SummaryClient::new());
    let response_text = client.response_text.clone();

    let (connection, process) = connect_agent(child, client, "summary-acp")?;

    // Initialize. This doubles as the readiness handshake: stdin writes are
    // buffered by the pipe, so no startup delay is needed.
    info!("Summary session: initializing connection...");
    let init_response = initialize_with_timeout(
        &connection,
        Implementation::new("thoughttree-summarizer", env!("CARGO_PKG_VERSION")),
    )
    .await?;

    info!(
        "Summary session connected to: {:?}",
        init_response.agent_info
    );

    // Create session
    let session_response = connection
        .new_session(NewSessionRequest::new(&notes_directory))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create session: {e:?}"))?;

    // Try to switch to Haiku if available
    if let Some(models) = &session_response.models {
        // Look for Haiku model
        let haiku = models.available_models.iter().find(|m| {
            let id = m.model_id.0.to_lowercase();
            id.contains("haiku")
        });

        if let Some(haiku_model) = haiku {
            info!("Switching to Haiku model: {}", haiku_model.model_id.0);
            let _ = connection
                .set_session_model(SetSessionModelRequest::new(
                    session_response.session_id.clone(),
                    haiku_model.model_id.clone(),
                ))
                .await;
        } else {
            info!(
                "Haiku not found, using default model: {}",
                models.current_model_id.0
            );
        }
    }

    // Truncate content to avoid huge inputs
    let truncated_content = if content.len() > 2000 {
        format!("{}...", &content[..2000])
    } else {
        content
    };

    // Build summarization prompt
    let prompt_text = format!(
        "Write a 3-5 word heading that describes what this text is about. \
         Be specific and concise. Do not call any tools. Return ONLY the heading, nothing else:\n\n{truncated_content}"
    );

    // Send prompt and wait for completion
    let prompt_result = connection
        .prompt(PromptRequest::new(
            session_response.session_id,
            vec![ContentBlock::Text(TextContent::new(prompt_text))],
        ))
        .await;

    if let Err(e) = prompt_result {
        warn!("Summary prompt failed: {:?}", e);
    }

    // Clean up
    drop(connection);
    process.shutdown("summary-acp").await;

    // Get result and clean it up
    let result = response_text.lock().await.trim().to_string();

    // Remove any quotes the model might have added
    let result = result.trim_matches('"').trim_matches('\'').trim();

    // Truncate if too long (aim for ~40 chars max)
    if result.len() > 40 {
        Ok(format!("{}…", &result[..37]))
    } else {
        Ok(result.to_string())
    }
}
