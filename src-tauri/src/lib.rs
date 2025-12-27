use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;

use serde::{Deserialize, Serialize};

// ============================================================================
// Agent Provider Types
// ============================================================================

/// Supported agent providers for ACP connections
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AgentProvider {
    #[default]
    ClaudeCode,
    GeminiCli,
}

impl AgentProvider {
    /// Human-readable display name for UI
    pub fn display_name(&self) -> &'static str {
        match self {
            AgentProvider::ClaudeCode => "Claude Code",
            AgentProvider::GeminiCli => "Gemini CLI",
        }
    }

    /// Short name for badges/labels
    pub fn short_name(&self) -> &'static str {
        match self {
            AgentProvider::ClaudeCode => "Claude",
            AgentProvider::GeminiCli => "Gemini",
        }
    }
}

/// Provider availability status for frontend
#[derive(Clone, Debug, Serialize)]
pub struct ProviderStatus {
    pub provider: AgentProvider,
    pub available: bool,
    pub error_message: Option<String>,
}

/// Model info discovered from ACP CreateSessionResponse.models.available_models
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_id: String,
    pub display_name: String,
}

/// User's preferred model per provider (stores model_id strings)
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ModelPreferences {
    #[serde(default, rename = "claude-code")]
    pub claude_code: Option<String>,
    #[serde(default, rename = "gemini-cli")]
    pub gemini_cli: Option<String>,
}

impl ModelPreferences {
    /// Get the model preference for a given provider
    pub fn get(&self, provider: &AgentProvider) -> Option<&String> {
        match provider {
            AgentProvider::ClaudeCode => self.claude_code.as_ref(),
            AgentProvider::GeminiCli => self.gemini_cli.as_ref(),
        }
    }

    /// Set the model preference for a given provider
    pub fn set(&mut self, provider: &AgentProvider, model_id: Option<String>) {
        match provider {
            AgentProvider::ClaudeCode => self.claude_code = model_id,
            AgentProvider::GeminiCli => self.gemini_cli = model_id,
        }
    }
}

/// Custom executable paths for providers (user-configured overrides)
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub struct ProviderPaths {
    #[serde(default, rename = "claude-code")]
    pub claude_code: Option<String>,
    #[serde(default, rename = "gemini-cli")]
    pub gemini_cli: Option<String>,
}

impl ProviderPaths {
    /// Get the custom path for a given provider
    pub fn get(&self, provider: &AgentProvider) -> Option<&String> {
        match provider {
            AgentProvider::ClaudeCode => self.claude_code.as_ref(),
            AgentProvider::GeminiCli => self.gemini_cli.as_ref(),
        }
    }

    /// Set the custom path for a given provider
    pub fn set(&mut self, provider: &AgentProvider, path: Option<String>) {
        match provider {
            AgentProvider::ClaudeCode => self.claude_code = path,
            AgentProvider::GeminiCli => self.gemini_cli = path,
        }
    }
}

use agent_client_protocol::{
    Agent, Client, ClientSideConnection, ContentBlock, ImageContent, Implementation,
    InitializeRequest, NewSessionRequest, PromptRequest, ProtocolVersion,
    RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate, SetSessionModelRequest,
    TextContent,
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

// Message types from frontend (with optional images)
#[derive(Clone, serde::Deserialize)]
struct MessageImage {
    data: String,
    mime_type: String,
}

#[derive(Clone, serde::Deserialize)]
struct Message {
    role: String,
    content: String,
    images: Option<Vec<MessageImage>>,
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
fn find_claude_code_executable(custom_path: Option<&str>) -> Option<PathBuf> {
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

/// Spawn the claude-code-acp sidecar
async fn spawn_claude_code_acp(
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

/// Find the Gemini CLI executable
/// Security: Only checks known installation paths
/// If custom_path is provided, it's checked first
fn find_gemini_cli_executable(custom_path: Option<&str>) -> Option<PathBuf> {
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
                    info!("Found Gemini CLI at {:?} (resolves to: {:?})", path, canonical);
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

/// Spawn Gemini CLI in ACP mode
async fn spawn_gemini_cli_acp(
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
async fn spawn_agent_subprocess(
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

/// Run a prompt session with ACP
async fn run_prompt_session(
    app_handle: AppHandle,
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
    let mut child =
        spawn_agent_subprocess(&provider, &notes_directory, &provider_paths, model_id.as_deref())
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
        content_blocks.iter().filter(|b| matches!(b, ContentBlock::Image(_))).count()
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

#[tauri::command]
async fn send_prompt(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    node_id: String,
    messages: Vec<Message>,
    provider: Option<AgentProvider>,
    model_id: Option<String>,
) -> Result<String, String> {
    let pending_permissions = state.pending_permissions.clone();

    // Load notes directory, default provider, and provider paths from config store
    let (notes_directory, default_provider, provider_paths) = {
        let store = app_handle
            .store("config.json")
            .map_err(|e| format!("Failed to open config store: {}", e))?;

        let notes_dir = store
            .get("notes_directory")
            .and_then(|v| v.as_str().map(PathBuf::from))
            .ok_or_else(|| {
                "Notes directory not configured. Please set it in settings.".to_string()
            })?;

        let default_prov = store
            .get("default_provider")
            .and_then(|v| serde_json::from_value::<AgentProvider>(v.clone()).ok())
            .unwrap_or_default();

        let paths: ProviderPaths = store
            .get("provider_paths")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        (notes_dir, default_prov, paths)
    };

    // Use provided provider or fall back to default
    let active_provider = provider.unwrap_or(default_provider);

    info!(
        "Using provider: {:?}, notes directory: {:?}",
        active_provider, notes_directory
    );

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
                    active_provider,
                    model_id,
                    provider_paths,
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
// Provider management commands
// ============================================================================

/// Check if a specific provider is available on this system
fn check_provider_availability(provider: &AgentProvider, paths: &ProviderPaths) -> ProviderStatus {
    match provider {
        AgentProvider::ClaudeCode => {
            let sidecar_available = find_sidecar_path().is_some();
            let custom_path = paths.claude_code.as_deref();
            let cli_available = find_claude_code_executable(custom_path).is_some();

            ProviderStatus {
                provider: provider.clone(),
                available: sidecar_available && cli_available,
                error_message: if !sidecar_available {
                    Some(
                        "claude-code-acp sidecar not found (dev: run bun run build:sidecar)"
                            .into(),
                    )
                } else if !cli_available {
                    Some(
                        "Claude Code CLI not found. Install via: brew install --cask claude-code"
                            .into(),
                    )
                } else {
                    None
                },
            }
        }
        AgentProvider::GeminiCli => {
            let custom_path = paths.gemini_cli.as_deref();
            let cli_available = find_gemini_cli_executable(custom_path).is_some();

            ProviderStatus {
                provider: provider.clone(),
                available: cli_available,
                error_message: if !cli_available {
                    Some("Gemini CLI not found. Install via: brew install gemini-cli".into())
                } else {
                    None
                },
            }
        }
    }
}

#[tauri::command]
async fn get_available_providers(app: AppHandle) -> Result<Vec<ProviderStatus>, String> {
    // Load custom paths from config store
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    let paths: ProviderPaths = store
        .get("provider_paths")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    Ok(vec![
        check_provider_availability(&AgentProvider::ClaudeCode, &paths),
        check_provider_availability(&AgentProvider::GeminiCli, &paths),
    ])
}

#[tauri::command]
async fn get_default_provider(app: AppHandle) -> Result<AgentProvider, String> {
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    Ok(store
        .get("default_provider")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default())
}

#[tauri::command]
async fn set_default_provider(app: AppHandle, provider: AgentProvider) -> Result<(), String> {
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    store.set(
        "default_provider",
        serde_json::to_value(&provider).unwrap(),
    );

    store
        .save()
        .map_err(|e| format!("Failed to save config: {}", e))?;

    info!("Default provider set to: {:?}", provider);
    Ok(())
}

#[tauri::command]
async fn get_model_preferences(app: AppHandle) -> Result<ModelPreferences, String> {
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    Ok(store
        .get("model_preferences")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default())
}

#[tauri::command]
async fn set_model_preference(
    app: AppHandle,
    provider: AgentProvider,
    model_id: Option<String>,
) -> Result<(), String> {
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    // Load existing preferences
    let mut preferences: ModelPreferences = store
        .get("model_preferences")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    // Update the preference for this provider
    preferences.set(&provider, model_id.clone());

    store.set(
        "model_preferences",
        serde_json::to_value(&preferences).unwrap(),
    );

    store
        .save()
        .map_err(|e| format!("Failed to save config: {}", e))?;

    info!(
        "Model preference for {:?} set to: {:?}",
        provider, model_id
    );
    Ok(())
}

// ============================================================================
// Provider path configuration commands
// ============================================================================

/// Validate an executable path by running --version and checking output
async fn validate_executable(path: &Path, provider: &AgentProvider) -> Result<String, String> {
    // Check file exists
    if !path.exists() {
        return Err("File does not exist".to_string());
    }

    // Check it's a file (not a directory)
    if !path.is_file() {
        return Err("Path is not a file".to_string());
    }

    // Run --version to validate it's the correct tool
    let output = Command::new(path)
        .arg("--version")
        .output()
        .await
        .map_err(|e| format!("Failed to execute: {}", e))?;

    let stdout = String::from_utf8_lossy(&output.stdout);
    let stderr = String::from_utf8_lossy(&output.stderr);
    let combined = format!("{}{}", stdout, stderr);

    // Check the output contains expected identifier
    let expected_pattern = match provider {
        AgentProvider::ClaudeCode => "claude",
        AgentProvider::GeminiCli => "gemini",
    };

    if combined.to_lowercase().contains(expected_pattern) {
        // Extract version info from first line
        let version_line = stdout
            .lines()
            .next()
            .or_else(|| stderr.lines().next())
            .unwrap_or("Unknown version")
            .trim();
        Ok(version_line.to_string())
    } else {
        Err(format!(
            "Not a valid {} executable (output: {})",
            provider.display_name(),
            combined.chars().take(100).collect::<String>()
        ))
    }
}

#[tauri::command]
async fn get_provider_paths(app: AppHandle) -> Result<ProviderPaths, String> {
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    Ok(store
        .get("provider_paths")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default())
}

#[tauri::command]
async fn set_provider_path(
    app: AppHandle,
    provider: AgentProvider,
    path: Option<String>,
) -> Result<(), String> {
    // If path is provided, validate it first
    if let Some(ref p) = path {
        let path_buf = PathBuf::from(p);
        validate_executable(&path_buf, &provider).await?;
    }

    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    // Load existing paths
    let mut paths: ProviderPaths = store
        .get("provider_paths")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    // Update the path for this provider
    paths.set(&provider, path.clone());

    store.set("provider_paths", serde_json::to_value(&paths).unwrap());

    store
        .save()
        .map_err(|e| format!("Failed to save config: {}", e))?;

    info!("Provider path for {:?} set to: {:?}", provider, path);
    Ok(())
}

#[tauri::command]
async fn validate_provider_path(provider: AgentProvider, path: String) -> Result<String, String> {
    let path_buf = PathBuf::from(&path);
    validate_executable(&path_buf, &provider).await
}

#[tauri::command]
async fn pick_provider_executable(app: AppHandle, provider: AgentProvider) -> Result<Option<String>, String> {
    let title = format!("Select {} Executable", provider.display_name());

    let path = app
        .dialog()
        .file()
        .set_title(&title)
        .blocking_pick_file();

    Ok(path.map(|p| p.to_string()))
}

/// Minimal ACP client just for model discovery - no streaming or permissions needed
struct ModelDiscoveryClient;

#[async_trait(?Send)]
impl Client for ModelDiscoveryClient {
    async fn request_permission(
        &self,
        _args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        // Should never be called during model discovery
        Ok(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ))
    }

    async fn session_notification(
        &self,
        _args: SessionNotification,
    ) -> agent_client_protocol::Result<()> {
        // No-op for discovery
        Ok(())
    }
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

#[tauri::command]
async fn get_available_models(
    app: AppHandle,
    provider: AgentProvider,
) -> Result<Vec<ModelInfo>, String> {
    // Get notes directory and provider paths for subprocess
    let store = app
        .store("config.json")
        .map_err(|e| format!("Failed to open config store: {}", e))?;

    let notes_dir = store
        .get("notes_directory")
        .and_then(|v| v.as_str().map(String::from))
        .ok_or_else(|| "Notes directory not configured".to_string())?;

    let provider_paths: ProviderPaths = store
        .get("provider_paths")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or_default();

    let notes_directory = PathBuf::from(&notes_dir);

    // Run in spawn_blocking with LocalSet for non-Send futures (same pattern as send_prompt)
    let result = tokio::task::spawn_blocking(move || {
        let rt = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .map_err(|e| format!("Failed to create runtime: {}", e))?;

        let local = tokio::task::LocalSet::new();
        local
            .block_on(&rt, async move {
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
                        Implementation::new("thoughttree", env!("CARGO_PKG_VERSION"))
                            .title("ThoughtTree"),
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
                Ok::<Vec<ModelInfo>, String>(models)
            })
    })
    .await
    .map_err(|e| format!("Task join error: {}", e))?;

    result
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
async fn run_summary_session(
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
    // Get notes directory and custom Claude path from config
    let (notes_directory, custom_path) = {
        let store = app
            .store("config.json")
            .map_err(|e| format!("Failed to open config store: {}", e))?;

        let notes_dir = store
            .get("notes_directory")
            .and_then(|v| v.as_str().map(PathBuf::from))
            .ok_or_else(|| "Notes directory not configured".to_string())?;

        let paths: ProviderPaths = store
            .get("provider_paths")
            .and_then(|v| serde_json::from_value(v.clone()).ok())
            .unwrap_or_default();

        (notes_dir, paths.claude_code)
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
                run_summary_session(content, notes_directory, custom_path).await
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
            // Provider commands
            get_available_providers,
            get_default_provider,
            set_default_provider,
            // Model commands
            get_model_preferences,
            set_model_preference,
            get_available_models,
            // Provider path commands
            get_provider_paths,
            set_provider_path,
            validate_provider_path,
            pick_provider_executable,
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

// ============================================================================
// Tests
// ============================================================================

#[cfg(test)]
mod tests {
    use super::*;

    mod provider_tests {
        use super::*;

        #[test]
        fn test_provider_default_is_claude_code() {
            let provider = AgentProvider::default();
            assert_eq!(provider, AgentProvider::ClaudeCode);
        }

        #[test]
        fn test_provider_serializes_to_kebab_case() {
            let claude = AgentProvider::ClaudeCode;
            let gemini = AgentProvider::GeminiCli;

            let claude_json = serde_json::to_string(&claude).unwrap();
            let gemini_json = serde_json::to_string(&gemini).unwrap();

            assert_eq!(claude_json, "\"claude-code\"");
            assert_eq!(gemini_json, "\"gemini-cli\"");
        }

        #[test]
        fn test_provider_deserializes_from_kebab_case() {
            let claude: AgentProvider = serde_json::from_str("\"claude-code\"").unwrap();
            let gemini: AgentProvider = serde_json::from_str("\"gemini-cli\"").unwrap();

            assert_eq!(claude, AgentProvider::ClaudeCode);
            assert_eq!(gemini, AgentProvider::GeminiCli);
        }

        #[test]
        fn test_provider_display_names() {
            assert_eq!(AgentProvider::ClaudeCode.display_name(), "Claude Code");
            assert_eq!(AgentProvider::GeminiCli.display_name(), "Gemini CLI");
        }

        #[test]
        fn test_provider_short_names() {
            assert_eq!(AgentProvider::ClaudeCode.short_name(), "Claude");
            assert_eq!(AgentProvider::GeminiCli.short_name(), "Gemini");
        }
    }
}
