//! Tidy: the Layered layout, ported from `src/lib/layout/` in the Tauri
//! frontend. Both must turn every case in `fixtures/layout/` into exactly the
//! same positions, so this port keeps the TypeScript evaluation order:
//! the same sorts, the same iteration order and the same float operations.
//!
//! Each Graph component is laid out on its own (a small Sugiyama):
//!
//! 1. Layer = longest path from a source; then every node is pulled down to
//!    sit just above its nearest child, so a second tree joins where it merges.
//! 2. Edges that skip layers get invisible waypoints that reserve a lane.
//! 3. Layers start in depth-first order from the sources, left to right by
//!    current position. Neighbours swap only where that removes crossings, so
//!    siblings keep their current order unless a swap removes a crossing.
//! 4. x is solved per layer as a weighted least-squares fit to the neighbours,
//!    with order and spacing as hard constraints (pool adjacent violators).
//!
//! The components are then packed into shelves of a roughly 16:10 block,
//! anchored at the old top-left corner and snapped to the grid.

use std::cmp::Ordering;
use std::collections::{HashMap, HashSet};

use indexmap::IndexMap;

use crate::{Graph, NodeId, Position};

/// Rendered card size in the GPUI canvas; Tidy lays out every node at this size.
pub const NODE_WIDTH: f64 = 170.;
pub const NODE_HEIGHT: f64 = 120.;

const WAYPOINT_WIDTH: f64 = 24.;
/// Alternating up/down passes of the x fit.
const SWEEPS: usize = 8;
/// Target width:height of the packed Graph components.
const ASPECT: f64 = 1.6;

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct LayoutOptions {
    pub grid_size: f64,
    /// Free space between neighbouring nodes.
    pub gap_x: f64,
    pub gap_y: f64,
    /// Free space between separate Graph components.
    pub component_gap: f64,
}

impl Default for LayoutOptions {
    fn default() -> Self {
        Self {
            grid_size: 20.,
            gap_x: 40.,
            gap_y: 40.,
            component_gap: 120.,
        }
    }
}

/// A node as Tidy sees it: current top-left corner and rendered size.
#[derive(Clone, Debug, PartialEq)]
pub struct LayoutNode {
    pub id: NodeId,
    pub x: f64,
    pub y: f64,
    pub width: f64,
    pub height: f64,
}

impl Graph {
    pub fn auto_layout(&mut self, options: LayoutOptions) {
        self.layout = self.compute_auto_layout(options);
    }

    /// Tidy every Graph component with Layered. Every GraphEdge stays in the
    /// Graph, including Synthesizer node edges; legacy cycles and edges with an
    /// unknown endpoint only stop constraining the layers.
    pub fn compute_auto_layout(&self, options: LayoutOptions) -> IndexMap<NodeId, Position> {
        let nodes: Vec<_> = self
            .nodes
            .keys()
            .map(|id| {
                let position = self.layout.get(id).copied().unwrap_or_default();
                LayoutNode {
                    id: id.clone(),
                    x: position.x,
                    y: position.y,
                    width: NODE_WIDTH,
                    height: NODE_HEIGHT,
                }
            })
            .collect();
        let edges: Vec<_> = self
            .edges
            .iter()
            .map(|edge| (edge.source.as_str(), edge.target.as_str()))
            .collect();
        let positions = compute_layout(&nodes, &edges, options);
        self.nodes
            .keys()
            .filter_map(|id| Some((id.clone(), *positions.get(id)?)))
            .collect()
    }
}

/// Grid-snapped top-left positions for `nodes`, anchored at their old
/// top-left corner. `edges` are `(source, target)` ids.
pub fn compute_layout(
    nodes: &[LayoutNode],
    edges: &[(&str, &str)],
    options: LayoutOptions,
) -> IndexMap<NodeId, Position> {
    if nodes.is_empty() {
        return IndexMap::new();
    }
    let s = Structure::new(nodes, edges);
    let layouts: Vec<_> = s
        .components
        .iter()
        .map(|component| layered_component(&s, component, options))
        .collect();
    let packed = pack_components(&layouts, nodes, options.component_gap);
    let anchor = Position {
        x: nodes.iter().map(|n| n.x).fold(f64::INFINITY, js_min),
        y: nodes.iter().map(|n| n.y).fold(f64::INFINITY, js_min),
    };
    to_top_left(&packed, nodes, anchor, options.grid_size)
}

/// Node centres by node index, in placement order. Layouts think in centres.
type Placement = Vec<(usize, Position)>;

/// The graph reduced to what Layered needs, over node indices. Edges that
/// would close a cycle are dropped, so `parents` and `children` form a DAG.
struct Structure<'a> {
    nodes: &'a [LayoutNode],
    /// Every parent, in edge order.
    parents: Vec<Vec<usize>>,
    /// Every child, in current left-to-right order.
    children: Vec<Vec<usize>>,
    /// Graph components in reading order, each in reading order.
    components: Vec<Vec<usize>>,
}

/// `a - b` as a JS comparator result: equal when the difference is zero.
fn cmp(a: f64, b: f64) -> Ordering {
    a.partial_cmp(&b).unwrap_or(Ordering::Equal)
}

fn js_min(a: f64, b: f64) -> f64 {
    if b < a {
        b
    } else {
        a
    }
}

fn js_max(a: f64, b: f64) -> f64 {
    if b > a {
        b
    } else {
        a
    }
}

impl<'a> Structure<'a> {
    fn new(nodes: &'a [LayoutNode], edges: &[(&str, &str)]) -> Self {
        let index: HashMap<&str, usize> = nodes
            .iter()
            .enumerate()
            .map(|(i, n)| (n.id.as_str(), i))
            .collect();
        let mut ordered: Vec<usize> = (0..nodes.len()).collect();
        ordered.sort_by(|&a, &b| {
            cmp(nodes[a].y, nodes[b].y).then_with(|| cmp(nodes[a].x, nodes[b].x))
        });

        // Same duplicate key as the TypeScript Tidy, NUL-joined ids included.
        let mut seen = HashSet::new();
        let valid: Vec<(usize, usize)> = edges
            .iter()
            .filter_map(|&(source, target)| {
                let key = format!("{source}\u{0}{target}");
                if source == target || seen.contains(&key) {
                    return None;
                }
                let edge = (*index.get(source)?, *index.get(target)?);
                seen.insert(key);
                Some(edge)
            })
            .collect();
        let kept = acyclic_edges(nodes.len(), &ordered, &valid);

        let mut parents = vec![Vec::new(); nodes.len()];
        let mut children = vec![Vec::new(); nodes.len()];
        for &(source, target) in &kept {
            parents[target].push(source);
            children[source].push(target);
        }
        for kids in &mut children {
            kids.sort_by(|&a, &b| by_position(nodes, a, b));
        }

        let root = find_components(nodes.len(), &ordered, &valid);
        let mut grouped: IndexMap<usize, Vec<usize>> = IndexMap::new();
        for &id in &ordered {
            grouped.entry(root[id]).or_default().push(id);
        }
        Self {
            nodes,
            parents,
            children,
            components: grouped.into_values().collect(),
        }
    }
}

/// Current left-to-right order: x, then y.
fn by_position(nodes: &[LayoutNode], a: usize, b: usize) -> Ordering {
    cmp(nodes[a].x, nodes[b].x).then_with(|| cmp(nodes[a].y, nodes[b].y))
}

/// Union-find over node indices; returns each node's component root.
fn find_components(len: usize, ordered: &[usize], edges: &[(usize, usize)]) -> Vec<usize> {
    let mut root: Vec<usize> = (0..len).collect();
    fn find(root: &mut [usize], id: usize) -> usize {
        let mut r = id;
        while root[r] != r {
            r = root[r];
        }
        root[id] = r;
        r
    }
    for &(source, target) in edges {
        let (s, t) = (find(&mut root, source), find(&mut root, target));
        root[s] = t;
    }
    let mut out = vec![0; len];
    for &id in ordered {
        out[id] = find(&mut root, id);
    }
    out
}

/// Drop back edges found by a DFS that starts from sources, so the rest is acyclic.
fn acyclic_edges(len: usize, ordered: &[usize], edges: &[(usize, usize)]) -> Vec<(usize, usize)> {
    #[derive(Clone, Copy, PartialEq)]
    enum State {
        New,
        Open,
        Done,
    }
    let mut out = vec![Vec::new(); len];
    let mut has_parent = vec![false; len];
    for &edge in edges {
        out[edge.0].push(edge);
        has_parent[edge.1] = true;
    }

    let mut state = vec![State::New; len];
    let mut kept = Vec::new();
    let mut visit = |start: usize, state: &mut Vec<State>| {
        if state[start] != State::New {
            return;
        }
        let mut stack = vec![(start, 0)];
        state[start] = State::Open;
        while let Some(frame) = stack.last_mut() {
            let (id, next) = *frame;
            if next >= out[id].len() {
                state[id] = State::Done;
                stack.pop();
                continue;
            }
            frame.1 += 1;
            let edge = out[id][next];
            match state[edge.1] {
                State::Open => continue, // back edge
                State::Done => kept.push(edge),
                State::New => {
                    kept.push(edge);
                    state[edge.1] = State::Open;
                    stack.push((edge.1, 0));
                }
            }
        }
    };
    for &id in ordered {
        if !has_parent[id] {
            visit(id, &mut state);
        }
    }
    for &id in ordered {
        visit(id, &mut state); // nodes only reachable through a cycle
    }
    kept
}

/// A layer entry: a real node, or a waypoint that reserves a long edge's lane.
struct Vertex {
    node: Option<usize>,
    layer: usize,
    width: f64,
    height: f64,
    up: Vec<usize>,
    down: Vec<usize>,
    x: f64,
    order: usize,
}

fn assign_layers(s: &Structure, component: &[usize]) -> HashMap<usize, i64> {
    let mut in_degree: HashMap<usize, usize> = component
        .iter()
        .map(|&id| (id, s.parents[id].len()))
        .collect();
    let mut layer: HashMap<usize, i64> = HashMap::new();
    let mut queue: Vec<usize> = component
        .iter()
        .copied()
        .filter(|id| in_degree[id] == 0)
        .collect();
    for &id in &queue {
        layer.insert(id, 0);
    }

    let mut i = 0;
    while i < queue.len() {
        let id = queue[i];
        for &kid in &s.children[id] {
            let next = layer.get(&kid).copied().unwrap_or(0).max(layer[&id] + 1);
            layer.insert(kid, next);
            let degree = in_degree.get_mut(&kid).expect("kid in component");
            *degree -= 1;
            if *degree == 0 {
                queue.push(kid);
            }
        }
        i += 1;
    }

    // Pull nodes down to sit just above their nearest child. Only nodes that
    // feed a deeper merge move, so a second tree joins where it merges.
    for &id in queue.iter().rev() {
        let kids = &s.children[id];
        if let Some(nearest) = kids.iter().map(|k| layer[k]).min() {
            layer.insert(id, nearest - 1);
        }
    }
    layer
}

/// Real vertices plus waypoints on long edges, and the layers in preorder.
fn build_layers(s: &Structure, component: &[usize]) -> (Vec<Vertex>, Vec<Vec<usize>>) {
    let layer_of = assign_layers(s, component);
    let min_layer = component.iter().map(|id| layer_of[id]).min().unwrap_or(0);
    let mut vertices: Vec<Vertex> = Vec::new();
    let mut vertex_of: HashMap<usize, usize> = HashMap::new();
    for &id in component {
        let n = &s.nodes[id];
        vertex_of.insert(id, vertices.len());
        vertices.push(Vertex {
            node: Some(id),
            layer: (layer_of[&id] - min_layer) as usize,
            width: n.width,
            height: n.height,
            up: Vec::new(),
            down: Vec::new(),
            x: 0.,
            order: 0,
        });
    }

    for &id in component {
        for &kid in &s.children[id] {
            let mut from = vertex_of[&id];
            let to = vertex_of[&kid];
            for layer in vertices[from].layer + 1..vertices[to].layer {
                let waypoint = vertices.len();
                vertices.push(Vertex {
                    node: None,
                    layer,
                    width: WAYPOINT_WIDTH,
                    height: 0.,
                    up: vec![from],
                    down: Vec::new(),
                    x: 0.,
                    order: 0,
                });
                vertices[from].down.push(waypoint);
                from = waypoint;
            }
            vertices[from].down.push(to);
            vertices[to].up.push(from);
        }
    }

    let depth = vertices.iter().map(|v| v.layer).max().unwrap_or(0) + 1;
    let mut layers = vec![Vec::new(); depth];
    for v in preorder(s, component, &vertex_of, &vertices) {
        layers[vertices[v].layer].push(v);
    }
    (vertices, layers)
}

/// Depth-first order from the sources; for a pure tree this already has no crossings.
fn preorder(
    s: &Structure,
    component: &[usize],
    vertex_of: &HashMap<usize, usize>,
    vertices: &[Vertex],
) -> Vec<usize> {
    let mut sources: Vec<usize> = component
        .iter()
        .copied()
        .filter(|&id| s.parents[id].is_empty())
        .collect();
    sources.sort_by(|&a, &b| by_position(s.nodes, a, b));
    let mut stack: Vec<usize> = sources.iter().rev().map(|id| vertex_of[id]).collect();
    let mut seen = vec![false; vertices.len()];
    let mut out = Vec::new();
    while let Some(v) = stack.pop() {
        if seen[v] {
            continue;
        }
        seen[v] = true;
        out.push(v);
        stack.extend(vertices[v].down.iter().rev());
    }
    out
}

/// Crossings among the edges of u and v toward one neighbouring layer, with u left of v.
fn pair_crossings(vertices: &[Vertex], u_ends: &[usize], v_ends: &[usize]) -> usize {
    u_ends
        .iter()
        .map(|&a| {
            v_ends
                .iter()
                .filter(|&&b| vertices[a].order > vertices[b].order)
                .count()
        })
        .sum()
}

/// Swap neighbours in a layer only where the swap removes crossings, until no
/// such swap is left. Every swap lowers the total, so this ends.
fn reduce_crossings(vertices: &mut [Vertex], layers: &mut [Vec<usize>]) {
    for layer in layers.iter() {
        for (i, &v) in layer.iter().enumerate() {
            vertices[v].order = i;
        }
    }
    let mut swapped = true;
    while swapped {
        swapped = false;
        for layer in layers.iter_mut() {
            swapped = swap_pass(vertices, layer) || swapped;
        }
    }
}

/// One left-to-right pass over a layer. Returns true when anything swapped.
fn swap_pass(vertices: &mut [Vertex], layer: &mut [usize]) -> bool {
    let mut swapped = false;
    for i in 0..layer.len().saturating_sub(1) {
        let (u, v) = (layer[i], layer[i + 1]);
        let kept = pair_crossings(vertices, &vertices[u].up, &vertices[v].up)
            + pair_crossings(vertices, &vertices[u].down, &vertices[v].down);
        let flipped = pair_crossings(vertices, &vertices[v].up, &vertices[u].up)
            + pair_crossings(vertices, &vertices[v].down, &vertices[u].down);
        if flipped >= kept {
            continue;
        }
        layer.swap(i, i + 1);
        vertices[u].order = i + 1;
        vertices[v].order = i;
        swapped = true;
    }
    swapped
}

/// Weighted least-squares placement of an ordered row: minimise
/// sum(w * (x - target)^2) subject to x[i+1] - x[i] >= gap[i].
/// Substituting y = x - offset turns this into isotonic regression (PAV).
fn fit_row(targets: &[f64], weights: &[f64], gaps: &[f64]) -> Vec<f64> {
    let mut offset = vec![0.];
    for i in 1..targets.len() {
        offset.push(offset[i - 1] + gaps[i - 1]);
    }

    struct Block {
        w: f64,
        sum: f64,
        size: usize,
    }
    let mut blocks: Vec<Block> = Vec::new();
    for (i, &t) in targets.iter().enumerate() {
        blocks.push(Block {
            w: weights[i],
            sum: weights[i] * (t - offset[i]),
            size: 1,
        });
        while blocks.len() > 1 {
            let b = &blocks[blocks.len() - 1];
            let a = &blocks[blocks.len() - 2];
            if a.sum / a.w <= b.sum / b.w {
                break;
            }
            let merged = Block {
                w: a.w + b.w,
                sum: a.sum + b.sum,
                size: a.size + b.size,
            };
            blocks.truncate(blocks.len() - 2);
            blocks.push(merged);
        }
    }

    let mut out = Vec::with_capacity(targets.len());
    for b in &blocks {
        for _ in 0..b.size {
            out.push(b.sum / b.w + offset[out.len()]);
        }
    }
    out
}

fn neighbours(v: &Vertex, up: bool) -> &[usize] {
    if up {
        &v.up
    } else {
        &v.down
    }
}

fn align_layer(vertices: &mut [Vertex], layer: &[usize], up: bool, gap_x: f64) {
    let targets: Vec<f64> = layer
        .iter()
        .map(|&v| {
            let ns = neighbours(&vertices[v], up);
            if ns.is_empty() {
                vertices[v].x
            } else {
                ns.iter().fold(0., |sum, &n| sum + vertices[n].x) / ns.len() as f64
            }
        })
        .collect();
    let weights: Vec<f64> = layer
        .iter()
        .map(|&v| neighbours(&vertices[v], up).len().max(1) as f64)
        .collect();
    let gaps: Vec<f64> = layer
        .windows(2)
        .map(|pair| (vertices[pair[0]].width + vertices[pair[1]].width) / 2. + gap_x)
        .collect();
    for (&v, x) in layer.iter().zip(fit_row(&targets, &weights, &gaps)) {
        vertices[v].x = x;
    }
}

fn assign_x(vertices: &mut [Vertex], layers: &[Vec<usize>], gap_x: f64) {
    for layer in layers {
        let mut x = 0.;
        for (i, &v) in layer.iter().enumerate() {
            if i > 0 {
                x += (vertices[layer[i - 1]].width + vertices[v].width) / 2. + gap_x;
            }
            vertices[v].x = x;
        }
    }
    for _ in 0..SWEEPS {
        for layer in layers[..layers.len().saturating_sub(1)].iter().rev() {
            align_layer(vertices, layer, false, gap_x);
        }
        for layer in layers.iter().skip(1) {
            align_layer(vertices, layer, true, gap_x);
        }
    }
}

/// Node centres of one Graph component, with layer 0 starting at y = 0.
fn layered_component(s: &Structure, component: &[usize], options: LayoutOptions) -> Placement {
    let (mut vertices, mut layers) = build_layers(s, component);
    reduce_crossings(&mut vertices, &mut layers);
    assign_x(&mut vertices, &layers, options.gap_x);

    let mut placement = Vec::new();
    let mut top = 0.;
    for layer in &layers {
        let height = layer.iter().map(|&v| vertices[v].height).fold(0., js_max);
        for &v in layer {
            if let Some(node) = vertices[v].node {
                placement.push((
                    node,
                    Position {
                        x: vertices[v].x,
                        y: top + height / 2.,
                    },
                ));
            }
        }
        top += height + options.gap_y;
    }
    placement
}

struct Bounds {
    min_x: f64,
    min_y: f64,
    max_x: f64,
    max_y: f64,
}

fn bounds(placement: &Placement, nodes: &[LayoutNode]) -> Bounds {
    let mut b = Bounds {
        min_x: f64::INFINITY,
        min_y: f64::INFINITY,
        max_x: f64::NEG_INFINITY,
        max_y: f64::NEG_INFINITY,
    };
    for &(id, p) in placement {
        let hw = nodes[id].width / 2.;
        let hh = nodes[id].height / 2.;
        b.min_x = js_min(b.min_x, p.x - hw);
        b.min_y = js_min(b.min_y, p.y - hh);
        b.max_x = js_max(b.max_x, p.x + hw);
        b.max_y = js_max(b.max_y, p.y + hh);
    }
    b
}

/// Arrange independent components in shelves so the whole canvas comes out
/// roughly `ASPECT` wide per unit tall instead of one long horizontal strip.
/// Taller components go first; each keeps its own internal layout.
fn pack_components(layouts: &[Placement], nodes: &[LayoutNode], gap: f64) -> Placement {
    struct Box<'a> {
        placement: &'a Placement,
        index: usize,
        b: Bounds,
        w: f64,
        h: f64,
    }
    let mut boxes: Vec<Box> = layouts
        .iter()
        .enumerate()
        .map(|(index, placement)| {
            let b = bounds(placement, nodes);
            let (w, h) = (b.max_x - b.min_x, b.max_y - b.min_y);
            Box {
                placement,
                index,
                b,
                w,
                h,
            }
        })
        .collect();
    let area = boxes
        .iter()
        .fold(0., |sum, b| sum + (b.w + gap) * (b.h + gap));
    let widest = boxes.iter().map(|b| b.w).fold(0., js_max);
    let row_width = js_max(widest, (area * ASPECT).sqrt());

    // Tallest first packs shelves tightly; ties keep reading order.
    boxes.sort_by(|a, b| {
        cmp(b.h, a.h)
            .then_with(|| cmp(b.w, a.w))
            .then_with(|| a.index.cmp(&b.index))
    });

    let mut out = Vec::new();
    let (mut cursor_x, mut cursor_y, mut shelf_height) = (0., 0., 0.);
    for b in &boxes {
        if cursor_x > 0. && cursor_x + b.w > row_width {
            cursor_x = 0.;
            cursor_y += shelf_height + gap;
            shelf_height = 0.;
        }
        for &(id, p) in b.placement {
            out.push((
                id,
                Position {
                    x: p.x - b.b.min_x + cursor_x,
                    y: p.y - b.b.min_y + cursor_y,
                },
            ));
        }
        cursor_x += b.w + gap;
        shelf_height = js_max(shelf_height, b.h);
    }
    out
}

/// `Math.round(value / grid) * grid`, with JavaScript's ties toward +∞.
fn snap(value: f64, grid: f64) -> f64 {
    if grid == 0. {
        return value;
    }
    let scaled = value / grid;
    let floor = scaled.floor();
    let rounded = if scaled - floor >= 0.5 {
        floor + 1.
    } else {
        floor
    };
    rounded * grid
}

/// Convert centres to top-left corners, anchored so the new layout starts at
/// `anchor` (the old top-left), and snapped to the grid.
fn to_top_left(
    placement: &Placement,
    nodes: &[LayoutNode],
    anchor: Position,
    grid: f64,
) -> IndexMap<NodeId, Position> {
    let b = bounds(placement, nodes);
    placement
        .iter()
        .map(|&(id, p)| {
            let n = &nodes[id];
            (
                n.id.clone(),
                Position {
                    x: snap(anchor.x + p.x - b.min_x - n.width / 2., grid),
                    y: snap(anchor.y + p.y - b.min_y - n.height / 2., grid),
                },
            )
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fit_row_keeps_targets_that_already_respect_the_gaps() {
        assert_eq!(
            fit_row(&[0., 100., 300.], &[1., 1., 1.], &[50., 50.]),
            [0., 100., 300.]
        );
    }

    #[test]
    fn fit_row_spreads_clashing_targets_around_their_weighted_mean() {
        assert_eq!(fit_row(&[100., 100.], &[1., 1.], &[200.]), [0., 200.]);
        assert_eq!(fit_row(&[0., 0.], &[3., 1.], &[100.]), [-25., 75.]);
    }

    #[test]
    fn snap_rounds_ties_toward_positive_infinity_like_javascript() {
        assert_eq!(snap(10., 20.), 20.);
        assert_eq!(snap(-10., 20.), 0.);
        assert_eq!(snap(-30., 20.), -20.);
        assert_eq!(snap(13.7, 0.), 13.7);
    }
}
