//! Shared application state (Zustand-equivalent on the GPUI side).
//!
//! Held inside an `Entity<AppState>` and observed by every view that renders
//! graph or selection state. Mutations go through the methods on this type so
//! the surface mirrors the action set in `src/store/useGraphStore.ts`.

use crate::graph::{
    types::{GraphNode, NodeRole, Position},
    Graph, GraphMutations, NodeId,
};
use chrono::Utc;
use gpui::Context;
use std::collections::HashSet;
use uuid::Uuid;

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
}

impl AppState {
    pub fn new() -> Self {
        let mut state = Self {
            graph: GraphMutations::empty(),
            selected: None,
            editing: None,
            streaming: HashSet::new(),
            default_provider: AgentProvider::ClaudeCode,
            project_path: None,
        };
        state.seed_demo_graph();
        state
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

    /// Stub for a streaming agent reply — appends the message char-by-char on
    /// a background tick so the UI exercises live updates without an ACP
    /// subprocess. The real integration would hand off to the existing
    /// `src-tauri/src/backend/acp` module.
    pub fn simulate_stream(&mut self, id: &NodeId, full_text: String, cx: &mut Context<Self>) {
        self.streaming.insert(id.clone());
        let id = id.clone();
        cx.spawn(|this, mut cx| async move {
            for ch in full_text.chars() {
                cx.background_executor()
                    .timer(std::time::Duration::from_millis(15))
                    .await;
                this.update(&mut cx, |state, cx| {
                    GraphMutations::append_content(&mut state.graph, &id, &ch.to_string());
                    cx.notify();
                })
                .ok();
            }
            this.update(&mut cx, |state, cx| {
                state.streaming.remove(&id);
                cx.notify();
            })
            .ok();
        })
        .detach();
    }
}
