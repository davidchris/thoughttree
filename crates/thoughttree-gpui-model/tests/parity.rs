use serde_json::{json, Value};
use thoughttree_gpui_model::*;

fn text_graph() -> Graph {
    let mut graph = Graph::default();
    for (id, role) in [
        ("root", Role::User),
        ("left", Role::Assistant),
        ("right", Role::Assistant),
        ("merge", Role::User),
    ] {
        let node = match role {
            Role::User => GraphNode::user(id, id, 1.0),
            _ => GraphNode::assistant(id, id, 1.0),
        };
        graph.add_node(node, Position::default());
    }
    for (source, target) in [
        ("root", "left"),
        ("root", "right"),
        ("left", "merge"),
        ("right", "merge"),
    ] {
        graph.add_edge(source, target).unwrap();
    }
    graph
}

#[test]
fn content_changes_invalidate_summaries_even_when_the_clock_has_not_advanced() {
    let mut graph = text_graph();
    let node = &mut graph.nodes["root"];
    node.content_updated_at = Some(100.);
    node.summary = Some("Previous summary".into());
    node.summary_timestamp = Some(100.);
    let mut editor = Editor::new(Project::from(graph));
    editor
        .edit(|project| project.graph.set_content("root", "root".into(), 200.))
        .unwrap();
    assert!(!editor.is_dirty());
    assert_eq!(
        editor.project.graph.nodes["root"].content_updated_at,
        Some(100.)
    );
    assert_eq!(
        editor.project.graph.nodes["root"].summary_timestamp,
        Some(100.)
    );
    editor
        .edit(|project| project.graph.set_content("root", "changed".into(), 100.))
        .unwrap();
    let node = &editor.project.graph.nodes["root"];
    assert!(editor.is_dirty());
    assert_eq!(node.summary.as_deref(), Some("Previous summary"));
    assert_eq!(node.summary_timestamp, None);
    assert_eq!(node.content_updated_at, Some(100.));

    editor.start_turn("left", "turn").unwrap();
    editor.project.graph.nodes["left"].summary = Some("Stale streamed summary".into());
    editor.project.graph.nodes["left"].summary_timestamp = Some(f64::MAX);
    let revision = editor.edit_revision;
    assert!(editor.append_turn("left", "turn", ""));
    assert_eq!(editor.edit_revision, revision);
    assert_eq!(
        editor.project.graph.nodes["left"].summary_timestamp,
        Some(f64::MAX)
    );
    assert!(editor.append_turn("left", "turn", " next chunk"));
    assert_eq!(editor.project.graph.nodes["left"].summary_timestamp, None);
    editor.project.graph.nodes["left"].summary_timestamp = Some(f64::MAX);
    assert!(editor.fail_turn("left", "turn", "provider stopped"));
    assert_eq!(editor.project.graph.nodes["left"].summary_timestamp, None);
    assert_eq!(
        editor.project.graph.nodes["left"].summary.as_deref(),
        Some("Stale streamed summary")
    );
}

fn file_data(path: &str) -> FileData {
    FileData {
        path: path.into(),
        name: path.rsplit('/').next().unwrap().into(),
        mime_type: "text/markdown".into(),
        size: 3,
        seen_mtime: 2,
        seen_size: 3,
    }
}

#[test]
fn conversation_context_matches_the_existing_typescript_frontend() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/typescript-parity.json")).unwrap();
    for fixture in fixtures["cases"].as_array().unwrap() {
        let project = Project::from_json(&fixture["project"].to_string()).unwrap();
        let target = fixture["target"].as_str().unwrap();
        assert_eq!(
            serde_json::to_value(project.graph.conversation_path_ids(target)).unwrap(),
            fixture["pathIds"],
            "{}",
            fixture["name"]
        );
        assert_eq!(
            serde_json::to_value(project.graph.conversation_path(target)).unwrap(),
            fixture["messages"],
            "{}",
            fixture["name"]
        );
    }
}

#[test]
fn provenance_allowlist_matches_typescript_including_known_loss() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/typescript-parity.json")).unwrap();
    for fixture in fixtures["provenance"].as_array().unwrap() {
        let expected: TurnProvenance = serde_json::from_value(fixture["expected"].clone()).unwrap();
        assert_eq!(normalize_provenance(&fixture["input"]), Some(expected));
    }
}

#[test]
fn cycle_and_file_target_rejection_leave_the_graph_unchanged() {
    let mut graph = text_graph();
    let before = graph.clone();
    assert_eq!(graph.add_edge("merge", "root"), Err(GraphError::Cycle));
    assert_eq!(graph.add_edge("root", "root"), Err(GraphError::Cycle));
    assert_eq!(
        graph.add_edge("missing", "root"),
        Err(GraphError::MissingNode("missing".into()))
    );
    assert!(!graph.add_edge("root", "left").unwrap());
    assert_eq!(graph, before);
    graph.add_node(
        GraphNode::file("file", file_data("file.md"), 1.0),
        Position::default(),
    );
    assert_eq!(graph.add_edge("root", "file"), Err(GraphError::FileTarget));
    assert_eq!(
        graph.set_content("file", "secret bytes".into(), 1.0),
        Err(GraphError::FileContent)
    );
}

#[test]
fn export_includes_every_selected_branch_and_disconnected_node() {
    let mut graph = text_graph();
    graph.add_node(
        GraphNode::user("other", "disconnected", 1.0),
        Position::default(),
    );
    let ids = graph.nodes.keys().cloned().collect::<Vec<_>>();
    let exported = graph.export_markdown(&ids);
    for content in ["root", "left", "right", "merge", "disconnected"] {
        assert!(exported.contains(content));
    }
    assert_eq!(exported.matches("## ").count(), 5);
    let selected = graph.selected_subgraph(&["left".into(), "right".into(), "merge".into()]);
    assert_eq!(selected.nodes.len(), 3);
    assert_eq!(selected.edges.len(), 2);
}

#[test]
fn legacy_v1_and_v2_migration_preserves_layout_and_assumes_claude() {
    for version in [1, 2] {
        let project = Project::from_json(&json!({"version": version, "nodes": [{"id":"a","position":{"x":50,"y":75}}], "edges":[], "nodeData":{"a":{"id":"a","role":"assistant","content":"hello","timestamp":10}}, "projectModelPreferences":{"claude-code":"opus","codex":null,"future-provider":"future-model"}}).to_string()).unwrap();
        let node = &project.graph.nodes["a"];
        assert_eq!(
            node.assistant_data().unwrap().provider,
            Some(GraphProvider::ClaudeCode)
        );
        assert_eq!(node.content_updated_at, Some(10.0));
        assert_eq!(project.graph.layout["a"], Position { x: 50.0, y: 75.0 });
        let settings = project.project_model_preferences.as_ref().unwrap();
        assert_eq!(settings.len(), 2);
        assert_eq!(settings["future-provider"], "future-model");
        let saved: Value = serde_json::from_str(&project.to_json().unwrap()).unwrap();
        assert_eq!(saved["version"], 5);
        assert_eq!(saved["graph"]["version"], 5);
    }
}

#[test]
fn current_versions_round_trip_settings_provenance_images_and_files() {
    let mut graph = text_graph();
    let NodeKind::User(user) = &mut graph.nodes["root"].kind else {
        unreachable!()
    };
    user.images.push(ImageAttachment {
        data: "YWJj".into(),
        mime_type: "image/png".into(),
        name: Some("a.png".into()),
    });
    graph.add_node(
        GraphNode::file("file", file_data("notes/doc.md"), 1.0),
        Position { x: -45.0, y: 12.0 },
    );
    graph.add_edge("file", "merge").unwrap();
    let mut project = Project::from(graph);
    project.project_effort_preferences = Some([("codex".into(), ReasoningEffort::XHigh)].into());
    project.project_model_preferences = Some([("codex".into(), "gpt-model".into())].into());
    for version in [3, 4, 5] {
        let mut value: Value = serde_json::from_str(&project.to_json().unwrap()).unwrap();
        value["version"] = json!(version);
        assert_eq!(Project::from_json(&value.to_string()).unwrap(), project);
    }
}

#[test]
fn persistence_never_retains_file_bytes_raw_tool_data_or_user_provenance() {
    let value = json!({"version":5,"graph":{"version":5,"nodes":[
        {"id":"u","role":"user","content":"user text","timestamp":1,"provenance":{"completeness":"complete","activity":[{"raw":"secret"}]}},
        {"id":"a","role":"assistant","content":"answer","timestamp":2,"provenance":{"completeness":"complete","references":[{"type":"file","scope":"external","path":"/Users/alice/key","displayName":"/Users/alice/key","relations":["read"]}],"activity":[{"type":"tool","kind":"execute","title":"cat /Users/alice/key","status":"completed","rawInput":"secret","rawOutput":"password"}]}},
        {"id":"f","role":"file","content":"forbidden file bytes","timestamp":1,"path":"a.md","name":"a.md","mimeType":"text/markdown","size":3,"seenMtime":2,"seenSize":3},
        {"id":"invalid","role":"file","content":"","timestamp":1,"path":"../escape","name":"escape","mimeType":"text/plain","size":3,"seenMtime":2,"seenSize":3}
    ],"edges":[{"id":"bad","source":"u","target":"f"}],"layout":[]}});
    let project = Project::from_json(&value.to_string()).unwrap();
    assert!(!project.graph.nodes.contains_key("invalid"));
    assert!(project.graph.edges.is_empty());
    let serialized = project.to_json().unwrap();
    for forbidden in [
        "rawInput",
        "rawOutput",
        "password",
        "/Users/alice",
        "forbidden file bytes",
    ] {
        assert!(!serialized.contains(forbidden), "{forbidden}");
    }
    let saved: Value = serde_json::from_str(&serialized).unwrap();
    assert!(saved["graph"]["nodes"][0].get("provenance").is_none());
    assert_eq!(
        saved["graph"]["nodes"][1]["provenance"]["completeness"],
        "partial"
    );
    assert_eq!(saved["graph"]["nodes"][2]["content"], "");
}

#[test]
fn malformed_project_and_newer_version_fail_without_mutating_editor() {
    let editor = Editor::new(Project::from(text_graph()));
    assert!(Project::from_json("not json").is_err());
    assert!(matches!(
        Project::from_json("{\"version\":6}"),
        Err(ProjectError::UnsupportedVersion(_))
    ));
    assert!(Project::from_json("{\"version\":5,\"graph\":{}}").is_err());
    assert_eq!(editor.project.graph.nodes.len(), 4);
}

#[test]
fn invalid_file_metadata_is_dropped() {
    for (field, value) in [
        ("path", json!("C:\\secret")),
        ("path", json!("file:secret")),
        ("name", json!("/secret")),
        ("size", json!(-1)),
        ("size", json!(1.5)),
        ("seenMtime", json!(9_007_199_254_740_992_u64)),
    ] {
        let mut node = serde_json::to_value(GraphNode::file("f", file_data("a.md"), 0.0)).unwrap();
        node[field] = value;
        assert!(normalize_node(&node).is_none(), "{node}");
    }
}

#[test]
fn save_completion_cannot_clear_newer_edits_or_a_different_project() {
    let mut editor = Editor::default();
    editor
        .edit(|project| {
            project
                .graph
                .add_node(GraphNode::user("a", "hello", 1.0), Position::default());
            Ok(())
        })
        .unwrap();
    let captured = editor.edit_revision;
    editor
        .edit(|project| project.graph.set_content("a", "newer".into(), 2.0))
        .unwrap();
    editor.mark_saved(captured);
    assert!(editor.is_dirty());
    editor.replace_project(Project::default()).unwrap();
    editor.touch();
    editor.mark_saved(captured);
    assert!(editor.is_dirty());
    editor.mark_saved(editor.edit_revision);
    assert!(!editor.is_dirty());
}

#[test]
fn failed_edits_rollback_and_undo_redo_preserve_selection() {
    let mut editor = Editor::new(Project::from(text_graph()));
    editor.selection.insert("left".into());
    let before = editor.project.clone();
    let error = editor.edit(|project| {
        project.graph.remove_node("left");
        project.graph.add_edge("merge", "root")
    });
    assert!(error.is_err());
    assert_eq!(editor.project, before);
    assert!(!editor.is_dirty());
    editor
        .edit(|project| {
            project.graph.remove_node("left");
            Ok(())
        })
        .unwrap();
    assert!(editor.selection.is_empty());
    assert!(editor.undo());
    assert!(editor.selection.contains("left"));
    assert_eq!(editor.project, before);
    assert!(editor.redo());
    assert!(!editor.project.graph.nodes.contains_key("left"));
}

#[test]
fn stream_identity_blocks_stale_events_and_project_replacement() {
    let mut editor = Editor::new(Project::from(text_graph()));
    editor.start_turn("left", "turn-1").unwrap();
    assert!(editor.start_turn("left", "turn-2").is_err());
    assert!(!editor.append_turn("left", "turn-old", "stale"));
    assert!(editor.append_turn("left", "turn-1", " response"));
    assert!(editor.replace_project(Project::default()).is_err());
    assert!(editor.is_node_blocked("root"));
    assert!(!editor.is_node_blocked("right"));
    assert!(editor.is_node_blocked("merge"));
    assert!(!editor.finish_turn("left", "turn-old"));
    assert!(editor.finish_turn("left", "turn-1"));
    assert!(!editor.append_turn("left", "turn-1", "late"));
    assert!(!editor.set_turn_provenance("left", "turn-1", &json!({"completeness":"complete"})));
    assert_eq!(editor.project.graph.nodes["left"].content, "left response");
}

#[test]
fn independent_branches_can_stream_concurrently() {
    let mut editor = Editor::new(Project::from(text_graph()));
    editor.start_turn("left", "turn-left").unwrap();
    editor.start_turn("right", "turn-right").unwrap();
    assert_eq!(editor.active_turns.len(), 2);
    assert!(editor.append_turn("left", "turn-left", " L"));
    assert!(editor.append_turn("right", "turn-right", " R"));
    assert!(editor.is_node_blocked("merge"));
    assert!(editor.is_node_blocked("root"));
    assert!(editor.finish_turn("left", "turn-left"));
    assert_eq!(editor.active_turns.len(), 1);
    assert!(editor.append_turn("right", "turn-right", " completed"));
    assert!(editor.finish_turn("right", "turn-right"));
}

#[test]
fn undoing_an_independent_edit_preserves_the_completed_response() {
    let mut editor = Editor::new(Project::from(text_graph()));
    editor.start_turn("left", "turn-left").unwrap();
    assert!(editor.append_turn("left", "turn-left", " partial"));
    editor
        .edit(|project| {
            project
                .graph
                .set_content("right", "edited branch".into(), 2.0)
        })
        .unwrap();
    assert!(editor.append_turn("left", "turn-left", " completed"));
    assert!(editor.set_turn_provenance(
        "left",
        "turn-left",
        &json!({"completeness":"complete","references":[],"activity":[]})
    ));
    assert!(!editor.undo());
    assert!(editor.finish_turn("left", "turn-left"));
    let completed = editor.project.graph.nodes["left"].clone();
    assert!(editor.undo());
    assert_eq!(editor.project.graph.nodes["right"].content, "right");
    assert_eq!(editor.project.graph.nodes["left"], completed);
    assert!(editor.redo());
    assert_eq!(editor.project.graph.nodes["right"].content, "edited branch");
    assert_eq!(editor.project.graph.nodes["left"], completed);
}

#[test]
fn failed_turn_keeps_partial_response_and_ignores_stale_errors() {
    let mut editor = Editor::new(Project::from(text_graph()));
    editor.start_turn("left", "current").unwrap();
    assert!(editor.append_turn("left", "current", " partial"));
    editor
        .edit(|project| project.graph.set_content("right", "edit".into(), 2.0))
        .unwrap();
    assert!(!editor.fail_turn("left", "obsolete", "stale error"));
    assert!(editor.fail_turn("left", "current", "connection closed"));
    assert!(!editor.active_turns.contains_key("left"));
    let response = editor.project.graph.nodes["left"].clone();
    assert_eq!(
        response.content,
        "left partial\n\n[Error: connection closed]"
    );
    assert!(response.assistant_data().unwrap().incomplete);
    assert!(response.content_updated_at.is_some());
    assert!(editor.undo());
    assert_eq!(editor.project.graph.nodes["left"], response);
    assert!(!editor.fail_turn("left", "current", "late error"));
}

#[test]
fn file_only_and_image_only_inputs_are_valid_but_empty_user_text_is_not() {
    let mut graph = Graph::default();
    graph.add_node(GraphNode::user("u", "  ", 1.0), Position::default());
    assert!(!graph.can_generate("u"));
    graph.add_node(
        GraphNode::file("f", file_data("a.md"), 1.0),
        Position::default(),
    );
    graph.add_edge("f", "u").unwrap();
    assert!(graph.can_generate("u"));
    assert_eq!(graph.conversation_path("u")[0].files[0].path, "a.md");
    assert_eq!(graph.conversation_path("u")[0].content, "");
}

#[test]
fn palette_snapshot_is_stable_matches_all_tokens_and_uses_utf8_ranges() {
    let mut graph = text_graph();
    graph.nodes["root"].content = "Résumé 🦀 Rust language".into();
    graph.nodes["left"].summary = Some("Rust guide".into());
    let snapshot = CorpusSnapshot::new(graph.nodes.values().cloned());
    graph.nodes["root"].content = "changed".into();
    let result = snapshot.search("rust", 10);
    assert_eq!(result.total, 2);
    assert_eq!(result.hits[0].node.id, "left");
    let snippet = result.hits[1].snippet.as_ref().unwrap();
    assert_eq!(
        &snippet.text[snippet.spans[0].start..snippet.spans[0].end],
        "Rust"
    );
    assert_eq!(snapshot.search("résumé language", 10).total, 1);
    assert_eq!(snapshot.search("rust missing", 10).total, 0);
    assert_eq!(snapshot.search("rust", 1).total, 2);
}

#[test]
fn palette_matches_the_existing_typescript_unicode_order_and_display() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/typescript-parity.json")).unwrap();
    for fixture in fixtures["palette"].as_array().unwrap() {
        let corpus: Vec<_> = fixture["corpus"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(normalize_node)
            .collect();
        let result = search_nodes(&corpus, fixture["query"].as_str().unwrap(), 20);
        let text = |value: &HighlightedText| json!({"text":value.text,"spans":value.spans.iter().map(|span| json!({"start":span.start,"end":span.end})).collect::<Vec<_>>()});
        let hits: Vec<_> = result.hits.iter().map(|hit| json!({"id":hit.node.id,"title":text(&hit.title),"snippet":hit.snippet.as_ref().map(&text)})).collect();
        assert_eq!(json!(hits), fixture["expected"], "{}", fixture["name"]);
    }
}

#[test]
fn kagi_import_round_trips_existing_fixture_and_inclusive_ranges() {
    let conversation =
        parse_kagi_export(include_bytes!("../../../test/fixtures/kagi-export-v1.json")).unwrap();
    assert_eq!(conversation.import_key, "Example research conversation");
    assert_eq!(conversation.turns.len(), 2);
    let provenance = conversation.turns[0].provenance.as_ref().unwrap();
    assert_eq!(provenance.references.len(), 3);
    let TurnReference::Url { relations, .. } = &provenance.references[0] else {
        unreachable!()
    };
    assert_eq!(relations, &[TurnReferenceRelation::Cited]);
    let graph = conversation.to_graph(TurnRange {
        start_index: Some(1),
        end_index: Some(1),
    });
    assert_eq!(graph.nodes.len(), 2);
    assert_eq!(graph.edges.len(), 1);
    assert!(graph.nodes.keys().all(|id| id.contains("turn:1:")));
    let project = Project::from(graph);
    assert_eq!(
        Project::from_json(&project.to_json().unwrap()).unwrap(),
        project
    );
}

#[test]
fn kagi_rejects_oversize_invalid_utf8_unknown_version_and_empty_input() {
    assert!(matches!(
        parse_kagi_export_with_limit(b"{}", 1),
        Err(ImportError::InputTooLarge { .. })
    ));
    assert!(matches!(
        parse_kagi_export([0xff]),
        Err(ImportError::InvalidJson)
    ));
    assert!(matches!(
        parse_kagi_export(include_bytes!(
            "../../../test/fixtures/kagi-export-v99.json"
        )),
        Err(ImportError::UnsupportedVersion(_))
    ));
    assert!(matches!(
        parse_kagi_export(br#"{"version":1,"messages":[]}"#),
        Err(ImportError::NoMessages)
    ));
}

#[test]
fn kagi_retains_unanswered_users_and_drops_orphan_assistant_messages() {
    let conversation = parse_kagi_export(br#"{"version":1,"messages":[{"role":"assistant","content":"orphan"},{"role":"user","content":"one"},{"role":"user","content":"two"},{"role":"assistant","content":"answer"},{"role":"user","content":"three"}]}"#).unwrap();
    assert_eq!(conversation.turns.len(), 3);
    assert!(conversation.turns[0].incomplete);
    assert_eq!(conversation.turns[1].assistant_answer, "answer");
    assert!(conversation.turns[2].incomplete);
}

#[test]
fn tidy_layout_preserves_synthesis_edges_and_supports_both_directions() {
    let mut graph = text_graph();
    let edges = graph.edges.clone();
    graph.auto_layout(LayoutOptions::default());
    assert_eq!(graph.edges, edges);
    assert!(graph.layout["root"].y < graph.layout["left"].y);
    assert!(graph.layout["left"].y < graph.layout["merge"].y);
    assert_ne!(graph.layout["left"].x, graph.layout["right"].x);
    graph.auto_layout(LayoutOptions {
        direction: LayoutDirection::LeftToRight,
        ..Default::default()
    });
    assert!(graph.layout["root"].x < graph.layout["left"].x);
}

#[test]
fn imported_graph_matches_typescript_text_metadata_ids_edges_and_layout() {
    let fixtures: Value =
        serde_json::from_str(include_str!("fixtures/typescript-parity.json")).unwrap();
    for fixture in fixtures["imports"].as_array().unwrap() {
        let imported = parse_kagi_export(fixture["input"].as_str().unwrap()).unwrap();
        let project = Project::from(imported.to_graph(TurnRange::default()));
        let expected =
            Project::from_json(&json!({"version":5,"graph":fixture["graph"]}).to_string()).unwrap();
        assert_eq!(project, expected, "{}", fixture["name"]);
    }
}

#[test]
fn layout_keeps_sibling_order_grid_and_legacy_unplaceable_nodes() {
    let mut graph = text_graph();
    graph
        .layout
        .insert("left".into(), Position { x: 600., y: 300. });
    graph
        .layout
        .insert("right".into(), Position { x: 100., y: 300. });
    graph.add_node(
        GraphNode::user("independent", "separate", 1.),
        Position { x: 900., y: 0. },
    );
    let edges = graph.edges.clone();
    graph.auto_layout(LayoutOptions::default());
    assert!(graph.layout["right"].x < graph.layout["left"].x);
    assert!(
        (graph.layout["root"].x - (graph.layout["right"].x + graph.layout["left"].x) / 2.).abs()
            <= 10.
    );
    assert!(graph.layout["independent"].x > graph.layout["left"].x);
    assert!(graph
        .layout
        .values()
        .all(|p| p.x % 20. == 0. && p.y % 20. == 0.));
    let reopened = Project::from_json(&Project::from(graph.clone()).to_json().unwrap()).unwrap();
    assert_eq!(reopened.graph.edges, edges);
    assert_eq!(reopened.graph.layout, graph.layout);
    graph.edges.push(GraphEdge {
        id: "legacy-cycle".into(),
        source: "merge".into(),
        target: "root".into(),
    });
    graph.edges.push(GraphEdge {
        id: "missing".into(),
        source: "unknown".into(),
        target: "gone".into(),
    });
    let nodes = graph.nodes.clone();
    graph.auto_layout(LayoutOptions::default());
    assert_eq!(graph.nodes, nodes);
    assert_eq!(graph.layout.len(), nodes.len());
    assert!(graph
        .layout
        .values()
        .all(|p| p.x.is_finite() && p.y.is_finite()));
}

#[test]
fn authored_project_round_trips_through_both_typescript_and_rust() {
    let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
    let fixture = root.join("docs/gpui/fixtures/parity.thoughttree");
    let mut source: Value =
        serde_json::from_str(&std::fs::read_to_string(fixture).unwrap()).unwrap();
    source["projectModelPreferences"] =
        json!({"codex":"fixture-model","claude-code":"fixture-claude"});
    let project = Project::from_json(&source.to_string()).unwrap();
    assert!(project
        .graph
        .nodes
        .values()
        .any(|node| !node.images().is_empty()));
    assert!(project
        .graph
        .nodes
        .values()
        .any(|node| node.file_data().is_some()));
    assert!(project
        .graph
        .nodes
        .values()
        .any(|node| node.provenance().is_some()));
    assert!(project
        .graph
        .nodes
        .values()
        .any(|node| node.summary.is_some()));
    assert!(project.project_model_preferences.is_some());
    assert!(project.project_effort_preferences.is_some());
    let saved = std::env::temp_dir().join(format!(
        "thoughttree-roundtrip-{}.json",
        uuid::Uuid::new_v4()
    ));
    let original = saved.with_extension("input.json");
    std::fs::write(&original, source.to_string()).unwrap();
    std::fs::write(&saved, project.to_json().unwrap()).unwrap();
    let result = std::process::Command::new("bun")
        .arg(root.join("crates/thoughttree-gpui-model/tests/check-ts-roundtrip.ts"))
        .arg(&original)
        .arg(&saved)
        .output();
    std::fs::remove_file(saved).unwrap();
    std::fs::remove_file(original).unwrap();
    let result = result.expect("Bun is required to verify the existing TypeScript project reader");
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
}

#[test]
fn provenance_survives_save_reopen_and_markdown_export_without_changing_answer() {
    let mut graph = text_graph();
    let node = graph.nodes.get_mut("left").unwrap();
    node.content = "Exact assistant answer【3】".into();
    let NodeKind::Assistant(assistant) = &mut node.kind else {
        unreachable!()
    };
    assistant.provenance = normalize_provenance(&json!({
        "completeness":"complete",
        "references":[{"type":"url","url":"https://example.com/evidence","title":"Evidence","citationIndex":3,"relations":["cited"]},{"type":"file","scope":"vault","path":"notes/context.md","relations":["read"]}],
        "activity":[{"type":"commentary","content":"Checked sources","timestamp":1}]
    }));
    let project = Project::from(graph);
    let reopened = Project::from_json(&project.to_json().unwrap()).unwrap();
    assert_eq!(reopened, project);
    assert_eq!(
        reopened.graph.nodes["left"].content,
        "Exact assistant answer【3】"
    );
    let exported = reopened
        .graph
        .export_markdown(&reopened.graph.conversation_path_ids("left"));
    for expected in [
        "Exact assistant answer【3】",
        "### Provenance",
        "https://example.com/evidence",
        "Relations: cited",
        "notes/context.md",
        "Checked sources",
    ] {
        assert!(exported.contains(expected), "{expected}");
    }
}
