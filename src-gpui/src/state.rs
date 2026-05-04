//! Shared application state (Zustand-equivalent on the GPUI side).
//!
//! Held inside an `Entity<AppState>` and observed by every view that renders
//! graph or selection state. Mutations go through the methods on this type so
//! the surface mirrors the action set in `src/store/useGraphStore.ts`.

use crate::acp::{AcpClient, PromptJob};
use crate::graph::{
    model::{ConversationMessage, GraphModel},
    types::{GraphNode, NodeRole, Position},
    Graph, GraphMutations, NodeId,
};
use agent_client_protocol::ContentBlock;
use chrono::Utc;
use gpui::Context;
use std::collections::HashSet;
use uuid::Uuid;

/// Flatten a conversation path into the prompt blocks ACP expects.
///
/// v1: send only the *last user message* as a single text block. The agent
/// keeps its own session history, so anything earlier is implicit. This
/// is wrong when the user branches mid-conversation (the agent would have
/// no memory of the off-branch turns) — see plan "Session lifecycle"
/// risk for the follow-up.
pub fn build_prompt_from_path(path: &[ConversationMessage]) -> Vec<ContentBlock> {
    path.iter()
        .rev()
        .find(|m| m.role == NodeRole::User)
        .map(|m| vec![ContentBlock::from(m.content.clone())])
        .unwrap_or_default()
}

/// Events the IO shell pushes into the GPUI side per active stream.
///
/// Applied via [`AppState::apply_stream_event`]. Pure mutation — no GPUI
/// types — so unit tests can drive it without standing up a `Context`.
#[derive(Clone, Debug, PartialEq)]
pub enum StreamEvent {
    Chunk(String),
    Complete,
    Error(String),
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum AgentProvider {
    ClaudeCode,
    GeminiCli,
}

impl AgentProvider {
    pub fn short_name(&self) -> &'static str {
        match self {
            AgentProvider::ClaudeCode => "Claude",
            AgentProvider::GeminiCli => "Gemini",
        }
    }
}

pub struct AppState {
    pub graph: Graph,
    pub selected: Option<NodeId>,
    pub editing: Option<NodeId>,
    pub streaming: HashSet<NodeId>,
    pub default_provider: AgentProvider,
    pub project_path: Option<String>,
    /// `None` if the ACP worker thread failed to start. The Send-to-agent
    /// path falls back to writing an error into the assistant node.
    pub acp_client: Option<AcpClient>,
}

impl AppState {
    pub fn new() -> Self {
        let acp_client = match AcpClient::spawn() {
            Ok(c) => Some(c),
            Err(err) => {
                tracing::error!("AcpClient::spawn failed: {err:?}");
                None
            }
        };
        let mut state = Self {
            graph: GraphMutations::empty(),
            selected: None,
            editing: None,
            streaming: HashSet::new(),
            default_provider: AgentProvider::ClaudeCode,
            project_path: None,
            acp_client,
        };
        state.seed_demo_graph();
        state
    }

    /// Spawn the drain loop that pulls `(NodeId, StreamEvent)` from the
    /// ACP worker thread and applies it to graph state. Call once after
    /// the entity is constructed.
    pub fn start_event_drain(&self, cx: &mut Context<Self>) {
        let Some(client) = self.acp_client.as_ref() else {
            return;
        };
        let events = client.events();
        cx.spawn(async move |this, cx| {
            while let Ok((id, event)) = events.recv().await {
                this.update(cx, |state, cx| {
                    state.apply_stream_event(&id, event);
                    cx.notify();
                })
                .ok();
            }
            tracing::warn!("acp event drain loop ended");
        })
        .detach();
    }

    fn seed_demo_graph(&mut self) {
        // A tiny demo DAG so the prototype renders something on first open.
        let u1 = self.create_user_at(Position { x: 80.0, y: 80.0 });
        self.set_content(&u1, "What's a DAG-shaped chat?".into());

        let a1 = self.create_assistant_downstream(&u1);
        self.set_content(
            &a1,
            "It's a conversation tree where you can branch, merge, and revisit prior turns instead of being locked into a single linear thread.".into(),
        );

        let a2 = self.create_assistant_downstream(&u1);
        self.set_content(
            &a2,
            "Alternative framing: each node is a message; edges encode 'this came after that'.".into(),
        );

        let synth = self.create_user_downstream(&a1);
        self.add_parent(&synth, &a2);
        self.set_content(&synth, "Combine those — when would I want to branch?".into());

        self.selected = Some(synth);
    }

    pub fn create_user_at(&mut self, position: Position) -> NodeId {
        let id = Uuid::new_v4().to_string();
        let node = GraphNode {
            id: id.clone(),
            role: NodeRole::User,
            content: String::new(),
            timestamp: Utc::now().timestamp_millis(),
            provider: None,
            model: None,
        };
        GraphMutations::add_node(&mut self.graph, node, position);
        id
    }

    pub fn create_user_downstream(&mut self, parent: &NodeId) -> NodeId {
        let parent_pos = self
            .graph
            .layout
            .get(parent)
            .copied()
            .unwrap_or(Position { x: 0.0, y: 0.0 });
        let pos = Position {
            x: parent_pos.x,
            y: parent_pos.y + 180.0,
        };
        let id = self.create_user_at(pos);
        GraphMutations::add_edge(&mut self.graph, parent, &id);
        id
    }

    pub fn create_assistant_downstream(&mut self, parent: &NodeId) -> NodeId {
        let parent_pos = self
            .graph
            .layout
            .get(parent)
            .copied()
            .unwrap_or(Position { x: 0.0, y: 0.0 });
        // Fan out siblings horizontally so the demo doesn't stack on itself.
        let existing_children = self.graph.edges.iter().filter(|e| &e.source == parent).count();
        let pos = Position {
            x: parent_pos.x + (existing_children as f32) * 280.0,
            y: parent_pos.y + 180.0,
        };
        let id = Uuid::new_v4().to_string();
        let node = GraphNode {
            id: id.clone(),
            role: NodeRole::Assistant,
            content: String::new(),
            timestamp: Utc::now().timestamp_millis(),
            provider: Some(self.default_provider.short_name().into()),
            model: None,
        };
        GraphMutations::add_node(&mut self.graph, node, pos);
        GraphMutations::add_edge(&mut self.graph, parent, &id);
        id
    }

    pub fn add_parent(&mut self, target: &NodeId, parent: &NodeId) {
        GraphMutations::add_edge(&mut self.graph, parent, target);
    }

    pub fn set_content(&mut self, id: &NodeId, content: String) {
        GraphMutations::set_content(&mut self.graph, id, content);
    }

    pub fn set_position(&mut self, id: &NodeId, position: Position) {
        GraphMutations::set_position(&mut self.graph, id, position);
    }

    pub fn delete_node(&mut self, id: &NodeId) {
        GraphMutations::remove_node(&mut self.graph, id);
        if self.selected.as_ref() == Some(id) {
            self.selected = None;
        }
        if self.editing.as_ref() == Some(id) {
            self.editing = None;
        }
        self.streaming.remove(id);
    }

    /// Apply one stream event to the state. Pure mutation: no GPUI side
    /// effects, no `Context` plumbing. The caller (typically a `cx.spawn`
    /// drain loop) is responsible for `cx.notify()` after.
    pub fn apply_stream_event(&mut self, id: &NodeId, event: StreamEvent) {
        match event {
            StreamEvent::Chunk(text) => {
                GraphMutations::append_content(&mut self.graph, id, &text);
            }
            StreamEvent::Complete | StreamEvent::Error(_) => {
                self.streaming.remove(id);
            }
        }
    }

    /// Branch off a fresh assistant node from `parent`, build the prompt
    /// from the conversation path, and submit it to ACP. Returns the new
    /// assistant `NodeId` so the caller can update selection.
    ///
    /// If the ACP client failed to start, writes an error message into
    /// the assistant node instead of streaming.
    pub fn start_agent_reply(&mut self, parent: &NodeId, cx: &mut Context<Self>) -> NodeId {
        let id = self.create_assistant_downstream(parent);
        let path = GraphModel::conversation_path(&self.graph, parent);
        let prompt = build_prompt_from_path(&path);

        if prompt.is_empty() {
            GraphMutations::set_content(
                &mut self.graph,
                &id,
                "(no user content in path — write something in the parent node first)".into(),
            );
            cx.notify();
            return id;
        }

        match self.acp_client.as_ref() {
            Some(client) => {
                self.streaming.insert(id.clone());
                client.submit(PromptJob {
                    node_id: id.clone(),
                    prompt,
                });
            }
            None => {
                GraphMutations::set_content(
                    &mut self.graph,
                    &id,
                    "ACP client failed to start. See logs (RUST_LOG=info).".into(),
                );
            }
        }
        cx.notify();
        id
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn empty_state() -> AppState {
        AppState {
            graph: GraphMutations::empty(),
            selected: None,
            editing: None,
            streaming: HashSet::new(),
            default_provider: AgentProvider::ClaudeCode,
            project_path: None,
            acp_client: None,
        }
    }

    fn seed_assistant(state: &mut AppState, id: &str, content: &str) {
        let node = GraphNode {
            id: id.to_string(),
            role: NodeRole::Assistant,
            content: content.to_string(),
            timestamp: 0,
            provider: None,
            model: None,
        };
        GraphMutations::add_node(&mut state.graph, node, Position::default());
    }

    #[test]
    fn chunk_event_appends_to_node_content() {
        let mut state = empty_state();
        seed_assistant(&mut state, "n1", "");
        state.apply_stream_event(&"n1".to_string(), StreamEvent::Chunk("hi".into()));
        assert_eq!(state.graph.nodes.get("n1").unwrap().content, "hi");
    }

    #[test]
    fn two_chunks_preserve_order() {
        let mut state = empty_state();
        seed_assistant(&mut state, "n1", "");
        state.apply_stream_event(&"n1".to_string(), StreamEvent::Chunk("hel".into()));
        state.apply_stream_event(&"n1".to_string(), StreamEvent::Chunk("lo".into()));
        assert_eq!(state.graph.nodes.get("n1").unwrap().content, "hello");
    }

    #[test]
    fn chunk_appends_to_existing_content() {
        let mut state = empty_state();
        seed_assistant(&mut state, "n1", "pre-");
        state.apply_stream_event(&"n1".to_string(), StreamEvent::Chunk("post".into()));
        assert_eq!(state.graph.nodes.get("n1").unwrap().content, "pre-post");
    }

    #[test]
    fn complete_event_clears_streaming_flag() {
        let mut state = empty_state();
        seed_assistant(&mut state, "n1", "");
        state.streaming.insert("n1".to_string());
        state.apply_stream_event(&"n1".to_string(), StreamEvent::Complete);
        assert!(!state.streaming.contains("n1"));
    }

    #[test]
    fn error_event_clears_streaming_flag() {
        let mut state = empty_state();
        seed_assistant(&mut state, "n1", "");
        state.streaming.insert("n1".to_string());
        state.apply_stream_event(
            &"n1".to_string(),
            StreamEvent::Error("subprocess died".into()),
        );
        assert!(!state.streaming.contains("n1"));
    }

    #[test]
    fn chunk_for_unknown_id_is_noop() {
        // The IO thread can race ahead of node deletion. Pinning the
        // contract: dropping the chunk is fine; panicking is not.
        let mut state = empty_state();
        state.apply_stream_event(&"ghost".to_string(), StreamEvent::Chunk("hi".into()));
        state.apply_stream_event(&"ghost".to_string(), StreamEvent::Complete);
        state.apply_stream_event(&"ghost".to_string(), StreamEvent::Error("oops".into()));
        assert!(state.graph.nodes.is_empty());
    }

    #[test]
    fn build_prompt_from_path_uses_last_user_message() {
        use crate::graph::model::ConversationMessage;
        let path = vec![
            ConversationMessage {
                role: NodeRole::User,
                content: "first turn".into(),
            },
            ConversationMessage {
                role: NodeRole::Assistant,
                content: "answer".into(),
            },
            ConversationMessage {
                role: NodeRole::User,
                content: "follow-up".into(),
            },
        ];
        let prompt = build_prompt_from_path(&path);
        assert_eq!(prompt.len(), 1, "v1 sends one ContentBlock");
        let agent_client_protocol::ContentBlock::Text(text) = &prompt[0] else {
            panic!("expected text block");
        };
        assert_eq!(text.text, "follow-up");
    }

    #[test]
    fn build_prompt_from_path_empty_when_no_user_message() {
        let prompt = build_prompt_from_path(&[]);
        assert!(prompt.is_empty());
    }
}
