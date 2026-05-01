use super::types::{Graph, GraphEdge, GraphNode, NodeId, Position};

pub struct GraphMutations;

impl GraphMutations {
    pub fn empty() -> Graph {
        Graph::default()
    }

    pub fn add_node(g: &mut Graph, node: GraphNode, position: Position) {
        g.layout.insert(node.id.clone(), position);
        g.nodes.insert(node.id.clone(), node);
    }

    pub fn add_edge(g: &mut Graph, source: &NodeId, target: &NodeId) {
        let id = format!("{source}->{target}");
        if g.edges.iter().any(|e| e.id == id) {
            return;
        }
        g.edges.push(GraphEdge {
            id,
            source: source.clone(),
            target: target.clone(),
        });
    }

    pub fn remove_node(g: &mut Graph, id: &NodeId) {
        if g.nodes.remove(id).is_none() {
            return;
        }
        g.layout.remove(id);
        g.edges.retain(|e| &e.source != id && &e.target != id);
    }

    pub fn set_content(g: &mut Graph, id: &NodeId, content: String) {
        if let Some(node) = g.nodes.get_mut(id) {
            node.content = content;
        }
    }

    pub fn append_content(g: &mut Graph, id: &NodeId, chunk: &str) {
        if let Some(node) = g.nodes.get_mut(id) {
            node.content.push_str(chunk);
        }
    }

    pub fn set_position(g: &mut Graph, id: &NodeId, position: Position) {
        g.layout.insert(id.clone(), position);
    }
}
