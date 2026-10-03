use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use crate::{Graph, NodeId, Position};

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum LayoutDirection {
    #[default]
    TopToBottom,
    LeftToRight,
}

#[derive(Clone, Copy, Debug)]
pub struct LayoutOptions {
    pub direction: LayoutDirection,
    pub grid_size: f64,
    pub node_gap: f64,
    pub level_gap: f64,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            direction: LayoutDirection::TopToBottom,
            grid_size: 20.0,
            node_gap: 210.0,
            level_gap: 160.0,
        }
    }
}

impl Graph {
    pub fn auto_layout(&mut self, options: LayoutOptions) {
        self.layout = self.compute_auto_layout(options);
    }

    /// A tidy spanning-tree layout. First incoming edge selects a placement
    /// parent; every other GraphEdge remains present, including synthesis edges.
    pub fn compute_auto_layout(&self, options: LayoutOptions) -> IndexMap<NodeId, Position> {
        if self.nodes.is_empty() {
            return IndexMap::new();
        }
        let mut parent = IndexMap::new();
        for edge in &self.edges {
            if self.nodes.contains_key(&edge.source) && self.nodes.contains_key(&edge.target) {
                parent
                    .entry(edge.target.clone())
                    .or_insert_with(|| edge.source.clone());
            }
        }
        let mut children: HashMap<NodeId, Vec<NodeId>> = HashMap::new();
        for (child, parent) in &parent {
            children
                .entry(parent.clone())
                .or_default()
                .push(child.clone());
        }
        for children in children.values_mut() {
            children.sort_by(|a, b| {
                self.position(a)
                    .x
                    .total_cmp(&self.position(b).x)
                    .then(self.position(a).y.total_cmp(&self.position(b).y))
            });
        }
        let mut roots: Vec<_> = self
            .nodes
            .keys()
            .filter(|id| !parent.contains_key(*id))
            .cloned()
            .collect();
        roots.sort_by(|a, b| {
            self.position(a)
                .y
                .total_cmp(&self.position(b).y)
                .then(self.position(a).x.total_cmp(&self.position(b).x))
        });
        let mut slots = HashMap::new();
        let mut depths = HashMap::new();
        let mut cursor = 0.0;
        for root in roots {
            let mut visiting = HashSet::new();
            place(
                &root,
                0,
                &children,
                &mut cursor,
                &mut slots,
                &mut depths,
                &mut visiting,
            );
            cursor += 1.0;
        }
        let anchor = self.nodes.keys().fold(
            Position {
                x: f64::INFINITY,
                y: f64::INFINITY,
            },
            |point, id| {
                let position = self.position(id);
                Position {
                    x: point.x.min(position.x),
                    y: point.y.min(position.y),
                }
            },
        );
        let min_slot = slots.values().copied().fold(f64::INFINITY, f64::min);
        self.nodes
            .keys()
            .map(|id| {
                let position = match (slots.get(id), depths.get(id)) {
                    (Some(slot), Some(depth)) => {
                        let across = (slot - min_slot) * options.node_gap;
                        let along = *depth as f64 * options.level_gap;
                        match options.direction {
                            LayoutDirection::TopToBottom => Position {
                                x: anchor.x + across,
                                y: anchor.y + along,
                            },
                            LayoutDirection::LeftToRight => Position {
                                x: anchor.x + along,
                                y: anchor.y + across,
                            },
                        }
                    }
                    _ => self.position(id),
                };
                (
                    id.clone(),
                    Position {
                        x: snap(position.x, options.grid_size),
                        y: snap(position.y, options.grid_size),
                    },
                )
            })
            .collect()
    }

    fn position(&self, id: &str) -> Position {
        self.layout.get(id).copied().unwrap_or_default()
    }
}

// Explicit stack avoids recursion limits for long imported conversations.
fn place(
    root: &str,
    depth: usize,
    children: &HashMap<NodeId, Vec<NodeId>>,
    cursor: &mut f64,
    slots: &mut HashMap<NodeId, f64>,
    depths: &mut HashMap<NodeId, usize>,
    visiting: &mut HashSet<NodeId>,
) {
    let mut stack = vec![(root.to_owned(), depth, false)];
    while let Some((id, depth, finish)) = stack.pop() {
        let kids = children.get(&id).map(Vec::as_slice).unwrap_or_default();
        if finish {
            let first = kids.iter().find_map(|child| slots.get(child)).copied();
            let last = kids
                .iter()
                .rev()
                .find_map(|child| slots.get(child))
                .copied();
            let slot = match (first, last) {
                (Some(first), Some(last)) => (first + last) / 2.0,
                _ => {
                    let slot = *cursor;
                    *cursor += 1.0;
                    slot
                }
            };
            slots.insert(id, slot);
            continue;
        }
        if !visiting.insert(id.clone()) {
            continue;
        }
        depths.insert(id.clone(), depth);
        stack.push((id, depth, true));
        for child in kids.iter().rev() {
            stack.push((child.clone(), depth + 1, false));
        }
    }
}

fn snap(value: f64, grid: f64) -> f64 {
    // Math.round in the browser rounds negative ties toward positive infinity.
    if grid == 0.0 {
        value
    } else {
        (value / grid + 0.5).floor() * grid
    }
}
