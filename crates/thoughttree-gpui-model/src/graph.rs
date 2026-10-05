use std::collections::{HashMap, HashSet, VecDeque};

use indexmap::{IndexMap, IndexSet};
use thiserror::Error;

use crate::{ConversationMessage, FileRef, GraphEdge, GraphNode, NodeId, NodeKind, Position, Role};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Graph {
    pub nodes: IndexMap<NodeId, GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub layout: IndexMap<NodeId, Position>,
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum GraphError {
    #[error("GraphNode does not exist: {0}")]
    MissingNode(String),
    #[error("File nodes cannot have incoming GraphEdges")]
    FileTarget,
    #[error("This GraphEdge would create a cycle")]
    Cycle,
    #[error("File node content comes from the Vault and cannot be edited")]
    FileContent,
    #[error("Only assistant nodes can stream")]
    NotAssistant,
    #[error("Wait for the current response to finish before changing its lineage")]
    ActiveTurn,
}

impl Graph {
    pub fn insert_node(&mut self, node: GraphNode, position: Position) {
        self.add_node(node, position);
    }

    pub fn connect(&mut self, source: &str, target: &str) -> Result<bool, GraphError> {
        self.add_edge(source, target)
    }

    pub fn remove_nodes(&mut self, ids: &[NodeId]) {
        for id in ids {
            self.remove_node(id);
        }
    }

    pub fn add_node(&mut self, node: GraphNode, position: Position) {
        self.layout.insert(node.id.clone(), position);
        self.nodes.insert(node.id.clone(), node);
    }

    pub fn add_edge(&mut self, source: &str, target: &str) -> Result<bool, GraphError> {
        if !self.nodes.contains_key(source) {
            return Err(GraphError::MissingNode(source.into()));
        }
        let target_node = self
            .nodes
            .get(target)
            .ok_or_else(|| GraphError::MissingNode(target.into()))?;
        if target_node.role() == Role::File {
            return Err(GraphError::FileTarget);
        }
        if self
            .edges
            .iter()
            .any(|edge| edge.source == source && edge.target == target)
        {
            return Ok(false);
        }
        if source == target || self.descendants(target).contains(source) {
            return Err(GraphError::Cycle);
        }
        self.edges.push(GraphEdge {
            id: format!("{source}->{target}"),
            source: source.into(),
            target: target.into(),
        });
        Ok(true)
    }

    pub fn remove_node(&mut self, id: &str) -> bool {
        if self.nodes.shift_remove(id).is_none() {
            return false;
        }
        self.layout.shift_remove(id);
        self.edges
            .retain(|edge| edge.source != id && edge.target != id);
        true
    }

    pub fn remove_edge(&mut self, id: &str) -> bool {
        let count = self.edges.len();
        self.edges.retain(|edge| edge.id != id);
        self.edges.len() != count
    }

    pub fn set_content(
        &mut self,
        id: &str,
        content: String,
        timestamp: f64,
    ) -> Result<(), GraphError> {
        let node = self
            .nodes
            .get_mut(id)
            .ok_or_else(|| GraphError::MissingNode(id.into()))?;
        if node.role() == Role::File {
            return Err(GraphError::FileContent);
        }
        if node.content == content {
            return Ok(());
        }
        node.content = content;
        node.content_updated_at = Some(timestamp);
        node.summary_timestamp = None;
        Ok(())
    }

    pub fn set_position(&mut self, id: &str, position: Position) -> Result<(), GraphError> {
        if !self.nodes.contains_key(id) {
            return Err(GraphError::MissingNode(id.into()));
        }
        self.layout.insert(id.into(), position);
        Ok(())
    }

    pub fn parents(&self, id: &str) -> Vec<NodeId> {
        self.edges
            .iter()
            .filter(|edge| edge.target == id)
            .map(|edge| edge.source.clone())
            .collect()
    }

    pub fn children(&self, id: &str) -> Vec<NodeId> {
        self.edges
            .iter()
            .filter(|edge| edge.source == id)
            .map(|edge| edge.target.clone())
            .collect()
    }

    fn traverse(&self, id: &str, parents: bool) -> IndexSet<NodeId> {
        let mut visited = IndexSet::new();
        let mut queue = VecDeque::from([id.to_owned()]);
        while let Some(id) = queue.pop_front() {
            let neighbours = if parents {
                self.parents(&id)
            } else {
                self.children(&id)
            };
            for next in neighbours {
                if visited.insert(next.clone()) {
                    queue.push_back(next);
                }
            }
        }
        visited
    }

    pub fn ancestors(&self, id: &str) -> IndexSet<NodeId> {
        self.traverse(id, true)
    }
    pub fn descendants(&self, id: &str) -> IndexSet<NodeId> {
        self.traverse(id, false)
    }

    pub fn has_non_linear_lineage(&self, target: &str) -> bool {
        let mut include = self.ancestors(target);
        include.insert(target.into());
        include.iter().any(|id| {
            self.parents(id)
                .iter()
                .filter(|id| include.contains(*id))
                .count()
                > 1
                || self
                    .children(id)
                    .iter()
                    .filter(|id| include.contains(*id))
                    .count()
                    > 1
        })
    }

    /// Stable Kahn traversal: equal timestamps retain ancestor discovery order,
    /// matching the browser's stable Array.sort rather than sorting by UUID.
    fn topological_ids(&self, include: &IndexSet<NodeId>) -> Vec<NodeId> {
        let mut degrees: IndexMap<NodeId, usize> = include
            .iter()
            .map(|id| {
                (
                    id.clone(),
                    self.parents(id)
                        .iter()
                        .filter(|parent| include.contains(*parent))
                        .count(),
                )
            })
            .collect();
        let mut ready: Vec<NodeId> = degrees
            .iter()
            .filter(|(_, degree)| **degree == 0)
            .map(|(id, _)| id.clone())
            .collect();
        let mut result = Vec::new();
        while !ready.is_empty() {
            ready.sort_by(|left, right| self.timestamp(left).total_cmp(&self.timestamp(right)));
            let next = ready.remove(0);
            result.push(next.clone());
            for child in self.children(&next) {
                let Some(degree) = degrees.get_mut(&child) else {
                    continue;
                };
                *degree = degree.saturating_sub(1);
                if *degree == 0 {
                    ready.push(child);
                }
            }
        }
        // Old Project files can contain cycles. Reading them preserves text;
        // mutations still reject new cycles, and traversal always terminates.
        let emitted: HashSet<_> = result.iter().cloned().collect();
        let mut leftover: Vec<_> = include
            .iter()
            .filter(|id| !emitted.contains(*id))
            .cloned()
            .collect();
        leftover.sort_by(|left, right| self.timestamp(left).total_cmp(&self.timestamp(right)));
        result.extend(leftover);
        result
    }

    fn timestamp(&self, id: &str) -> f64 {
        self.nodes.get(id).map_or(0.0, |node| node.timestamp)
    }

    pub fn conversation_path_ids(&self, target: &str) -> Vec<NodeId> {
        let mut include = self.ancestors(target);
        include.insert(target.into());
        self.topological_ids(&include)
    }

    pub fn conversation_path(&self, target: &str) -> Vec<ConversationMessage> {
        let ids = self.conversation_path_ids(target);
        let structure = self
            .has_non_linear_lineage(target)
            .then(|| Lineage::new(self, &ids));
        let mut messages = Vec::new();
        for id in &ids {
            let Some(node) = self.nodes.get(id) else {
                continue;
            };
            let Some(mut message) = segment(node) else {
                continue;
            };
            if let Some(lineage) = &structure {
                message.content = lineage.marker(node, &message.content);
            }
            merge_message(&mut messages, message);
        }
        if let (Some(lineage), Some(last)) = (structure, messages.last_mut()) {
            last.content.push_str("\n\n");
            last.content.push_str(&lineage.map(target));
        }
        messages
    }

    pub fn can_generate(&self, id: &str) -> bool {
        let Some(node) = self.nodes.get(id).filter(|node| node.role() == Role::User) else {
            return false;
        };
        !node.content.trim().is_empty()
            || !node.images().is_empty()
            || self.ancestors(id).iter().any(|ancestor| {
                self.nodes
                    .get(ancestor)
                    .is_some_and(|node| node.role() == Role::File)
            })
    }

    /// Includes every selected node, including siblings and disconnected roots.
    pub fn selected_subgraph(&self, ids: &[NodeId]) -> Graph {
        let include: IndexSet<_> = ids.iter().cloned().collect();
        let nodes = self
            .topological_ids(&include)
            .into_iter()
            .filter_map(|id| self.nodes.get(&id).cloned().map(|node| (id, node)))
            .collect();
        let edges = self
            .edges
            .iter()
            .filter(|edge| include.contains(&edge.source) && include.contains(&edge.target))
            .cloned()
            .collect();
        let layout = self
            .layout
            .iter()
            .filter(|(id, _)| include.contains(*id))
            .map(|(id, position)| (id.clone(), *position))
            .collect();
        Graph {
            nodes,
            edges,
            layout,
        }
    }

    pub fn export_markdown(&self, ids: &[NodeId]) -> String {
        self.selected_subgraph(ids)
            .nodes
            .values()
            .map(|node| {
                if let Some(file) = node.file_data() {
                    return format!("## File\n\n{}", file.path);
                }
                let title = if node.role() == Role::User {
                    "User"
                } else {
                    "Assistant"
                };
                let mut text = format!("## {title}\n\n{}", node.content);
                if let Some(provenance) = node.provenance() {
                    text.push_str(&format!("\n\n{}", provenance.markdown()));
                }
                text
            })
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")
    }
}

fn segment(node: &GraphNode) -> Option<ConversationMessage> {
    let mut message = ConversationMessage {
        role: node.role(),
        content: node.content.clone(),
        images: node.images().to_vec(),
        files: Vec::new(),
    };
    if let Some(file) = node.file_data() {
        message.role = Role::User;
        message.content.clear();
        message.files.push(FileRef::from(file));
        return Some(message);
    }
    if message.content.trim().is_empty() {
        message.content.clear();
        if message.images.is_empty() {
            return None;
        }
    }
    Some(message)
}

fn merge_message(messages: &mut Vec<ConversationMessage>, mut message: ConversationMessage) {
    if let Some(last) = messages.last_mut().filter(|last| last.role == message.role) {
        if !message.content.is_empty() {
            if !last.content.is_empty() {
                last.content.push_str("\n\n");
            }
            last.content.push_str(&message.content);
        }
        last.images.append(&mut message.images);
        last.files.append(&mut message.files);
    } else {
        messages.push(message);
    }
}

struct Lineage<'a> {
    graph: &'a Graph,
    ids: &'a [NodeId],
    short_ids: HashMap<NodeId, String>,
    index: HashMap<NodeId, usize>,
}

impl<'a> Lineage<'a> {
    fn new(graph: &'a Graph, ids: &'a [NodeId]) -> Self {
        let mut used = HashSet::new();
        let mut short_ids = HashMap::new();
        for id in ids {
            let mut length = 4.min(id.chars().count());
            let mut short: String = id.chars().take(length).collect();
            while used.contains(&short) && length < id.chars().count() {
                length += 1;
                short = id.chars().take(length).collect();
            }
            used.insert(short.clone());
            short_ids.insert(id.clone(), short);
        }
        let index = ids
            .iter()
            .enumerate()
            .map(|(i, id)| (id.clone(), i))
            .collect();
        Self {
            graph,
            ids,
            short_ids,
            index,
        }
    }

    fn parents(&self, id: &str) -> Vec<NodeId> {
        let mut parents: Vec<_> = self
            .graph
            .parents(id)
            .into_iter()
            .filter(|id| self.index.contains_key(id))
            .collect();
        parents.sort_by_key(|id| self.index[id]);
        parents
    }

    fn marker(&self, node: &GraphNode, content: &str) -> String {
        let mut annotations = String::new();
        let parents = self.parents(&node.id);
        if node.role() == Role::User {
            if parents.len() >= 2 {
                annotations.push_str(&format!(
                    "<graph: this message merges branches {}>\n",
                    self.short_list(&parents)
                ));
            }
            if let Some(parent) = parents.iter().find(|id| {
                self.graph
                    .children(id)
                    .iter()
                    .filter(|id| self.index.contains_key(*id))
                    .count()
                    >= 2
            }) {
                annotations.push_str(&format!(
                    "<graph: this message starts a new branch from {}>\n",
                    self.short_ids[parent]
                ));
            }
        }
        let text = match &node.kind {
            NodeKind::File(file) => format!("[file: {}]", file.name),
            _ if content.is_empty() => "[image attached]".to_owned(),
            _ => content.to_owned(),
        };
        format!(
            "<node id=\"{}\">\n{annotations}{text}\n</node>",
            self.short_ids[&node.id]
        )
    }

    fn short_list(&self, ids: &[NodeId]) -> String {
        ids.iter()
            .map(|id| self.short_ids[id].as_str())
            .collect::<Vec<_>>()
            .join(", ")
    }

    fn map(&self, target: &str) -> String {
        let lines = self
            .ids
            .iter()
            .map(|id| {
                let parents = self.parents(id);
                let parent_text = if parents.is_empty() {
                    "(root)".into()
                } else {
                    self.short_list(&parents)
                };
                let current = if id == target { " [current]" } else { "" };
                let role = self
                    .graph
                    .nodes
                    .get(id)
                    .map_or("unknown", |node| node.role().as_str());
                format!("{} ({role}) <- {parent_text}{current}", self.short_ids[id])
            })
            .collect::<Vec<_>>()
            .join("\n");
        format!("<graph-map>\nThis conversation is a DAG, not a line: the messages above are a linearization of the current node's ancestor graph. <node id> markers tie each text segment to a graph node; the map below is the topology.\n{lines}\n</graph-map>")
    }
}
