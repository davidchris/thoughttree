use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use agent_client_protocol::schema::v1::{
    ContentBlock, RequestPermissionOutcome, RequestPermissionRequest, RequestPermissionResponse,
    SelectedPermissionOutcome, SessionNotification, SessionUpdate,
};
use async_trait::async_trait;
use futures::lock::Mutex;
use tracing::{debug, info, warn};

use crate::events::{
    PermissionRequestEvent, PermissionRequestOption, SessionEventSink, StreamChunkEvent,
};
use crate::permissions::PermissionBroker;

/// Markdown inserted between assistant message segments that were split by
/// tool calls or thinking, so intermediary commentary and the final answer
/// don't run together as one paragraph.
pub const SEGMENT_SEPARATOR: &str = "\n\n---\n\n";

/// Agent-to-client traffic for one ACP session. `sessions::connect_agent`
/// registers an implementation as the connection's notification and
/// permission-request handlers, so the futures must be `Send`.
#[async_trait]
pub trait SessionClient: Send + Sync + 'static {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse>;

    async fn session_notification(
        &self,
        args: SessionNotification,
    ) -> agent_client_protocol::Result<()>;
}

/// ACP Client that streams to frontend and handles permissions via UI
pub struct StreamingClient<S> {
    sink: S,
    node_id: String,
    turn_id: String,
    broker: PermissionBroker,
    notes_directory: PathBuf,
    /// Some message text has been streamed for this turn.
    has_message_text: AtomicBool,
    /// A non-message update (tool call, thought, plan) arrived after message
    /// text, so the next message chunk starts a new segment.
    segment_boundary_pending: AtomicBool,
}

impl<S: SessionEventSink> StreamingClient<S> {
    pub fn new(
        sink: S,
        node_id: String,
        turn_id: String,
        broker: PermissionBroker,
        notes_directory: PathBuf,
    ) -> Self {
        Self {
            sink,
            node_id,
            turn_id,
            broker,
            notes_directory,
            has_message_text: AtomicBool::new(false),
            segment_boundary_pending: AtomicBool::new(false),
        }
    }

    /// Record that the agent did something other than emit message text.
    /// Only matters once some text exists; a boundary before the first
    /// segment would render as a leading rule.
    fn note_segment_boundary(&self) {
        if self.has_message_text.load(Ordering::Relaxed) {
            self.segment_boundary_pending.store(true, Ordering::Relaxed);
        }
    }

    /// Prefix `text` with a separator when it opens a new message segment.
    fn with_segment_separator(&self, text: String) -> String {
        if text.is_empty() {
            return text;
        }
        let needs_separator = self.segment_boundary_pending.swap(false, Ordering::Relaxed);
        self.has_message_text.store(true, Ordering::Relaxed);
        if needs_separator {
            format!("{SEGMENT_SEPARATOR}{text}")
        } else {
            text
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

#[async_trait]
impl<S: SessionEventSink> SessionClient for StreamingClient<S> {
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
                        turn_id: self.turn_id.clone(),
                        chunk: self.with_segment_separator(text.text),
                    });
                }
            }
            SessionUpdate::AgentThoughtChunk(chunk) => {
                self.note_segment_boundary();
                if let ContentBlock::Text(text) = chunk.content {
                    debug!("[Thought] {}", text.text);
                }
            }
            SessionUpdate::ToolCall(tc) => {
                self.note_segment_boundary();
                info!("[Tool Call] {:?}", tc);
            }
            SessionUpdate::ToolCallUpdate(update) => {
                self.note_segment_boundary();
                debug!("[Tool Update] {:?}", update);
            }
            SessionUpdate::Plan(plan) => {
                self.note_segment_boundary();
                debug!("[Plan] {:?}", plan);
            }
            // Bookkeeping, not agent activity: must not split message segments.
            SessionUpdate::UsageUpdate(usage) => {
                info!("[Usage] {}/{} tokens in context", usage.used, usage.size);
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

#[async_trait]
impl SessionClient for ModelDiscoveryClient {
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

#[async_trait]
impl SessionClient for SummaryClient {
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

    use agent_client_protocol::schema::v1::{
        ContentBlock, ContentChunk, PermissionOption, PermissionOptionKind,
        RequestPermissionOutcome, RequestPermissionRequest, SessionNotification, SessionUpdate,
        TextContent, ToolCall, ToolCallId, ToolCallUpdate, ToolCallUpdateFields, UsageUpdate,
    };

    use super::{is_allowed_summary_tool, SessionClient, StreamingClient, SEGMENT_SEPARATOR};
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

        /// What the frontend renders: chunks concatenated in order.
        fn content(&self) -> String {
            self.stream_chunks()
                .into_iter()
                .map(|event| event.chunk)
                .collect()
        }
    }

    fn streaming_client(sink: RecordingSink) -> StreamingClient<RecordingSink> {
        StreamingClient::new(
            sink,
            "node-42".to_string(),
            "turn-42".to_string(),
            PermissionBroker::new(),
            PathBuf::from("/tmp"),
        )
    }

    fn message_chunk(text: &str) -> SessionNotification {
        SessionNotification::new(
            "session-1",
            SessionUpdate::AgentMessageChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(text),
            ))),
        )
    }

    fn thought_chunk(text: &str) -> SessionNotification {
        SessionNotification::new(
            "session-1",
            SessionUpdate::AgentThoughtChunk(ContentChunk::new(ContentBlock::Text(
                TextContent::new(text),
            ))),
        )
    }

    fn tool_call() -> SessionNotification {
        SessionNotification::new(
            "session-1",
            SessionUpdate::ToolCall(ToolCall::new(ToolCallId::new("tc-1"), "Read file")),
        )
    }

    fn usage_update() -> SessionNotification {
        SessionNotification::new(
            "session-1",
            SessionUpdate::UsageUpdate(UsageUpdate::new(10, 100)),
        )
    }

    async fn notify_all(
        client: &StreamingClient<RecordingSink>,
        updates: Vec<SessionNotification>,
    ) {
        for update in updates {
            client.session_notification(update).await.unwrap();
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
                "turn-42".to_string(),
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
                    turn_id: "turn-42".to_string(),
                    chunk: "hello world".to_string(),
                }]
            );
        });
    }

    #[test]
    fn streaming_client_separates_message_segments_split_by_tool_calls() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let sink = RecordingSink::default();
            let client = streaming_client(sink.clone());

            // Shape observed with Codex: commentary, tool, commentary, tool, answer.
            notify_all(
                &client,
                vec![
                    message_chunk("I'm retrieving the note."),
                    tool_call(),
                    message_chunk("IMP has a physics component."),
                    tool_call(),
                    message_chunk("**HU IMP** is worth it."),
                ],
            )
            .await;

            assert_eq!(
                sink.content(),
                format!(
                    "I'm retrieving the note.{SEGMENT_SEPARATOR}IMP has a physics component.\
                     {SEGMENT_SEPARATOR}**HU IMP** is worth it."
                )
            );
        });
    }

    #[test]
    fn streaming_client_separates_message_segments_split_by_thoughts() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let sink = RecordingSink::default();
            let client = streaming_client(sink.clone());

            notify_all(
                &client,
                vec![
                    message_chunk("Looking."),
                    thought_chunk("hmm"),
                    message_chunk("Found it."),
                ],
            )
            .await;

            assert_eq!(
                sink.content(),
                format!("Looking.{SEGMENT_SEPARATOR}Found it.")
            );
        });
    }

    #[test]
    fn streaming_client_keeps_contiguous_chunks_and_leading_activity_untouched() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let sink = RecordingSink::default();
            let client = streaming_client(sink.clone());

            // Thinking and tools before any text must not produce a leading rule;
            // token-level streaming chunks of one message must not be split.
            notify_all(
                &client,
                vec![
                    thought_chunk("plan"),
                    tool_call(),
                    message_chunk("Hel"),
                    message_chunk("lo "),
                    message_chunk("world"),
                ],
            )
            .await;

            assert_eq!(sink.content(), "Hello world");
        });
    }

    #[test]
    fn usage_update_notification_decodes_from_codex_wire_shape() {
        // codex-acp 1.11 sends this every turn; the 0.9 crate rejected it as
        // an unknown variant and logged a decode error.
        let notification: SessionNotification = serde_json::from_value(serde_json::json!({
            "sessionId": "session-1",
            "update": {"sessionUpdate": "usage_update", "used": 1234, "size": 200000}
        }))
        .unwrap();

        match notification.update {
            SessionUpdate::UsageUpdate(usage) => {
                assert_eq!((usage.used, usage.size), (1234, 200000));
            }
            other => panic!("expected usage update, got {other:?}"),
        }
    }

    #[test]
    fn streaming_client_ignores_usage_updates_for_segmenting() {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()
            .unwrap();

        runtime.block_on(async {
            let sink = RecordingSink::default();
            let client = streaming_client(sink.clone());

            notify_all(
                &client,
                vec![message_chunk("Hel"), usage_update(), message_chunk("lo")],
            )
            .await;

            assert_eq!(sink.content(), "Hello");
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
                "turn-42".to_string(),
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
