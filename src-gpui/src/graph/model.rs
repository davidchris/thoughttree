//! Pure traversal API over a Graph value.
//!
//! Mirrors the language in CONTEXT.md: parents, children, ancestors, descendants,
//! and conversation_path (topo-sorted by timestamp, with consecutive same-role
//! messages merged).

use super::types::{Graph, NodeId, NodeRole};
use std::collections::{HashMap, HashSet, VecDeque};

pub struct GraphModel;

#[derive(Clone, Debug)]
pub struct ConversationMessage {
    pub role: NodeRole,
    pub content: String,
}

struct Adjacency {
    parents: HashMap<NodeId, Vec<NodeId>>,
    children: HashMap<NodeId, Vec<NodeId>>,
}

fn adjacency(g: &Graph) -> Adjacency {
    let mut parents: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
    for e in &g.edges {
        parents
            .entry(e.target.clone())
            .or_default()
            .push(e.source.clone());
        children
            .entry(e.source.clone())
            .or_default()
            .push(e.target.clone());
    }
    Adjacency { parents, children }
}

fn bfs(start: &NodeId, neighbours: &HashMap<NodeId, Vec<NodeId>>) -> HashSet<NodeId> {
    let mut visited: HashSet<NodeId> = HashSet::new();
    let mut queue: VecDeque<NodeId> = VecDeque::new();
    if let Some(seeds) = neighbours.get(start) {
        queue.extend(seeds.iter().cloned());
    }
    while let Some(cur) = queue.pop_front() {
        if !visited.insert(cur.clone()) {
            continue;
        }
        if let Some(next) = neighbours.get(&cur) {
            queue.extend(next.iter().cloned());
        }
    }
    visited
}

impl GraphModel {
    pub fn parents(g: &Graph, id: &NodeId) -> Vec<NodeId> {
        g.edges
            .iter()
            .filter(|e| &e.target == id)
            .map(|e| e.source.clone())
            .collect()
    }

    pub fn children(g: &Graph, id: &NodeId) -> Vec<NodeId> {
        g.edges
            .iter()
            .filter(|e| &e.source == id)
            .map(|e| e.target.clone())
            .collect()
    }

    pub fn ancestors(g: &Graph, id: &NodeId) -> HashSet<NodeId> {
        bfs(id, &adjacency(g).parents)
    }

    pub fn descendants(g: &Graph, id: &NodeId) -> HashSet<NodeId> {
        bfs(id, &adjacency(g).children)
    }

    /// Topo-sort ancestors + target by timestamp; cycle-tolerant fallback
    /// preserves all included nodes rather than silently dropping any.
    pub fn conversation_path_ids(g: &Graph, target: &NodeId) -> Vec<NodeId> {
        let mut include = Self::ancestors(g, target);
        include.insert(target.clone());

        let adj = adjacency(g);
        let mut in_degree: HashMap<NodeId, usize> = HashMap::new();
        for id in &include {
            in_degree.insert(id.clone(), 0);
        }
        for id in &include {
            if let Some(ps) = adj.parents.get(id) {
                let count = ps.iter().filter(|p| include.contains(*p)).count();
                in_degree.insert(id.clone(), count);
            }
        }

        let mut ready: Vec<NodeId> = in_degree
            .iter()
            .filter_map(|(id, deg)| (*deg == 0).then(|| id.clone()))
            .collect();

        let ts_of = |id: &NodeId| g.nodes.get(id).map(|n| n.timestamp).unwrap_or(0);

        let mut result: Vec<NodeId> = Vec::with_capacity(include.len());
        let mut emitted: HashSet<NodeId> = HashSet::new();

        while !ready.is_empty() {
            ready.sort_by_key(|id| ts_of(id));
            let next = ready.remove(0);
            emitted.insert(next.clone());
            result.push(next.clone());
            if let Some(cs) = adj.children.get(&next) {
                for child in cs {
                    if !include.contains(child) {
                        continue;
                    }
                    let entry = in_degree.entry(child.clone()).or_insert(0);
                    if *entry > 0 {
                        *entry -= 1;
                    }
                    if *entry == 0 && !emitted.contains(child) && !ready.contains(child) {
                        ready.push(child.clone());
                    }
                }
            }
        }

        if emitted.len() < include.len() {
            let mut leftover: Vec<NodeId> =
                include.into_iter().filter(|id| !emitted.contains(id)).collect();
            leftover.sort_by_key(|id| ts_of(id));
            result.extend(leftover);
        }

        result
    }

    pub fn conversation_path(g: &Graph, target: &NodeId) -> Vec<ConversationMessage> {
        let ids = Self::conversation_path_ids(g, target);
        let mut merged: Vec<ConversationMessage> = Vec::new();
        for id in ids {
            let Some(node) = g.nodes.get(&id) else {
                continue;
            };
            if node.content.trim().is_empty() {
                continue;
            }
            if let Some(last) = merged.last_mut() {
                if last.role == node.role {
                    last.content.push_str("\n\n");
                    last.content.push_str(&node.content);
                    continue;
                }
            }
            merged.push(ConversationMessage {
                role: node.role,
                content: node.content.clone(),
            });
        }
        merged
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::graph::mutations::GraphMutations;
    use crate::graph::types::{GraphNode, Position};

    fn user(id: &str, content: &str, ts: i64) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            role: NodeRole::User,
            content: content.to_string(),
            timestamp: ts,
            provider: None,
            model: None,
        }
    }

    fn assistant(id: &str, content: &str, ts: i64) -> GraphNode {
        GraphNode {
            id: id.to_string(),
            role: NodeRole::Assistant,
            content: content.to_string(),
            timestamp: ts,
            provider: None,
            model: None,
        }
    }

    #[test]
    fn synthesizer_topo_sorts_by_timestamp_and_merges_same_role() {
        let mut g = GraphMutations::empty();
        // Two parallel branches converging into a synthesizer node.
        GraphMutations::add_node(&mut g, user("u1", "hi", 1), Position::default());
        GraphMutations::add_node(&mut g, assistant("a1", "branch A", 2), Position::default());
        GraphMutations::add_node(&mut g, assistant("a2", "branch B", 3), Position::default());
        GraphMutations::add_node(&mut g, user("u2", "synth", 4), Position::default());
        GraphMutations::add_edge(&mut g, &"u1".to_string(), &"a1".to_string());
        GraphMutations::add_edge(&mut g, &"u1".to_string(), &"a2".to_string());
        GraphMutations::add_edge(&mut g, &"a1".to_string(), &"u2".to_string());
        GraphMutations::add_edge(&mut g, &"a2".to_string(), &"u2".to_string());

        let path = GraphModel::conversation_path(&g, &"u2".to_string());
        assert_eq!(path.len(), 3, "user, merged-assistant, user");
        assert_eq!(path[0].role, NodeRole::User);
        assert_eq!(path[1].role, NodeRole::Assistant);
        assert!(path[1].content.contains("branch A"));
        assert!(path[1].content.contains("branch B"));
        assert_eq!(path[2].role, NodeRole::User);
    }
}
