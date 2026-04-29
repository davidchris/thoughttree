use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::{
    Agent, ClientSideConnection, ContentBlock, ImageContent, Implementation, InitializeRequest,
    NewSessionRequest, PromptRequest, ProtocolVersion, SetSessionModelRequest, TextContent,
};
use chrono::Local;
use futures::lock::Mutex;
use tokio::sync::oneshot;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{error, info, warn};

use crate::backend::acp::clients::{ModelDiscoveryClient, StreamingClient, SummaryClient};
use crate::backend::acp::process::{spawn_agent_subprocess, spawn_claude_code_acp};
use crate::backend::types::{AgentProvider, Message, ModelInfo, ProviderPaths};

/// Run a prompt session with ACP
pub(crate) async fn run_prompt_session(
    app_handle: tauri::AppHandle,
    node_id: String,
    messages: Vec<Message>,
    pending_permissions: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
    notes_directory: PathBuf,
    provider: AgentProvider,
    model_id: Option<String>,
    provider_paths: ProviderPaths,
) -> anyhow::Result<String> {
    // Spawn the ACP subprocess in the notes directory so skills are loaded
    // For Gemini, model_id is passed at spawn time via --model flag
    let mut child = spawn_agent_subprocess(
        &provider,
        &notes_directory,
        &provider_paths,
        model_id.as_deref(),
    )
    .await?;

    // Get stdin/stdout handles
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to get stdin handle"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to get stdout handle"))?;

    // Log stderr
    if let Some(stderr) = child.stderr.take() {
        tokio::task::spawn_local(async move {
            use tokio::io::AsyncBufReadExt;
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!("[claude-code-acp stderr] {}", line);
            }
        });
    }

    // Create client with notes directory for permission filtering
    let client = Arc::new(StreamingClient::new(
        app_handle,
        node_id,
        pending_permissions,
        notes_directory.clone(),
    ));

    // Create connection
    info!("Creating ACP connection...");
    let (connection, io_future) =
        ClientSideConnection::new(client, stdin.compat_write(), stdout.compat(), |f| {
            tokio::task::spawn_local(f);
        });

    // Run I/O in background
    tokio::task::spawn_local(async move {
        if let Err(e) = io_future.await {
            error!("I/O error: {:?}", e);
        }
    });

    // Initialize
    info!("Initializing connection...");
    let init_response = connection
        .initialize(InitializeRequest::new(ProtocolVersion::LATEST).client_info(
            Implementation::new("thoughttree", env!("CARGO_PKG_VERSION")).title("ThoughtTree"),
        ))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize: {:?}", e))?;

    info!(
        "Connected to agent: {:?} (protocol: {})",
        init_response.agent_info, init_response.protocol_version
    );

    // Create session with notes directory as cwd
    info!("Creating session with cwd: {:?}", notes_directory);
    let session_response = connection
        .new_session(NewSessionRequest::new(notes_directory))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create session: {:?}", e))?;

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
            .map_err(|e| anyhow::anyhow!("Failed to set model: {:?}", e))?;
    }

    // Get current date and format it
    let current_date = Local::now().format("%B %d, %Y").to_string();
    let date_prefix = format!("Current date: {}\n\n", current_date);

    // Build prompt from conversation messages
    let prompt_text = messages
        .iter()
        .map(|msg| format!("{}: {}", msg.role, msg.content))
        .collect::<Vec<_>>()
        .join("\n\n");

    // Prepend current date to the prompt
    let prompt_text = format!("{}{}", date_prefix, prompt_text);

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
        .map_err(|e| anyhow::anyhow!("Failed to send prompt: {:?}", e))?;

    info!("Stop reason: {:?}", prompt_response.stop_reason);

    // Clean shutdown - just drop the child, kill_on_drop(true) will terminate it
    drop(connection);
    drop(child);

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
    let mut child = spawn_agent_subprocess(&provider, &notes_directory, &provider_paths, None)
        .await
        .map_err(|e| format!("Failed to spawn agent: {}", e))?;

    // Get stdin/stdout handles
    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| "Failed to get stdin handle".to_string())?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| "Failed to get stdout handle".to_string())?;

    // Drop stderr - we don't need it for discovery
    drop(child.stderr.take());

    // Create minimal client
    let client = Arc::new(ModelDiscoveryClient);

    // Create connection
    let (connection, io_future) =
        ClientSideConnection::new(client, stdin.compat_write(), stdout.compat(), |f| {
            tokio::task::spawn_local(f);
        });

    // Run I/O in background
    tokio::task::spawn_local(async move {
        let _ = io_future.await;
    });

    // Initialize
    let _init_response = connection
        .initialize(InitializeRequest::new(ProtocolVersion::LATEST).client_info(
            Implementation::new("thoughttree", env!("CARGO_PKG_VERSION")).title("ThoughtTree"),
        ))
        .await
        .map_err(|e| format!("Failed to initialize: {:?}", e))?;

    // Create session to get models
    let session_response = connection
        .new_session(NewSessionRequest::new(&notes_directory))
        .await
        .map_err(|e| format!("Failed to create session: {:?}", e))?;

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

    // Child process will be dropped and killed here
    Ok(models)
}

/// Run a summarization session with Haiku model
pub(crate) async fn run_summary_session(
    content: String,
    notes_directory: PathBuf,
    custom_path: Option<String>,
) -> anyhow::Result<String> {
    // Spawn ACP subprocess
    let mut child = spawn_claude_code_acp(&notes_directory, custom_path.as_deref()).await?;

    let stdin = child
        .stdin
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to get stdin handle"))?;
    let stdout = child
        .stdout
        .take()
        .ok_or_else(|| anyhow::anyhow!("Failed to get stdout handle"))?;

    // Log stderr for debugging
    if let Some(stderr) = child.stderr.take() {
        tokio::task::spawn_local(async move {
            use tokio::io::AsyncBufReadExt;
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                warn!("[summary-acp stderr] {}", line);
            }
        });
    }

    // Small delay to ensure subprocess is ready
    tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;

    let client = Arc::new(SummaryClient::new());
    let response_text = client.response_text.clone();

    // Create connection
    let (connection, io_future) =
        ClientSideConnection::new(client, stdin.compat_write(), stdout.compat(), |f| {
            tokio::task::spawn_local(f);
        });

    // Run I/O in background
    tokio::task::spawn_local(async move {
        if let Err(e) = io_future.await {
            error!("[summary] I/O error: {:?}", e);
        }
    });

    // Initialize
    info!("Summary session: initializing connection...");
    let init_response = connection
        .initialize(InitializeRequest::new(ProtocolVersion::LATEST).client_info(
            Implementation::new("thoughttree-summarizer", env!("CARGO_PKG_VERSION")),
        ))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize summary session: {:?}", e))?;

    info!(
        "Summary session connected to: {:?}",
        init_response.agent_info
    );

    // Create session
    let session_response = connection
        .new_session(NewSessionRequest::new(&notes_directory))
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create session: {:?}", e))?;

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
         Be specific and concise. Do not call any tools. Return ONLY the heading, nothing else:\n\n{}",
        truncated_content
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
    drop(child);

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
