//! Tidy (Layered) behaviour, and exact parity with the TypeScript Tidy through
//! the shared fixture in `fixtures/layout/`.
//!
//! The TypeScript Tidy and its fixture generator are gone, so the `expected`
//! positions in `fixtures/layout/` are golden values. To update a golden after
//! an intended layout change, run
//! `cargo test -p thoughttree-gpui-model --test layout`. Each failure names the
//! fixture file and the node, with the new position (`left`) and the golden
//! (`right`). Make sure that the new position is correct, write it into the
//! `expected` entry of that node, and run the test again until it passes.

use indexmap::IndexMap;
use serde_json::Value;
use thoughttree_gpui_model::*;

fn node(id: &str, x: f64, y: f64) -> LayoutNode {
    LayoutNode {
        id: id.into(),
        x,
        y,
        width: NODE_WIDTH,
        height: NODE_HEIGHT,
    }
}

fn chain(prefix: &str, length: usize, x: f64) -> (Vec<LayoutNode>, Vec<(String, String)>) {
    let nodes: Vec<_> = (0..length)
        .map(|i| node(&format!("{prefix}{i}"), x, i as f64))
        .collect();
    let edges = nodes
        .windows(2)
        .map(|pair| (pair[0].id.clone(), pair[1].id.clone()))
        .collect();
    (nodes, edges)
}

fn layout(
    nodes: &[LayoutNode],
    edges: &[(String, String)],
    options: LayoutOptions,
) -> IndexMap<NodeId, Position> {
    let edges: Vec<_> = edges
        .iter()
        .map(|(s, t)| (s.as_str(), t.as_str()))
        .collect();
    compute_layout(nodes, &edges, options)
}

fn edge(source: &str, target: &str) -> (String, String) {
    (source.into(), target.into())
}

const RAW: LayoutOptions = LayoutOptions {
    grid_size: 0.,
    gap_x: 40.,
    gap_y: 40.,
    component_gap: 120.,
};

#[test]
fn layout_matches_every_typescript_parity_fixture_exactly() {
    let dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../../fixtures/layout");
    let mut cases = 0;
    for entry in std::fs::read_dir(&dir).unwrap() {
        let path = entry.unwrap().path();
        if path.extension().and_then(|e| e.to_str()) != Some("json") {
            continue;
        }
        let fixture: Value =
            serde_json::from_str(&std::fs::read_to_string(&path).unwrap()).unwrap();
        let nodes: Vec<_> = fixture["nodes"]
            .as_array()
            .unwrap()
            .iter()
            .map(|n| LayoutNode {
                id: n["id"].as_str().unwrap().into(),
                x: n["x"].as_f64().unwrap(),
                y: n["y"].as_f64().unwrap(),
                width: n["width"].as_f64().unwrap(),
                height: n["height"].as_f64().unwrap(),
            })
            .collect();
        let edges: Vec<_> = fixture["edges"]
            .as_array()
            .unwrap()
            .iter()
            .map(|e| edge(e["source"].as_str().unwrap(), e["target"].as_str().unwrap()))
            .collect();
        let options = &fixture["options"];
        let options = LayoutOptions {
            grid_size: options["gridSize"].as_f64().unwrap(),
            gap_x: options["gapX"].as_f64().unwrap(),
            gap_y: options["gapY"].as_f64().unwrap(),
            component_gap: options["componentGap"].as_f64().unwrap(),
        };
        let actual = layout(&nodes, &edges, options);
        let expected = fixture["expected"].as_object().unwrap();
        assert_eq!(actual.len(), expected.len(), "{}", path.display());
        for (id, position) in expected {
            let expected = Position {
                x: position["x"].as_f64().unwrap(),
                y: position["y"].as_f64().unwrap(),
            };
            assert_eq!(
                actual[id.as_str()],
                expected,
                "{} node {id}",
                path.display()
            );
        }
        cases += 1;
    }
    assert!(
        cases >= 5,
        "found {cases} fixture cases in {}",
        dir.display()
    );
}

#[test]
fn layout_puts_a_synthesizer_node_below_and_between_its_parents() {
    let (mut nodes, mut edges) = chain("l", 4, 0.);
    let (right, right_edges) = chain("r", 2, 500.);
    nodes.extend(right);
    nodes.push(node("m", 0., 0.));
    edges.extend(right_edges);
    edges.extend([edge("l3", "m"), edge("r1", "m")]);
    let pos = layout(&nodes, &edges, RAW);

    let m = pos["m"];
    assert!(m.y > pos["l3"].y && m.y > pos["r1"].y);
    let (lo, hi) = (pos["l3"].x.min(pos["r1"].x), pos["l3"].x.max(pos["r1"].x));
    assert!(lo <= m.x && m.x <= hi);
    // The short tree is pulled down to join where it merges.
    assert_eq!(pos["r1"].y, pos["l3"].y);
}

#[test]
fn layout_keeps_graph_components_joined_by_a_synthesizer_node_together() {
    // Two linked trees plus ten unrelated ones that could separate them.
    let (mut nodes, mut edges) = chain("a", 3, 0.);
    for i in 0..10 {
        let (other, other_edges) = chain(&format!("o{i}x"), 3, 0.);
        nodes.extend(other);
        edges.extend(other_edges);
    }
    let (b, b_edges) = chain("b", 3, 0.);
    nodes.extend(b);
    nodes.push(node("m", 0., 0.));
    edges.extend(b_edges);
    edges.extend([edge("a2", "m"), edge("b2", "m")]);
    let pos = layout(&nodes, &edges, LayoutOptions::default());
    assert_eq!(pos["a2"].y, pos["b2"].y);
    assert!((pos["a2"].x - pos["b2"].x).abs() <= NODE_WIDTH + 60.);
}

#[test]
fn layout_packs_separate_graph_components_into_a_block() {
    let (mut nodes, mut edges) = (Vec::new(), Vec::new());
    for i in 0..16 {
        let (tree, tree_edges) = chain(&format!("t{i}x"), 3, i as f64 * 210.);
        nodes.extend(tree);
        edges.extend(tree_edges);
    }
    let pos = layout(&nodes, &edges, LayoutOptions::default());
    let xs = pos.values().map(|p| p.x);
    let ys: Vec<_> = pos.values().map(|p| p.y).collect();
    let width = xs.clone().fold(f64::MIN, f64::max) - xs.fold(f64::MAX, f64::min) + NODE_WIDTH;
    let top = ys.iter().copied().fold(f64::MAX, f64::min);
    let height = ys.iter().copied().fold(f64::MIN, f64::max) - top + NODE_HEIGHT;
    assert!(width / height < 3., "{width} x {height}");
    let rows: std::collections::BTreeSet<_> = ys.iter().map(|y| *y as i64).collect();
    assert!(rows.len() > 3);
}

#[test]
fn layout_keeps_sibling_order_unless_a_swap_removes_a_crossing() {
    // Siblings of p keep their current x order: b, c, a.
    let nodes = [
        node("p", 0., 0.),
        node("a", 20., 1.),
        node("b", -50., 1.),
        node("c", 10., 1.),
    ];
    let edges = [edge("p", "a"), edge("p", "b"), edge("p", "c")];
    let pos = layout(&nodes, &edges, RAW);
    assert!(pos["b"].x < pos["c"].x && pos["c"].x < pos["a"].x);
    assert!((pos["p"].x - pos["c"].x).abs() < 1e-9);

    // q's child m also has parent a, so keeping b left of a would cross.
    let nodes = [
        node("q", 0., 160.),
        node("p", 300., 0.),
        node("b", 200., 160.),
        node("a", 400., 160.),
        node("bc", 200., 320.),
        node("m", 100., 320.),
    ];
    let edges = [
        edge("p", "b"),
        edge("p", "a"),
        edge("b", "bc"),
        edge("q", "m"),
        edge("a", "m"),
    ];
    let pos = layout(&nodes, &edges, RAW);
    assert!(pos["a"].x < pos["b"].x);
    assert!(pos["q"].x < pos["a"].x);
}

#[test]
fn layout_does_not_swap_nodes_when_the_swap_removes_no_crossing() {
    let mut nodes: Vec<_> = (0..4)
        .map(|i| node(&format!("p{i}"), i as f64 * 200., 0.))
        .collect();
    nodes.extend(
        ["k0", "k2", "k4"]
            .iter()
            .enumerate()
            .map(|(i, id)| node(id, i as f64 * 300., 160.)),
    );
    let mut edges = Vec::new();
    for (kid, parents) in [
        ("k0", ["p0", "p2", "p3"].as_slice()),
        ("k2", &["p0", "p1", "p2"]),
        ("k4", &["p0", "p3"]),
    ] {
        edges.extend(parents.iter().map(|p| edge(p, kid)));
    }
    let pos = layout(&nodes, &edges, RAW);
    assert!(pos["k0"].x < pos["k4"].x);
}

#[test]
fn layout_is_deterministic_on_the_grid_and_keeps_every_node_and_edge() {
    let mut graph = Graph::default();
    for (i, id) in ["root", "left", "right", "merge", "c1", "c2", "solo"]
        .iter()
        .enumerate()
    {
        graph.add_node(
            GraphNode::user(*id, *id, 1.),
            Position {
                x: 37. * i as f64,
                y: 13. * i as f64,
            },
        );
    }
    for (source, target) in [
        ("root", "left"),
        ("root", "right"),
        ("left", "merge"),
        ("right", "merge"),
        ("c1", "c2"),
    ] {
        graph.add_edge(source, target).unwrap();
    }
    // A legacy cycle and an edge between nodes that no longer exist.
    for (id, source, target) in [("legacy-cycle", "c2", "c1"), ("missing", "unknown", "gone")] {
        graph.edges.push(GraphEdge {
            id: id.into(),
            source: source.into(),
            target: target.into(),
        });
    }
    let (nodes, edges) = (graph.nodes.clone(), graph.edges.clone());

    let first = graph.compute_auto_layout(LayoutOptions::default());
    assert_eq!(first, graph.compute_auto_layout(LayoutOptions::default()));
    graph.auto_layout(LayoutOptions::default());
    assert_eq!(graph.nodes, nodes);
    assert_eq!(graph.edges, edges);
    assert_eq!(
        graph.layout.keys().collect::<Vec<_>>(),
        nodes.keys().collect::<Vec<_>>()
    );
    assert!(graph
        .layout
        .values()
        .all(|p| p.x % 20. == 0. && p.y % 20. == 0.));
    assert_ne!(graph.layout["c1"].y, graph.layout["c2"].y);
    assert!(graph.layout["merge"].y > graph.layout["left"].y);

    let reopened = Project::from_json(&Project::from(graph.clone()).to_json().unwrap()).unwrap();
    assert_eq!(reopened.graph.edges, edges);
    assert_eq!(reopened.graph.layout, graph.layout);
}

#[test]
fn layout_drops_duplicate_edges_by_the_same_key_as_typescript() {
    // "a" -> "b\0c" and "a\0b" -> "c" share the TypeScript key "a\0b\0c", so
    // only the first counts; c stays a separate root, as in the TypeScript Tidy.
    let nodes = [
        node("a", 0., 0.),
        node("b\0c", 0., 0.),
        node("a\0b", 0., 0.),
        node("c", 0., 0.),
    ];
    let edges = [edge("a", "b\0c"), edge("a\0b", "c")];
    let pos = layout(&nodes, &edges, LayoutOptions::default());
    // Positions from the TypeScript Tidy for the same input.
    let expected = [
        ("a", 0., 0.),
        ("b\0c", 0., 160.),
        ("a\0b", 300., 0.),
        ("c", 0., 400.),
    ];
    for (id, x, y) in expected {
        assert_eq!(pos[id], Position { x, y }, "{id:?}");
    }
}

#[test]
fn layout_of_an_empty_graph_is_empty() {
    assert!(compute_layout(&[], &[], LayoutOptions::default()).is_empty());
}
