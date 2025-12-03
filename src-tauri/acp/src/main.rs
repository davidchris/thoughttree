use std::io::Write;
use std::path::PathBuf;
use std::process::Stdio;
use std::sync::Arc;

use agent_client_protocol::{
    Agent, Client, ClientCapabilities, ClientSideConnection, ContentBlock, FileSystemCapability,
    Implementation, InitializeRequest, NewSessionRequest, PromptRequest, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SessionNotification, SessionUpdate,
    TextContent, VERSION,
};
use anyhow::{Context, Result};
use async_trait::async_trait;
use tokio::process::{Child, Command};
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{debug, error, info, warn};
use tracing_subscriber::EnvFilter;

/// Minimal ACP client that auto-approves permissions and streams responses.
struct MinimalClient;

#[async_trait(?Send)]
impl Client for MinimalClient {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        // Auto-approve by selecting the first option (typically "Allow")
        info!("Permission requested: {:?}", args.tool_call);

        // Get the first option's ID, or create a placeholder if no options provided
        let outcome = if let Some(first_opt) = args.options.first() {
            RequestPermissionOutcome::Selected {
                option_id: first_opt.id.clone(),
            }
        } else {
            // No options provided, just cancel (shouldn't happen normally)
            warn!("No permission options provided, cancelling");
            RequestPermissionOutcome::Cancelled
        };

        Ok(RequestPermissionResponse {
            outcome,
            meta: None,
        })
    }

    async fn session_notification(
        &self,
        args: SessionNotification,
    ) -> agent_client_protocol::Result<()> {
        // Handle streaming updates from the agent
        match args.update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let ContentBlock::Text(text) = chunk.content {
                    print!("{}", text.text);
                    let _ = std::io::stdout().flush();
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                if let ContentBlock::Text(text) = chunk.content {
                    debug!("[Thought] {}", text.text);
                }
            }
            SessionUpdate::ToolCall(tc) => {
                info!("[Tool Call] {:?}", tc);
            }
            SessionUpdate::ToolCallUpdate(update) => {
                debug!("[Tool Update] {:?}", update);
            }
            SessionUpdate::Plan(plan) => {
                debug!("[Plan] {:?}", plan);
            }
            _ => {
                debug!("[Other update] {:?}", args.update);
            }
        }
        Ok(())
    }
}

/// Spawn the claude-code-acp subprocess.
async fn spawn_claude_code_acp() -> Result<Child> {
    info!("Spawning claude-code-acp...");

    let child = Command::new("npx")
        .args(["@zed-industries/claude-code-acp"])
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context(
            "Failed to spawn claude-code-acp. Ensure you have:\n\
             1. Node.js and npm installed\n\
             2. Run: npx @zed-industries/claude-code-acp (first time may need to confirm install)",
        )?;

    Ok(child)
}

/// Main async logic for the ACP client.
async fn run() -> Result<()> {
    // Spawn the ACP adapter subprocess
    let mut child = spawn_claude_code_acp().await?;

    // Get stdin/stdout handles
    let stdin = child
        .stdin
        .take()
        .context("Failed to get stdin handle from subprocess")?;
    let stdout = child
        .stdout
        .take()
        .context("Failed to get stdout handle from subprocess")?;

    // Spawn a task to log stderr
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

    // Create the ACP connection
    // ClientSideConnection::new(client, outgoing_bytes, incoming_bytes, spawn)
    // outgoing_bytes = stdin to subprocess (we write to it)
    // incoming_bytes = stdout from subprocess (we read from it)
    info!("Creating ACP connection...");
    let client = Arc::new(MinimalClient);

    let (connection, io_future) = ClientSideConnection::new(
        client,
        stdin.compat_write(),  // outgoing bytes (write to subprocess stdin)
        stdout.compat(),       // incoming bytes (read from subprocess stdout)
        |f| {
            tokio::task::spawn_local(f);
        },
    );

    // Run the I/O processing in the background
    tokio::task::spawn_local(async move {
        if let Err(e) = io_future.await {
            error!("I/O error: {:?}", e);
        }
    });

    // Initialize the connection
    info!("Initializing connection...");
    let init_response = connection
        .initialize(InitializeRequest {
            protocol_version: VERSION,
            client_capabilities: ClientCapabilities {
                fs: FileSystemCapability {
                    read_text_file: false,
                    write_text_file: false,
                    meta: None,
                },
                terminal: false,
                meta: None,
            },
            client_info: Some(Implementation {
                name: "acp-client-prototype".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("ACP Client Prototype".to_string()),
            }),
            meta: None,
        })
        .await
        .context("Failed to initialize connection")?;

    info!(
        "Connected to agent: {:?} (protocol: {})",
        init_response.agent_info, init_response.protocol_version
    );

    // Create a new session
    info!("Creating session...");
    let cwd = std::env::current_dir().context("Failed to get current directory")?;
    let session_response = connection
        .new_session(NewSessionRequest {
            cwd: PathBuf::from(cwd),
            mcp_servers: vec![],
            meta: None,
        })
        .await
        .context("Failed to create session")?;

    info!("Session created: {}", session_response.session_id);

    // Send a prompt
    info!("Sending prompt...");
    println!("\n--- Response ---\n");

    let prompt_response = connection
        .prompt(PromptRequest {
            session_id: session_response.session_id,
            prompt: vec![ContentBlock::Text(TextContent {
                text: "What is the Agent Client Protocol? Explain briefly in 2-3 sentences."
                    .to_string(),
                annotations: None,
                meta: None,
            })],
            meta: None,
        })
        .await
        .context("Failed to send prompt")?;

    println!("\n\n--- End Response ---");
    info!("Stop reason: {:?}", prompt_response.stop_reason);

    // Clean shutdown
    info!("Shutting down...");
    drop(connection);

    // Wait for subprocess to exit
    let _ = child.wait().await;

    Ok(())
}

#[tokio::main(flavor = "current_thread")]
async fn main() -> Result<()> {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(EnvFilter::from_default_env().add_directive("info".parse().unwrap()))
        .init();

    // Run in a LocalSet for non-Send futures
    let local = tokio::task::LocalSet::new();
    local.run_until(run()).await
}
