use std::path::PathBuf;
use std::sync::Arc;

use agent_client_protocol::{
    Client, ContentBlock, RequestPermissionOutcome, RequestPermissionRequest,
    RequestPermissionResponse, SelectedPermissionOutcome, SessionNotification, SessionUpdate,
};
use async_trait::async_trait;
use futures::lock::Mutex;
use tracing::{debug, info, warn};

use crate::events::{
    PermissionRequestEvent, PermissionRequestOption, SessionEventSink, StreamChunkEvent,
};
use crate::permissions::PermissionBroker;

/// ACP Client that streams to frontend and handles permissions via UI
pub struct StreamingClient<S> {
    sink: S,
    node_id: String,
    broker: PermissionBroker,
    notes_directory: PathBuf,
}

impl<S: SessionEventSink> StreamingClient<S> {
    pub fn new(
        sink: S,
        node_id: String,
        broker: PermissionBroker,
        notes_directory: PathBuf,
    ) -> Self {
        Self {
            sink,
            node_id,
            broker,
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
        let options: Vec<PermissionRequestOption> = args
            .options
            .iter()
            .map(|opt| PermissionRequestOption {
                id: opt.option_id.0.to_string(),
                label: opt.name.clone(),
            })
            .collect();

        let event = PermissionRequestEvent::new(
            request_id.clone(),
            self.node_id.clone(),
            tool_type,
            tool_name,
            description,
            options,
        );

        match self.broker.request(event, &self.sink).await {
            Ok(option_id_str) => {
                info!("Permission response received: {}", option_id_str);
                Ok(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                        option_id_str,
                    )),
                ))
            }
            Err(err) => {
                warn!("Permission request cancelled: {err}");
                Ok(RequestPermissionResponse::new(
                    RequestPermissionOutcome::Cancelled,
                ))
            }
        }
    }
}

#[async_trait(?Send)]
impl<S: SessionEventSink> Client for StreamingClient<S> {
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
                    self.sink.stream_chunk(StreamChunkEvent {
                        node_id: self.node_id.clone(),
                        chunk: text.text,
                    });
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
pub struct ModelDiscoveryClient;

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
pub struct SummaryClient {
    pub response_text: Arc<Mutex<String>>,
}

impl SummaryClient {
    pub fn new() -> Self {
        Self {
            response_text: Arc::new(Mutex::new(String::new())),
        }
    }
}

impl Default for SummaryClient {
    fn default() -> Self {
        Self::new()
    }
}

/// Summary generation is background work, so keep tool access extremely strict.
/// Deny-by-default and only allow explicit read-only discovery tools.
pub fn is_allowed_summary_tool(tool_name: &str) -> bool {
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
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex as StdMutex};

    use agent_client_protocol::{
        Client, ContentBlock, ContentChunk, PermissionOption, PermissionOptionKind,
        RequestPermissionOutcome, RequestPermissionRequest, SessionNotification, SessionUpdate,
        TextContent, ToolCallUpdate, ToolCallUpdateFields,
    };

    use super::{is_allowed_summary_tool, StreamingClient};
    use crate::events::{PermissionRequestEvent, SessionEventSink, StreamChunkEvent};
    use crate::permissions::PermissionBroker;

    #[derive(Clone, Default)]
    struct RecordingSink {
        stream_chunks: Arc<StdMutex<Vec<StreamChunkEvent>>>,
        permission_requests: Arc<StdMutex<Vec<PermissionRequestEvent>>>,
    }

    impl RecordingSink {
        fn stream_chunks(&self) -> Vec<StreamChunkEvent> {
            self.stream_chunks.lock().unwrap().clone()
        }
    }

    impl SessionEventSink for RecordingSink {
        fn stream_chunk(&self, event: StreamChunkEvent) {
            self.stream_chunks.lock().unwrap().push(event);
        }

        fn permission_request(&self, event: PermissionRequestEvent) {
            self.permission_requests.lock().unwrap().push(event);
        }
    }

    fn permission_request() -> RequestPermissionRequest {
        let tool_call =
            ToolCallUpdate::new("tool-call-1", ToolCallUpdateFields::new().title("WebFetch"));

        RequestPermissionRequest::new(
            "session-1",
            tool_call,
            vec![PermissionOption::new(
                "allow",
                "Allow",
                PermissionOptionKind::AllowOnce,
            )],
        )
    }

    #[test]
    fn test_summary_tool_allowlist_only_allows_read_tools() {
        assert!(is_allowed_summary_tool("Read"));
        assert!(is_allowed_summary_tool("Grep"));
        assert!(is_allowed_summary_tool("Glob"));
        assert!(!is_allowed_summary_tool("Bash"));
        assert!(!is_allowed_summary_tool("Write"));
        assert!(!is_allowed_summary_tool("WebFetch"));
    }

    #[test]
    fn streaming_client_forwards_message_chunks_to_sink() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let sink = RecordingSink::default();
            let client = StreamingClient::new(
                sink.clone(),
                "node-42".to_string(),
                PermissionBroker::new(),
                PathBuf::from("/tmp"),
            );

            client
                .session_notification(SessionNotification::new(
                    "session-1",
                    SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                        TextContent::new("hello world"),
                    ))),
                ))
                .await
                .unwrap();

            assert_eq!(
                sink.stream_chunks(),
                vec![StreamChunkEvent {
                    node_id: "node-42".to_string(),
                    chunk: "hello world".to_string(),
                }]
            );
        });
    }

    #[test]
    fn streaming_client_permission_request_uses_broker_response() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();
        let local = tokio::task::LocalSet::new();

        local.block_on(&runtime, async {
            let broker = PermissionBroker::new();
            let sink = RecordingSink::default();
            let client = Arc::new(StreamingClient::new(
                sink,
                "node-42".to_string(),
                broker.clone(),
                PathBuf::from("/tmp"),
            ));

            let request = permission_request();
            let request_task = {
                let client = client.clone();
                tokio::task::spawn_local(async move { client.request_permission(request).await })
            };

            tokio::task::yield_now().await;

            let pending = broker.pending().await;
            assert_eq!(pending.len(), 1);

            broker
                .respond(&pending[0].request_id, "allow".to_string())
                .await
                .unwrap();

            let response = request_task.await.unwrap().unwrap();
            match response.outcome {
                RequestPermissionOutcome::Selected(selected) => {
                    assert_eq!(selected.option_id.0.as_ref(), "allow");
                }
                other => panic!("expected selected permission outcome, got {other:?}"),
            }
        });
    }
}
