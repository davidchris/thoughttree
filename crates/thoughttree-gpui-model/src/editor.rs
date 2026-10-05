use indexmap::{IndexMap, IndexSet};

use crate::{normalize_provenance, now_ms, GraphError, NodeId, NodeKind, Project};

#[derive(Clone, Debug)]
struct HistoryEntry {
    project: Project,
    selection: IndexSet<NodeId>,
}

/// Session state is deliberately separate from serialized Graph data. A save
/// completion may clear dirty only for the exact edit revision it captured.
#[derive(Clone, Debug)]
pub struct Editor {
    pub project: Project,
    pub selection: IndexSet<NodeId>,
    pub active_turns: IndexMap<NodeId, String>,
    pub edit_revision: u64,
    saved_revision: u64,
    undo: Vec<HistoryEntry>,
    redo: Vec<HistoryEntry>,
}

impl Default for Editor {
    fn default() -> Self {
        Self::new(Project::default())
    }
}

impl Editor {
    pub fn new(project: Project) -> Self {
        Self {
            project,
            selection: IndexSet::new(),
            active_turns: IndexMap::new(),
            edit_revision: 0,
            saved_revision: 0,
            undo: Vec::new(),
            redo: Vec::new(),
        }
    }

    pub fn is_dirty(&self) -> bool {
        self.edit_revision != self.saved_revision
    }

    pub fn mark_saved(&mut self, captured_revision: u64) {
        if captured_revision == self.edit_revision {
            self.saved_revision = captured_revision;
        }
    }

    pub fn touch(&mut self) {
        self.edit_revision = self.edit_revision.wrapping_add(1);
    }

    fn snapshot(&self) -> HistoryEntry {
        HistoryEntry {
            project: self.project.clone(),
            selection: self.selection.clone(),
        }
    }

    /// One user operation is one undo step. A rejected operation restores the
    /// previous Graph and leaves selection, dirty state and redo untouched.
    pub fn edit<T>(
        &mut self,
        operation: impl FnOnce(&mut Project) -> Result<T, GraphError>,
    ) -> Result<T, GraphError> {
        let before = self.snapshot();
        let result = operation(&mut self.project);
        if result.is_err() {
            self.project = before.project;
            return result;
        }
        if self.project != before.project {
            self.undo.push(before);
            if self.undo.len() > 100 {
                self.undo.remove(0);
            }
            self.redo.clear();
            self.selection
                .retain(|id| self.project.graph.nodes.contains_key(id));
            self.touch();
        }
        result
    }

    pub fn undo(&mut self) -> bool {
        if !self.active_turns.is_empty() {
            return false;
        }
        let Some(previous) = self.undo.pop() else {
            return false;
        };
        self.redo.push(self.snapshot());
        self.project = previous.project;
        self.selection = previous.selection;
        self.touch();
        true
    }

    pub fn redo(&mut self) -> bool {
        if !self.active_turns.is_empty() {
            return false;
        }
        let Some(next) = self.redo.pop() else {
            return false;
        };
        self.undo.push(self.snapshot());
        self.project = next.project;
        self.selection = next.selection;
        self.touch();
        true
    }

    /// Caller must preserve a Recovery snapshot before replacing dirty work.
    pub fn replace_project(&mut self, project: Project) -> Result<(), GraphError> {
        if !self.active_turns.is_empty() {
            return Err(GraphError::ActiveTurn);
        }
        self.project = project;
        self.selection.clear();
        self.undo.clear();
        self.redo.clear();
        self.touch();
        self.saved_revision = self.edit_revision;
        Ok(())
    }

    pub fn start_turn(&mut self, node_id: &str, turn_id: &str) -> Result<(), GraphError> {
        if self.is_node_blocked(node_id) {
            return Err(GraphError::ActiveTurn);
        }
        if !self
            .project
            .graph
            .nodes
            .get(node_id)
            .is_some_and(|node| matches!(node.kind, NodeKind::Assistant(_)))
        {
            return Err(GraphError::NotAssistant);
        }
        if turn_id.is_empty() {
            return Err(GraphError::ActiveTurn);
        }
        self.active_turns.insert(node_id.into(), turn_id.into());
        Ok(())
    }

    pub fn begin_turn(&mut self, node_id: &str) -> Result<String, GraphError> {
        let turn_id = uuid::Uuid::new_v4().to_string();
        self.start_turn(node_id, &turn_id)?;
        Ok(turn_id)
    }

    pub fn append_turn(&mut self, node_id: &str, turn_id: &str, text: &str) -> bool {
        if !self.matches_turn(node_id, turn_id) {
            return false;
        }
        let Some(node) = self.project.graph.nodes.get_mut(node_id) else {
            return false;
        };
        if text.is_empty() {
            return true;
        }
        node.content.push_str(text);
        node.content_updated_at = Some(now_ms());
        node.summary_timestamp = None;
        self.update_stream_history(node_id);
        self.touch();
        true
    }

    pub fn set_turn_provenance(
        &mut self,
        node_id: &str,
        turn_id: &str,
        raw: &serde_json::Value,
    ) -> bool {
        if !self.matches_turn(node_id, turn_id) {
            return false;
        }
        let Some(provenance) = normalize_provenance(raw) else {
            return false;
        };
        let Some(node) = self.project.graph.nodes.get_mut(node_id) else {
            return false;
        };
        let NodeKind::Assistant(data) = &mut node.kind else {
            return false;
        };
        data.provenance = Some(provenance);
        self.update_stream_history(node_id);
        self.touch();
        true
    }

    pub fn finish_turn(&mut self, node_id: &str, turn_id: &str) -> bool {
        if !self.matches_turn(node_id, turn_id) {
            return false;
        }
        self.active_turns.shift_remove(node_id);
        true
    }

    /// A failed Turn keeps its partial response and records the failure on that
    /// exact assistant. Late provider failures cannot affect a replacement Turn.
    pub fn fail_turn(&mut self, node_id: &str, turn_id: &str, error: &str) -> bool {
        if !self.matches_turn(node_id, turn_id) {
            return false;
        }
        let Some(node) = self.project.graph.nodes.get_mut(node_id) else {
            return false;
        };
        let NodeKind::Assistant(assistant) = &mut node.kind else {
            return false;
        };
        assistant.incomplete = true;
        node.content.push_str(&format!("\n\n[Error: {error}]"));
        node.content_updated_at = Some(now_ms());
        node.summary_timestamp = None;
        self.update_stream_history(node_id);
        self.touch();
        self.finish_turn(node_id, turn_id)
    }

    fn matches_turn(&self, node_id: &str, turn_id: &str) -> bool {
        !turn_id.is_empty()
            && self
                .active_turns
                .get(node_id)
                .is_some_and(|active| active == turn_id)
    }

    // A user may edit an independent branch while a response streams. Undoing
    // that edit must not restore the partial response captured by its snapshot.
    // Entries preceding creation of this assistant retain its absence, so
    // undoing the generation itself still removes the node and its edges.
    fn update_stream_history(&mut self, node_id: &str) {
        let Some(current) = self.project.graph.nodes.get(node_id) else {
            return;
        };
        for snapshot in self.undo.iter_mut().chain(self.redo.iter_mut()) {
            if let Some(previous) = snapshot.project.graph.nodes.get_mut(node_id) {
                *previous = current.clone();
            }
        }
    }

    pub fn is_node_blocked(&self, node_id: &str) -> bool {
        let ancestors = self.project.graph.ancestors(node_id);
        let descendants = self.project.graph.descendants(node_id);
        self.active_turns
            .keys()
            .any(|id| id == node_id || ancestors.contains(id) || descendants.contains(id))
    }

    pub fn blocked_node_ids(&self) -> IndexSet<NodeId> {
        let mut result = IndexSet::new();
        for id in self.active_turns.keys() {
            result.insert(id.clone());
            result.extend(self.project.graph.ancestors(id));
            result.extend(self.project.graph.descendants(id));
        }
        result
    }
}
