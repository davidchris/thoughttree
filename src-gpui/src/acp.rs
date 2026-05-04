//! ACP (Agent Client Protocol) bridge between the GPUI side (smol /
//! `cx.spawn`) and the tokio-flavored ACP stack.
//!
//! ## Why a dedicated thread
//!
//! `agent-client-protocol` uses `tokio::process::Command` for the
//! subprocess and exposes `Client` via `async_trait(?Send)` — both
//! require a tokio runtime, and the futures live in a `LocalSet`. GPUI
//! drives its own smol-based executor on the main thread. Rather than
//! make either side run inside the other, this module owns a tokio
//! current-thread runtime on a dedicated OS thread; the two sides
//! communicate only through `async_channel`s, which are runtime-agnostic.
//!
//! ## Channel shapes
//!
//! - **prompts** (GPUI → tokio): caller submits `PromptJob { node_id,
//!   prompt }`; the worker forwards the prompt blocks to the agent.
//! - **events** (tokio → GPUI): worker emits `(NodeId, StreamEvent)`
//!   per chunk and at completion. `StreamEvent` is defined in
//!   `state.rs` next to its consumer.
//!
//! ## What's deliberately minimal
//!
//! - Single ACP session for the lifetime of the client. Branching the
//!   conversation tree should ideally start a new session per branch
//!   (see plan "Session lifecycle" risk).
//! - Permissions are auto-approved (first option). The standalone
//!   harness in `src-tauri/acp/src/main.rs` does the same.
//! - Subprocess is `npx @zed-industries/claude-code-acp`. Production
//!   sidecar discovery (see `src-tauri/src/backend/acp/process.rs`) is
//!   out of scope.
//! - Only `AgentMessageChunk` updates feed the UI. Tool calls,
//!   thoughts, plans are dropped here (logged via tracing).

use crate::graph::NodeId;
use crate::state::StreamEvent;
use agent_client_protocol::{
    Agent, Client, ClientSideConnection, ContentBlock, Implementation, InitializeRequest,
    NewSessionRequest, PromptRequest, ProtocolVersion, RequestPermissionOutcome,
    RequestPermissionRequest, RequestPermissionResponse, SelectedPermissionOutcome, SessionId,
    SessionNotification, SessionUpdate,
};
use anyhow::{Context as _, Result};
use async_trait::async_trait;
use std::collections::HashMap;
use std::process::Stdio;
use std::sync::{Arc, Mutex};
use tokio::process::Command;
use tokio_util::compat::{TokioAsyncReadCompatExt, TokioAsyncWriteCompatExt};

#[derive(Debug)]
pub struct PromptJob {
    pub node_id: NodeId,
    pub prompt: Vec<ContentBlock>,
}

pub struct AcpClient {
    prompts: async_channel::Sender<PromptJob>,
    events: async_channel::Receiver<(NodeId, StreamEvent)>,
}

impl AcpClient {
    /// Boot the tokio worker thread. Returns immediately; the actual
    /// subprocess spawn + ACP handshake happens asynchronously inside
    /// the worker. If the handshake fails later, an `Error` event will
    /// surface on the `events` receiver the next time a prompt is sent.
    pub fn spawn() -> Result<Self> {
        let (prompts_tx, prompts_rx) = async_channel::unbounded::<PromptJob>();
        let (events_tx, events_rx) = async_channel::unbounded::<(NodeId, StreamEvent)>();

        std::thread::Builder::new()
            .name("acp-worker".into())
            .spawn(move || {
                if let Err(err) = run_worker(prompts_rx, events_tx.clone()) {
                    tracing::error!("acp worker exited with error: {err:?}");
                    // Best-effort: surface the error; we no longer know
                    // which node it was for, so this is mostly for logs.
                    let _ = events_tx
                        .try_send(("".to_string(), StreamEvent::Error(format!("{err:?}"))));
                }
            })
            .context("spawn acp worker thread")?;

        Ok(Self {
            prompts: prompts_tx,
            events: events_rx,
        })
    }

    /// Submit a prompt. Fire-and-forget; the matching events arrive on
    /// `events()`.
    pub fn submit(&self, job: PromptJob) {
        // Unbounded channel — try_send only fails if the receiver was
        // dropped, which means the worker thread is gone. Log and
        // ignore; the caller can inspect `events` for follow-up.
        if let Err(err) = self.prompts.try_send(job) {
            tracing::error!("acp prompts channel closed: {err}");
        }
    }

    /// Cloneable receiver — call from a `cx.spawn` drain loop.
    pub fn events(&self) -> async_channel::Receiver<(NodeId, StreamEvent)> {
        self.events.clone()
    }
}

fn run_worker(
    prompts: async_channel::Receiver<PromptJob>,
    events: async_channel::Sender<(NodeId, StreamEvent)>,
) -> Result<()> {
    let rt = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()
        .context("build tokio runtime")?;
    let local = tokio::task::LocalSet::new();
    rt.block_on(local.run_until(worker_loop(prompts, events)))
}

async fn worker_loop(
    prompts: async_channel::Receiver<PromptJob>,
    events: async_channel::Sender<(NodeId, StreamEvent)>,
) -> Result<()> {
    tracing::info!("acp worker: spawning claude-code-acp via npx");
    let mut child = Command::new("npx")
        .args(["@zed-industries/claude-code-acp"])
        // claude-code-acp refuses to start inside another Claude Code
        // session (it shares runtime resources). Unsetting this lets the
        // prototype be `cargo run` from a Claude Code dev shell without
        // tripping the safety check; harmless if the var was unset.
        .env_remove("CLAUDECODE")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true)
        .spawn()
        .context(
            "failed to spawn `npx @zed-industries/claude-code-acp` — \
             ensure Node.js is installed and run `npx @zed-industries/claude-code-acp --help` \
             once before launching to accept the install prompt",
        )?;

    let stdin = child.stdin.take().context("subprocess stdin")?;
    let stdout = child.stdout.take().context("subprocess stdout")?;

    if let Some(stderr) = child.stderr.take() {
        tokio::task::spawn_local(async move {
            use tokio::io::AsyncBufReadExt;
            let reader = tokio::io::BufReader::new(stderr);
            let mut lines = reader.lines();
            while let Ok(Some(line)) = lines.next_line().await {
                tracing::warn!(target: "claude-code-acp.stderr", "{line}");
            }
        });
    }

    let sessions: Arc<Mutex<HashMap<SessionId, NodeId>>> = Arc::new(Mutex::new(HashMap::new()));
    let client = Arc::new(StreamingClient {
        sessions: sessions.clone(),
        events: events.clone(),
    });

    let (connection, io_future) = ClientSideConnection::new(
        client,
        stdin.compat_write(),
        stdout.compat(),
        |f| {
            tokio::task::spawn_local(f);
        },
    );

    tokio::task::spawn_local(async move {
        if let Err(err) = io_future.await {
            tracing::error!("acp I/O loop ended: {err:?}");
        }
    });

    tracing::info!("acp worker: initializing connection");
    let init = connection
        .initialize(
            InitializeRequest::new(ProtocolVersion::LATEST).client_info(
                Implementation::new("thoughttree-gpui", env!("CARGO_PKG_VERSION"))
                    .title("ThoughtTree (GPUI prototype)"),
            ),
        )
        .await
        .context("initialize ACP connection")?;
    tracing::info!(
        "acp worker: connected — protocol={:?}",
        init.protocol_version
    );

    let cwd = std::env::current_dir().context("get cwd for new_session")?;
    let session_response = connection
        .new_session(NewSessionRequest::new(cwd))
        .await
        .context("new_session")?;
    let session_id = session_response.session_id;
    tracing::info!("acp worker: session_id={session_id:?}");

    while let Ok(job) = prompts.recv().await {
        sessions
            .lock()
            .unwrap()
            .insert(session_id.clone(), job.node_id.clone());

        let result = connection
            .prompt(PromptRequest::new(session_id.clone(), job.prompt))
            .await;

        sessions.lock().unwrap().remove(&session_id);

        let event = match result {
            Ok(response) => {
                tracing::info!("acp worker: prompt complete, stop={:?}", response.stop_reason);
                StreamEvent::Complete
            }
            Err(err) => {
                tracing::error!("acp worker: prompt error: {err:?}");
                StreamEvent::Error(format!("{err:?}"))
            }
        };
        let _ = events.send((job.node_id, event)).await;
    }

    tracing::info!("acp worker: prompts channel closed, shutting down");
    drop(connection);
    let _ = child.wait().await;
    Ok(())
}

/// `Client` implementation that forwards `AgentMessageChunk` text into
/// the GPUI side's `events` channel and auto-approves permissions.
struct StreamingClient {
    sessions: Arc<Mutex<HashMap<SessionId, NodeId>>>,
    events: async_channel::Sender<(NodeId, StreamEvent)>,
}

#[async_trait(?Send)]
impl Client for StreamingClient {
    async fn request_permission(
        &self,
        args: RequestPermissionRequest,
    ) -> agent_client_protocol::Result<RequestPermissionResponse> {
        let outcome = if let Some(first) = args.options.first() {
            RequestPermissionOutcome::Selected(SelectedPermissionOutcome::new(
                first.option_id.clone(),
            ))
        } else {
            RequestPermissionOutcome::Cancelled
        };
        Ok(RequestPermissionResponse::new(outcome))
    }

    async fn session_notification(
        &self,
        args: SessionNotification,
    ) -> agent_client_protocol::Result<()> {
        match args.update {
            SessionUpdate::AgentMessageChunk(chunk) => {
                if let ContentBlock::Text(text) = chunk.content {
                    let node_id = self.sessions.lock().unwrap().get(&args.session_id).cloned();
                    if let Some(id) = node_id {
                        let _ = self.events.send((id, StreamEvent::Chunk(text.text))).await;
                    }
                }
            }
            other => {
                tracing::debug!("acp: dropping non-text update: {other:?}");
            }
        }
        Ok(())
    }
}
