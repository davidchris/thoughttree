use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::{
    Client, ContentBlock, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionNotification, SessionUpdate,
};
use async_trait::async_trait;
use futures::lock::Mutex;
use tauri::{AppHandle, Emitter};
use tokio::sync::oneshot;
use tracing::{debug, error, info, warn};

use crate::backend::types::{ChunkPayload, PermissionOption, PermissionPayload};

/// ACP Client that streams to frontend and handles permissions via UI
pub(crate) struct StreamingClient {
    app_handle: AppHandle,
    node_id: String,
    pending_permissions: Arc<Mutex<HashMap<String, oneshot::Sender<String>>>>,
    notes_directory: PathBuf,
}

impl StreamingClient {
    pub(crate) fn new(
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

/// Minimal ACP client just for model discovery - no streaming or permissions needed
pub(crate) struct ModelDiscoveryClient;

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

/// Simple ACP client for summarization - collects response text, auto-approves all tools
pub(crate) struct SummaryClient {
    pub response_text: Arc<Mutex<String>>,
}

impl SummaryClient {
    pub fn new() -> Self {
        Self {
            response_text: Arc::new(Mutex::new(String::new())),
        }
    }
}

/// Summary generation is background work, so keep tool access extremely strict.
/// Deny-by-default and only allow explicit read-only discovery tools.
pub(crate) fn is_allowed_summary_tool(tool_name: &str) -> bool {
    const ALLOWED_PATTERNS: [&str; 3] = ["Read", "Grep", "Glob"];
    ALLOWED_PATTERNS
        .iter()
        .any(|pattern| tool_name.contains(pattern))
}

#[async_trait(?Send)]
impl Client for SummaryClient {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        let tool_name = args.tool_call.fields.title.as_deref().unwrap_or("Unknown");
        if !is_allowed_summary_tool(tool_name) {
            warn!("[summary] denying tool request: {}", tool_name);
            return Ok(RequestPermissionResponse::new(
                RequestPermissionOutcome::Cancelled,
            ));
        }

        // For explicitly allowed read-only tools, select the first option (typically Allow).
        if let Some(first_opt) = args.options.first() {
            return Ok(RequestPermissionResponse::new(
                RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                    first_opt.option_id.clone(),
                )),
            ));
        }

        Ok(RequestPermissionResponse::new(
            RequestPermissionOutcome::Cancelled,
        ))
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

#[cfg(test)]
mod tests {
    use super::is_allowed_summary_tool;

    #[test]
    fn test_summary_tool_allowlist_only_allows_read_tools() {
        assert!(is_allowed_summary_tool("Read"));
        assert!(is_allowed_summary_tool("Grep"));
        assert!(is_allowed_summary_tool("Glob"));
        assert!(!is_allowed_summary_tool("Bash"));
        assert!(!is_allowed_summary_tool("Write"));
        assert!(!is_allowed_summary_tool("WebFetch"));
    }
}
