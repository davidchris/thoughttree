use std::collections::{BTreeSet, HashMap, HashSet, VecDeque};

use crate::{Graph, GraphEdge, NodeId};

/// The Lineage highlight: while exactly one GraphNode is selected, its
/// ancestors and the GraphEdges among them and into it. Every parent counts,
/// so both sides of a Synthesizer node are included. The selected GraphNode
/// itself is never part of it, even when a legacy cycle makes it its own
/// ancestor.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct LineageHighlight {
    pub nodes: BTreeSet<NodeId>,
    pub edges: BTreeSet<String>,
}

impl LineageHighlight {
    /// Empty unless `selection` holds exactly one id among `nodes`. Edges with
    /// an endpoint outside `nodes` are ignored.
    pub fn new<'a>(
        nodes: impl IntoIterator<Item = &'a NodeId>,
        edges: &[GraphEdge],
        selection: &BTreeSet<NodeId>,
    ) -> Self {
        let known: HashSet<&str> = nodes.into_iter().map(String::as_str).collect();
        let mut selected = selection.iter();
        let (Some(selected), None) = (selected.next(), selected.next()) else {
            return Self::default();
        };
        if !known.contains(selected.as_str()) {
            return Self::default();
        }

        let mut parents: HashMap<&str, Vec<&str>> = HashMap::new();
        for edge in edges {
            if known.contains(edge.source.as_str()) && known.contains(edge.target.as_str()) {
                parents.entry(&edge.target).or_default().push(&edge.source);
            }
        }
        let mut ancestors = BTreeSet::new();
        let mut queue = VecDeque::from([selected.as_str()]);
        while let Some(id) = queue.pop_front() {
            for &parent in parents.get(id).into_iter().flatten() {
                if parent != selected && ancestors.insert(parent.to_owned()) {
                    queue.push_back(parent);
                }
            }
        }

        let edges = edges
            .iter()
            .filter(|edge| {
                ancestors.contains(&edge.source)
                    && (&edge.target == selected || ancestors.contains(&edge.target))
            })
            .map(|edge| edge.id.clone())
            .collect();
        Self {
            nodes: ancestors,
            edges,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.nodes.is_empty()
    }
}

impl Graph {
    pub fn lineage_highlight(&self, selection: &BTreeSet<NodeId>) -> LineageHighlight {
        LineageHighlight::new(self.nodes.keys(), &self.edges, selection)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{GraphNode, Position};

    fn graph(nodes: &[&str], edges: &[(&str, &str)]) -> Graph {
        let mut graph = Graph::default();
        for (i, id) in nodes.iter().enumerate() {
            graph.add_node(GraphNode::user(*id, *id, i as f64), Position::default());
        }
        // Raw edges: legacy files can hold cycles and dangling endpoints.
        graph.edges = edges
            .iter()
            .map(|(source, target)| GraphEdge {
                id: format!("{source}->{target}"),
                source: (*source).into(),
                target: (*target).into(),
            })
            .collect();
        graph
    }

    fn select(ids: &[&str]) -> BTreeSet<NodeId> {
        ids.iter().map(|id| (*id).to_owned()).collect()
    }

    fn set(ids: &[&str]) -> BTreeSet<String> {
        select(ids)
    }

    #[test]
    fn lineage_of_a_linear_chain_is_every_earlier_node() {
        let graph = graph(&["a", "b", "c", "d"], &[("a", "b"), ("b", "c"), ("c", "d")]);
        let lineage = graph.lineage_highlight(&select(&["c"]));
        assert_eq!(lineage.nodes, set(&["a", "b"]));
        assert_eq!(lineage.edges, set(&["a->b", "b->c"]));
    }

    #[test]
    fn lineage_of_a_synthesizer_node_covers_every_parent_but_no_side_branch() {
        //  a   x
        //  |   |
        //  b   y
        //   \ /
        //    m     b -> c is a side branch
        let graph = graph(
            &["a", "b", "c", "m", "x", "y"],
            &[("a", "b"), ("b", "m"), ("b", "c"), ("x", "y"), ("y", "m")],
        );
        let lineage = graph.lineage_highlight(&select(&["m"]));
        assert_eq!(lineage.nodes, set(&["a", "b", "x", "y"]));
        assert_eq!(lineage.edges, set(&["a->b", "b->m", "x->y", "y->m"]));
    }

    #[test]
    fn a_legacy_cycle_never_puts_the_selected_node_in_its_own_lineage() {
        let graph = graph(&["a", "b", "c"], &[("a", "b"), ("b", "a"), ("b", "c")]);
        let lineage = graph.lineage_highlight(&select(&["b"]));
        assert_eq!(lineage.nodes, set(&["a"]));
        assert_eq!(lineage.edges, set(&["a->b"]));
    }

    #[test]
    fn edges_with_a_missing_endpoint_are_ignored() {
        let graph = graph(&["a", "b"], &[("ghost", "a"), ("a", "b"), ("b", "gone")]);
        let lineage = graph.lineage_highlight(&select(&["b"]));
        assert_eq!(lineage.nodes, set(&["a"]));
        assert_eq!(lineage.edges, set(&["a->b"]));
    }

    #[test]
    fn no_lineage_without_exactly_one_known_selected_node() {
        let graph = graph(&["a", "b", "c"], &[("a", "b"), ("b", "c")]);
        assert!(graph.lineage_highlight(&select(&[])).is_empty());
        assert!(graph.lineage_highlight(&select(&["b", "c"])).is_empty());
        assert!(graph.lineage_highlight(&select(&["ghost"])).is_empty());
        assert!(graph.lineage_highlight(&select(&["a"])).is_empty());
    }
}
