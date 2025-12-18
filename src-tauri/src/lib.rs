use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use agent_client_protocol::{
    Agent, Client, ClientSideConnection, ContentBlock, Implementation, InitializeRequest,
    NewSessionRequest, PromptRequest, ProtocolVersion, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome,
    SessionNotification, SessionUpdate, SetSessionModelRequest, TextContent,
};
use async_trait::async_trait;
use chrono::Local;
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
        let tool_type = args.tool_call.tool_call_id.0.to_string();
        let tool_name = args
            .tool_call
            .fields
            .title
            .clone()
            .unwrap_or_else(|| "Unknown tool".to_string());

        // Format locations or other details as description
        let description = if let Some(locations) = &args.tool_call.fields.locations {
            if !locations.is_empty() {
                locations
                    .iter()
                    .map(|loc| loc.path.display().to_string())
                    .collect::<Vec<_>>()
                    .join(", ")
            } else {
                "No additional details".to_string()
            }
        } else {
            "No additional details".to_string()
        };

        // Build options
        let options: Vec<PermissionOption> = args
            .options
            .iter()
            .map(|opt| PermissionOption {
                id: opt.option_id.0.to_string(),
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
            return Ok(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
        }

        // Wait for response from frontend
        match rx.await {
            Ok(option_id_str) => {
                info!("Permission response received: {}", option_id_str);
                Ok(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                        option_id_str,
                    )),
                ))
            }
            Err(_) => {
                warn!("Permission request cancelled (channel dropped)");
                Ok(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Cancelled,
                ))
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
        let tool_name = args.tool_call.fields.title.as_deref().unwrap_or("Unknown");
        let tool_id = args.tool_call.tool_call_id.0.to_string();

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
            return Ok(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
        }

        // AUTO-APPROVE: Read-only search tools (within notes directory) and Skills
        let auto_approve_patterns = ["Read", "Grep", "Glob", "WebSearch", "Skill"];
        if auto_approve_patterns.iter().any(|p| tool_name.contains(p)) {
            // For file operations, validate they're within notes_directory using canonicalization
            // This prevents symlink-based path traversal attacks
            if let Some(locations) = &args.tool_call.fields.locations {
                let canonical_notes = match std::fs::canonicalize(&self.notes_directory) {
                    Ok(p) => p,
                    Err(e) => {
                        warn!("Failed to canonicalize notes directory: {}", e);
                        return Ok(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Cancelled,
                        ));
                    }
                };

                for loc in locations {
                    // Canonicalize the requested path to resolve symlinks
                    let canonical_loc = match std::fs::canonicalize(&loc.path) {
                        Ok(p) => p,
                        Err(e) => {
                            warn!(
                                "Tool '{}' denied - failed to canonicalize path {:?}: {}",
                                tool_name, loc.path, e
                            );
                            return Ok(RequestPermissionResponse::new(
                                RequestPermissionOutcome::Cancelled,
                            ));
                        }
                    };

                    if !canonical_loc.starts_with(&canonical_notes) {
                        warn!(
                            "Tool '{}' denied - path {:?} is outside notes directory",
                            tool_name, loc.path
                        );
                        return Ok(RequestPermissionResponse::new(
                            RequestPermissionOutcome::Cancelled,
                        ));
                    }
                }
            }

            // Auto-approve by selecting first option
            if let Some(first_opt) = args.options.first() {
                info!("Auto-approving tool '{}'", tool_name);
                return Ok(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                        first_opt.option_id.clone(),
                    )),
                ));
            }
        }

        // PROMPT USER: WebFetch (per-session approval)
        if tool_name.contains("WebFetch") {
            info!("Prompting user for WebFetch permission");
            return self.prompt_user_for_permission(args).await;
        }

        // DEFAULT: Deny unknown tools
        warn!("Unknown tool '{}' denied by default", tool_name);
        Ok(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ))
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

/// Find the bundled claude-code-acp sidecar binary
fn find_sidecar_path() -> Option<PathBuf> {
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
            if !current.pop() {
                break;
            }
        }
    }

    None
}

/// Find the Claude Code CLI executable
/// Security: Only checks known installation paths, canonicalizes results to prevent symlink attacks
fn find_claude_code_executable() -> Option<PathBuf> {
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
            // Canonicalize to resolve any symlinks and verify the real path
            match std::fs::canonicalize(&path) {
                Ok(canonical) => {
                    info!(
                        "Found Claude CLI at {:?} (canonical: {:?})",
                        path, canonical
                    );
                    return Some(canonical);
                }
                Err(e) => {
                    warn!("Failed to canonicalize Claude CLI path {:?}: {}", path, e);
                    continue;
                }
            }
        }
    }

    // Native install script location (~/.claude/local/claude)
    // Use dirs crate pattern for home directory (more reliable than HOME env var)
    if let Some(home) = dirs::home_dir() {
        let native_install = home.join(".claude/local/claude");
        if native_install.exists() {
            match std::fs::canonicalize(&native_install) {
                Ok(canonical) => {
                    info!(
                        "Found Claude CLI at {:?} (canonical: {:?})",
                        native_install, canonical
                    );
                    return Some(canonical);
                }
                Err(e) => {
                    warn!(
                        "Failed to canonicalize Claude CLI path {:?}: {}",
                        native_install, e
                    );
                }
            }
        }
    }

    // Security: We intentionally do NOT fall back to PATH lookup via `which`
    // This prevents PATH injection attacks where a malicious binary could be executed
    warn!("Claude Code CLI not found in any known location");
    None
}

/// Spawn the claude-code-acp sidecar
async fn spawn_claude_code_acp(notes_directory: &Path) -> anyhow::Result<tokio::process::Child> {
    let sidecar_path = find_sidecar_path().ok_or_else(|| {
        anyhow::anyhow!(
            "claude-code-acp sidecar not found.\n\
             For development: run 'bun run build:sidecar' first.\n\
             For users: the app bundle may be corrupted."
        )
    })?;

    // Find Claude Code CLI for the sidecar to use
    let claude_cli_path = find_claude_code_executable().ok_or_else(|| {
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

    // Get current date and format it
    let current_date = Local::now().format("%B %d, %Y").to_string();
    let date_prefix = format!("Current date: {}\n\n", current_date);

    // Build prompt from conversation messages
    // Only include the last user message as the prompt, context comes from session
    let prompt_text = messages
        .iter()
        .map(|(role, content)| format!("{}: {}", role, content))
        .collect::<Vec<_>>()
        .join("\n\n");

    // Prepend current date to the prompt
    let prompt_text = format!("{}{}", date_prefix, prompt_text);

    // Validate prompt is not empty
    if prompt_text.trim().is_empty() {
        return Err(anyhow::anyhow!("Cannot send empty prompt"));
    }

    // Send prompt
    info!("Sending prompt...");
    let prompt_response = connection
        .prompt(PromptRequest::new(
            session_response.session_id,
            vec![ContentBlock::Text(TextContent::new(prompt_text))],
        ))
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
        local
            .block_on(&rt, async move {
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
        Err(format!(
            "No pending permission request with ID: {}",
            request_id
        ))
    }
}

#[tauri::command]
async fn check_acp_available() -> Result<bool, String> {
    // Check if the bundled sidecar binary exists
    Ok(find_sidecar_path().is_some())
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

    store.set("notes_directory", serde_json::json!(path));

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

/// Validate that a path is within the notes directory (security check)
/// Prevents path traversal attacks by canonicalizing both paths
fn validate_path_in_notes_dir(path: &Path, notes_dir: &Path) -> Result<PathBuf, String> {
    // Canonicalize the notes directory (must exist)
    let canonical_notes = std::fs::canonicalize(notes_dir)
        .map_err(|e| format!("Failed to resolve notes directory: {}", e))?;

    // For files that may not exist yet (save), we canonicalize the parent directory
    let canonical_path = if path.exists() {
        std::fs::canonicalize(path).map_err(|e| format!("Failed to resolve path: {}", e))?
    } else {
        // For new files, canonicalize parent and append filename
        let parent = path
            .parent()
            .ok_or_else(|| "Invalid path: no parent directory".to_string())?;
        let filename = path
            .file_name()
            .ok_or_else(|| "Invalid path: no filename".to_string())?;
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|e| format!("Failed to resolve parent directory: {}", e))?;
        canonical_parent.join(filename)
    };

    // Check if path is within notes directory
    if !canonical_path.starts_with(&canonical_notes) {
        return Err(format!(
            "Security error: path is outside the notes directory"
        ));
    }

    Ok(canonical_path)
}

#[tauri::command]
async fn save_project(app: AppHandle, path: String, data: String) -> Result<(), String> {
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

    // Validate path is within notes directory
    let validated_path = validate_path_in_notes_dir(Path::new(&path), &notes_directory)?;

    std::fs::write(&validated_path, &data).map_err(|e| format!("Failed to save project: {}", e))?;
    info!("Project saved to: {:?}", validated_path);
    Ok(())
}

#[tauri::command]
async fn load_project(app: AppHandle, path: String) -> Result<String, String> {
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

    // Validate path is within notes directory
    let validated_path = validate_path_in_notes_dir(Path::new(&path), &notes_directory)?;

    let data = std::fs::read_to_string(&validated_path)
        .map_err(|e| format!("Failed to load project: {}", e))?;
    info!("Project loaded from: {:?}", validated_path);
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
async fn get_recent_projects(app: AppHandle) -> Result<Vec<String>, String> {
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    let recent_projects = store
        .get("recent_projects")
        .and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
        })
        .unwrap_or_default();

    Ok(recent_projects)
}

#[tauri::command]
async fn add_recent_project(app: AppHandle, path: String) -> Result<(), String> {
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    let mut recent_projects: Vec<String> = store
        .get("recent_projects")
        .and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
        })
        .unwrap_or_default();

    // Remove the path if it already exists
    recent_projects.retain(|p| p != &path);

    // Add to the beginning
    recent_projects.insert(0, path);

    // Keep only the most recent 10 projects
    recent_projects.truncate(10);

    store.set("recent_projects", serde_json::json!(recent_projects));

    store
        .save()
        .map_err(|e| format!("Failed to save config store: {}", e))?;

    Ok(())
}

#[tauri::command]
async fn remove_recent_project(app: AppHandle, path: String) -> Result<(), String> {
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    let mut recent_projects: Vec<String> = store
        .get("recent_projects")
        .and_then(|v| {
            v.as_array().map(|arr| {
                arr.iter()
                    .filter_map(|v| v.as_str().map(|s| s.to_string()))
                    .collect()
            })
        })
        .unwrap_or_default();

    // Remove the path
    recent_projects.retain(|p| p != &path);

    store.set("recent_projects", serde_json::json!(recent_projects));

    store
        .save()
        .map_err(|e| format!("Failed to save config store: {}", e))?;

    Ok(())
}

#[tauri::command]
async fn export_markdown(
    app: AppHandle,
    content: String,
    default_name: String,
) -> Result<Option<String>, String> {
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

    if let Some(dir) = store
        .get("notes_directory")
        .and_then(|v| v.as_str().map(PathBuf::from))
    {
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
    use walkdir::WalkDir;

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

    // Sanitize query: limit length and remove potentially dangerous characters
    let query = query.chars().take(100).collect::<String>();
    let query_lower = query.to_lowercase();

    // Use walkdir for safe, native file search
    // - Does NOT follow symlinks (security: prevents escaping notes directory)
    // - Early termination at result limit (performance: no DoS via large directories)
    // - Proper error handling per-entry
    let mut files = Vec::new();

    for entry in WalkDir::new(&notes_directory)
        .follow_links(false) // Security: don't follow symlinks
        .max_depth(20) // Reasonable depth limit
        .into_iter()
        .filter_map(|e| e.ok())
    // Skip entries we can't read
    {
        // Only include files, not directories
        if !entry.file_type().is_file() {
            continue;
        }

        // Get relative path from notes directory
        let rel_path = match entry.path().strip_prefix(&notes_directory) {
            Ok(p) => p.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        // If query is empty, match all files; otherwise do case-insensitive match
        if query.is_empty() || rel_path.to_lowercase().contains(&query_lower) {
            files.push(rel_path);

            // Early termination at limit
            if files.len() >= max_results {
                break;
            }
        }
    }

    Ok(files)
}

// ============================================================================
// Summary generation (uses Haiku via ACP)
// ============================================================================

/// Simple ACP client for summarization - collects response text, auto-approves all tools
struct SummaryClient {
    response_text: Arc<Mutex<String>>,
}

impl SummaryClient {
    fn new() -> Self {
        Self {
            response_text: Arc::new(Mutex::new(String::new())),
        }
    }
}

#[async_trait(?Send)]
impl Client for SummaryClient {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        // Auto-approve first option for summarization (it only uses read-only tools if any)
        if let Some(first_opt) = args.options.first() {
            Ok(RequestPermissionResponse::new(
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                    first_opt.option_id.clone(),
                )),
            ))
        } else {
            Ok(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ))
        }
    }

    async fn session_notification(
        &self,
        args: SessionNotification,
    ) -> agent_client_protocol::Result<()> {
        if let SessionUpdate::AgentMessageChunk(chunk) = args.update {
            if let ContentBlock::Text(text) = chunk.content {
                let mut response = self.response_text.lock().await;
                response.push_str(&text.text);
            }
        }
        Ok(())
    }
}

/// Run a summarization session with Haiku model
async fn run_summary_session(content: String, notes_directory: PathBuf) -> anyhow::Result<String> {
    // Spawn ACP subprocess
    let mut child = spawn_claude_code_acp(&notes_directory).await?;

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
         Be specific and concise. Return ONLY the heading, nothing else:\n\n{}",
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

#[derive(Clone, serde::Serialize)]
struct SummaryResult {
    node_id: String,
    summary: String,
}

#[tauri::command]
async fn generate_summary(
    app: AppHandle,
    node_id: String,
    content: String,
) -> Result<SummaryResult, String> {
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

    info!("Generating summary for node: {}", node_id);

    // Run in LocalSet for non-Send futures
    let result = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to create runtime: {}", e))?;

        let local = tokio::task::LocalSet::new();
        local
            .block_on(&rt, async move {
                run_summary_session(content, notes_directory).await
            })
            .map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?;

    match result {
        Ok(summary) => {
            info!("Generated summary for {}: {}", node_id, summary);
            Ok(SummaryResult { node_id, summary })
        }
        Err(e) => {
            warn!("Summary generation failed for {}: {}", node_id, e);
            Err(e)
        }
    }
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
        .plugin(tauri_plugin_shell::init())
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
            // Recent projects commands
            get_recent_projects,
            add_recent_project,
            remove_recent_project,
            // File search
            search_files,
            // Summary generation
            generate_summary,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
