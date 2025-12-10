use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use agent_client_protocol::{
    Agent, Client, ClientCapabilities, ClientSideConnection, ContentBlock, FileSystemCapability,
    Implementation, InitializeRequest, NewSessionRequest, PermissionOptionId, PromptRequest,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SessionNotification, SessionUpdate, TextContent, VERSION,
};
use async_trait::async_trait;
use futures::lock::Mutex;
use tauri::{AppHandle, Emitter, State};
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_store::StoreExt;
use tokio::process::Command;
use tokio::sync::oneshot;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};
use tracing::{debug, error, info, warn};

// Types for frontend communication
#[derive(Clone, serde::Serialize)]
struct ChunkPayload {
    node_id: String,
    chunk: String,
}

#[derive(Clone, serde::Serialize)]
struct PermissionPayload {
    id: String,
    tool_type: String,
    tool_name: String,
    description: String,
    options: Vec<PermissionOption>,
}

#[derive(Clone, serde::Serialize)]
struct PermissionOption {
    id: String,
    label: String,
}

// App state for managing permission responses
pub struct AppState {
    pending_permissions: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            pending_permissions: Arc::new(Mutex::new(HashMap::new())),
        }
    }
}

/// ACP Client that streams to frontend and handles permissions via UI
struct StreamingClient {
    app_handle: AppHandle,
    node_id: String,
    pending_permissions: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
    notes_directory: PathBuf,
}

impl StreamingClient {
    fn new(
        app_handle: AppHandle,
        node_id: String,
        pending_permissions: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
        notes_directory: PathBuf,
    ) -> Self {
        Self {
            app_handle,
            node_id,
            pending_permissions,
            notes_directory,
        }
    }

    /// Prompt user for permission via frontend dialog
    async fn prompt_user_for_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        // Generate unique request ID
        let request_id = uuid::Uuid::new_v4().to_string();

        // Create channel for response
        let (tx, rx) = oneshot::channel();

        // Store sender for later
        {
            let mut pending = self.pending_permissions.lock().await;
            pending.insert(request_id.clone(), tx);
        }

        // Build description from tool call
        let tool_type = args.tool_call.id.0.to_string();
        let tool_name = args
            .tool_call
            .fields
            .title
            .clone()
            .unwrap_or_else(|| "Unknown tool".to_string());

        // Format locations or other details as description
        let description = if let Some(locations) = &args.tool_call.fields.locations {
            locations
                .iter()
                .map(|loc| loc.path.display().to_string())
                .collect::<Vec<_>>()
                .join(", ")
        } else {
            "No additional details".to_string()
        };

        // Build options
        let options: Vec<PermissionOption> = args
            .options
            .iter()
            .map(|opt| PermissionOption {
                id: opt.id.0.to_string(),
                label: opt.name.clone(),
            })
            .collect();

        // Emit permission request to frontend
        let payload = PermissionPayload {
            id: request_id.clone(),
            tool_type,
            tool_name,
            description,
            options,
        };

        if let Err(e) = self.app_handle.emit("permission-request", payload) {
            error!("Failed to emit permission request: {:?}", e);
            let mut pending = self.pending_permissions.lock().await;
            pending.remove(&request_id);
            return Ok(RequestPermissionResponse {
                outcome: RequestPermissionOutcome::Cancelled,
                meta: None,
            });
        }

        // Wait for response from frontend
        match rx.await {
            Ok(option_id_str) => {
                info!("Permission response received: {}", option_id_str);
                let option_id = PermissionOptionId(Arc::from(option_id_str.as_str()));
                Ok(RequestPermissionResponse {
                    outcome: RequestPermissionOutcome::Selected { option_id },
                    meta: None,
                })
            }
            Err(_) => {
                warn!("Permission request cancelled (channel dropped)");
                Ok(RequestPermissionResponse {
                    outcome: RequestPermissionOutcome::Cancelled,
                    meta: None,
                })
            }
        }
    }
}

#[async_trait(?Send)]
impl Client for StreamingClient {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        let tool_name = args
            .tool_call
            .fields
            .title
            .as_deref()
            .unwrap_or("Unknown");
        let tool_id = args.tool_call.id.0.to_string();

        info!(
            "Permission requested - tool: {} (id: {})",
            tool_name, tool_id
        );

        // DENY: Bash, Write, Edit, and any execution/modification tools
        // ThoughtTree is for thinking, not doing!
        let denied_patterns = [
            "Bash",
            "Write",
            "Edit",
            "NotebookEdit",
            "TodoWrite",
            "Task",
            "bash",
            "write",
            "edit",
        ];
        if denied_patterns
            .iter()
            .any(|p| tool_name.contains(p) || tool_id.contains(p))
        {
            warn!(
                "Tool '{}' denied - ThoughtTree only allows read-only operations",
                tool_name
            );
            return Ok(RequestPermissionResponse {
                outcome: RequestPermissionOutcome::Cancelled,
                meta: None,
            });
        }

        // AUTO-APPROVE: Read-only search tools (within notes directory) and Skills
        let auto_approve_patterns = ["Read", "Grep", "Glob", "WebSearch", "Skill"];
        if auto_approve_patterns
            .iter()
            .any(|p| tool_name.contains(p))
        {
            // For file operations, validate they're within notes_directory
            if let Some(locations) = &args.tool_call.fields.locations {
                for loc in locations {
                    if !loc.path.starts_with(&self.notes_directory) {
                        warn!(
                            "Tool '{}' denied - path {:?} is outside notes directory {:?}",
                            tool_name, loc.path, self.notes_directory
                        );
                        return Ok(RequestPermissionResponse {
                            outcome: RequestPermissionOutcome::Cancelled,
                            meta: None,
                        });
                    }
                }
            }

            // Auto-approve by selecting first option
            if let Some(first_opt) = args.options.first() {
                info!("Auto-approving tool '{}'", tool_name);
                return Ok(RequestPermissionResponse {
                    outcome: RequestPermissionOutcome::Selected {
                        option_id: first_opt.id.clone(),
                    },
                    meta: None,
                });
            }
        }

        // PROMPT USER: WebFetch (per-session approval)
        if tool_name.contains("WebFetch") {
            info!("Prompting user for WebFetch permission");
            return self.prompt_user_for_permission(args).await;
        }

        // DEFAULT: Deny unknown tools
        warn!("Unknown tool '{}' denied by default", tool_name);
        Ok(RequestPermissionResponse {
            outcome: RequestPermissionOutcome::Cancelled,
            meta: None,
        })
    }

    async fn session_notification(
        &self,
        args: SessionNotification,
    ) -> agent_client_protocol::Result<()> {
        match args.update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let ContentBlock::Text(text) = chunk.content {
                    // Send chunk to frontend
                    let payload = ChunkPayload {
                        node_id: self.node_id.clone(),
                        chunk: text.text,
                    };
                    if let Err(e) = self.app_handle.emit("stream-chunk", payload) {
                        error!("Failed to emit chunk: {:?}", e);
                    }
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

/// Spawn the claude-code-acp subprocess
async fn spawn_claude_code_acp(notes_directory: &Path) -> anyhow::Result<tokio::process::Child> {
    info!("Spawning claude-code-acp in {:?}...", notes_directory);

    let child = Command::new("npx")
        .args(["@zed-industries/claude-code-acp"])
        .current_dir(notes_directory)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .map_err(|e| anyhow::anyhow!(
            "Failed to spawn claude-code-acp: {}. Ensure you have:\n\
             1. Node.js and npm installed\n\
             2. Run: npx @zed-industries/claude-code-acp (first time may need to confirm install)",
            e
        ))?;

    Ok(child)
}

/// Run a prompt session with ACP
async fn run_prompt_session(
    app_handle: AppHandle,
    node_id: String,
    messages: Vec<(String, String)>,
    pending_permissions: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
    notes_directory: PathBuf,
) -> anyhow::Result<String> {
    // Spawn the ACP subprocess in the notes directory so skills are loaded
    let mut child = spawn_claude_code_acp(&notes_directory).await?;

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
    let (connection, io_future) = ClientSideConnection::new(
        client,
        stdin.compat_write(),
        stdout.compat(),
        |f| {
            tokio::task::spawn_local(f);
        },
    );

    // Run I/O in background
    tokio::task::spawn_local(async move {
        if let Err(e) = io_future.await {
            error!("I/O error: {:?}", e);
        }
    });

    // Initialize
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
                name: "thoughttree".to_string(),
                version: env!("CARGO_PKG_VERSION").to_string(),
                title: Some("ThoughtTree".to_string()),
            }),
            meta: None,
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to initialize: {:?}", e))?;

    info!(
        "Connected to agent: {:?} (protocol: {})",
        init_response.agent_info, init_response.protocol_version
    );

    // Create session with notes directory as cwd
    info!("Creating session with cwd: {:?}", notes_directory);
    let session_response = connection
        .new_session(NewSessionRequest {
            cwd: notes_directory,
            mcp_servers: vec![],
            meta: None,
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create session: {:?}", e))?;

    info!("Session created: {}", session_response.session_id);

    // Build prompt from conversation messages
    // Only include the last user message as the prompt, context comes from session
    let prompt_text = messages
        .iter()
        .map(|(role, content)| format!("{}: {}", role, content))
        .collect::<Vec<_>>()
        .join("\n\n");

    // Validate prompt is not empty
    if prompt_text.trim().is_empty() {
        return Err(anyhow::anyhow!("Cannot send empty prompt"));
    }

    // Send prompt
    info!("Sending prompt...");
    let prompt_response = connection
        .prompt(PromptRequest {
            session_id: session_response.session_id,
            prompt: vec![ContentBlock::Text(TextContent {
                text: prompt_text,
                annotations: None,
                meta: None,
            })],
            meta: None,
        })
        .await
        .map_err(|e| anyhow::anyhow!("Failed to send prompt: {:?}", e))?;

    info!("Stop reason: {:?}", prompt_response.stop_reason);

    // Clean shutdown - just drop the child, kill_on_drop(true) will terminate it
    drop(connection);
    drop(child);

    Ok(format!("{:?}", prompt_response.stop_reason))
}

#[tauri::command]
async fn send_prompt(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    node_id: String,
    messages: Vec<(String, String)>,
) -> Result<String, String> {
    let pending_permissions = state.pending_permissions.clone();

    // Load notes directory from config store
    let notes_directory = {
        let store = app_handle
            .store("config.json")
            .map_err(|e| format!("Failed to open config store: {}", e))?;

        store
            .get("notes_directory")
            .and_then(|v| v.as_str().map(PathBuf::from))
            .ok_or_else(|| {
                "Notes directory not configured. Please set it in settings.".to_string()
            })?
    };

    info!("Using notes directory: {:?}", notes_directory);

    // Run in LocalSet for non-Send futures
    let result = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to create runtime: {}", e))?;

        let local = tokio::task::LocalSet::new();
        local.block_on(&rt, async move {
            run_prompt_session(
                app_handle,
                node_id,
                messages,
                pending_permissions,
                notes_directory,
            )
            .await
        })
        .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?;

    result
}

#[tauri::command]
async fn respond_to_permission(
    state: State<'_, AppState>,
    request_id: String,
    option_id: String,
) -> Result<(), String> {
    let mut pending = state.pending_permissions.lock().await;

    if let Some(sender) = pending.remove(&request_id) {
        sender
            .send(option_id)
            .map_err(|_| "Failed to send permission response")?;
        Ok(())
    } else {
        Err(format!("No pending permission request with ID: {}", request_id))
    }
}

#[tauri::command]
async fn check_acp_available() -> Result<bool, String> {
    // Check if npx and claude-code-acp are available
    let output = tokio::process::Command::new("npx")
        .args(["@zed-industries/claude-code-acp", "--version"])
        .output()
        .await;

    Ok(output.is_ok())
}

// ============================================================================
// Configuration commands
// ============================================================================

#[tauri::command]
async fn get_notes_directory(app: AppHandle) -> Result<Option<String>, String> {
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    Ok(store
        .get("notes_directory")
        .and_then(|v| v.as_str().map(String::from)))
}

#[tauri::command]
async fn set_notes_directory(app: AppHandle, path: String) -> Result<(), String> {
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    store
        .set("notes_directory", serde_json::json!(path));

    store
        .save()
        .map_err(|e| format!("Failed to save config: {}", e))?;

    info!("Notes directory set to: {}", path);
    Ok(())
}

#[tauri::command]
async fn pick_notes_directory(app: AppHandle) -> Result<Option<String>, String> {
    use tauri_plugin_dialog::DialogExt;

    let path = app
        .dialog()
        .file()
        .set_title("Select Notes Directory")
        .blocking_pick_folder();

    Ok(path.map(|p| p.to_string()))
}

// ============================================================================
// Project file commands
// ============================================================================

#[tauri::command]
async fn save_project(path: String, data: String) -> Result<(), String> {
    std::fs::write(&path, &data).map_err(|e| format!("Failed to save project: {}", e))?;
    info!("Project saved to: {}", path);
    Ok(())
}

#[tauri::command]
async fn load_project(path: String) -> Result<String, String> {
    let data = std::fs::read_to_string(&path)
        .map_err(|e| format!("Failed to load project: {}", e))?;
    info!("Project loaded from: {}", path);
    Ok(data)
}

#[tauri::command]
async fn new_project_dialog(app: AppHandle) -> Result<Option<String>, String> {
    // Get notes directory as default location
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    let default_dir = store
        .get("notes_directory")
        .and_then(|v| v.as_str().map(PathBuf::from));

    let mut dialog = app
        .dialog()
        .file()
        .set_title("Save New Project")
        .add_filter("ThoughtTree Project", &["thoughttree"])
        .set_file_name("untitled.thoughttree");

    if let Some(dir) = default_dir {
        dialog = dialog.set_directory(dir);
    }

    let path = dialog.blocking_save_file();

    Ok(path.map(|p| p.to_string()))
}

#[tauri::command]
async fn open_project_dialog(app: AppHandle) -> Result<Option<String>, String> {
    // Get notes directory as default location
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    let default_dir = store
        .get("notes_directory")
        .and_then(|v| v.as_str().map(PathBuf::from));

    let mut dialog = app
        .dialog()
        .file()
        .set_title("Open Project")
        .add_filter("ThoughtTree Project", &["thoughttree"]);

    if let Some(dir) = default_dir {
        dialog = dialog.set_directory(dir);
    }

    let path = dialog.blocking_pick_file();

    Ok(path.map(|p| p.to_string()))
}

#[tauri::command]
async fn export_markdown(app: AppHandle, content: String, default_name: String) -> Result<Option<String>, String> {
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Export as Markdown")
        .add_filter("Markdown", &["md"])
        .set_file_name(&default_name);

    // Get notes directory as default location
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    if let Some(dir) = store.get("notes_directory").and_then(|v| v.as_str().map(PathBuf::from)) {
        dialog = dialog.set_directory(dir);
    }

    let path = dialog.blocking_save_file();

    if let Some(p) = path {
        let path_str = p.to_string();
        std::fs::write(&path_str, &content)
            .map_err(|e| format!("Failed to export markdown: {}", e))?;
        info!("Exported markdown to: {}", path_str);
        Ok(Some(path_str))
    } else {
        Ok(None)
    }
}

#[tauri::command]
async fn search_files(
    app: AppHandle,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    // Get notes directory from config
    let notes_directory = {
        let store = app
            .store("config.json")
            .map_err(|e| format!("Failed to open config store: {}", e))?;

        store
            .get("notes_directory")
            .and_then(|v| v.as_str().map(PathBuf::from))
            .ok_or_else(|| "Notes directory not configured".to_string())?
    };

    let max_results = limit.unwrap_or(20);

    // Use fd for fast gitignore-respecting file search
    // If query is empty, use "." to match all files
    let search_pattern = if query.is_empty() { ".".to_string() } else { query };

    let output = std::process::Command::new("fd")
        .args([
            "--type",
            "f",
            "--follow",
            "--max-results",
            &max_results.to_string(),
            &search_pattern,
        ])
        .current_dir(&notes_directory)
        .output()
        .map_err(|e| format!("Failed to execute fd: {}", e))?;

    if !output.status.success() {
        // fd returns non-zero for no matches, which is fine
        return Ok(vec![]);
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let files: Vec<String> = stdout
        .lines()
        .filter(|line| !line.is_empty())
        .map(|line| line.to_string())
        .collect();

    Ok(files)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    // Initialize logging
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            send_prompt,
            respond_to_permission,
            check_acp_available,
            // Config commands
            get_notes_directory,
            set_notes_directory,
            pick_notes_directory,
            // Project commands
            save_project,
            load_project,
            new_project_dialog,
            open_project_dialog,
            export_markdown,
            // File search
            search_files,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
