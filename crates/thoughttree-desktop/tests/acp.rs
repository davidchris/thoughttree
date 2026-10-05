#![cfg(unix)]

use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    path::PathBuf,
    sync::OnceLock,
    time::Duration,
};

use thoughttree_desktop::{
    AgentProvider, Desktop, DesktopEvent, Message, PromptRequest, ReasoningEffort,
};

struct Harness {
    _directory: tempfile::TempDir,
    desktop: Desktop,
    events: async_channel::Receiver<DesktopEvent>,
    runtime: tokio::runtime::Runtime,
}

fn fixture() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("../../scripts/gpui-fixture-agent.py")
        .canonicalize()
        .unwrap()
}

impl Harness {
    fn new() -> Self {
        static STATE: OnceLock<tempfile::TempDir> = OnceLock::new();
        STATE.get_or_init(|| {
            let directory = tempfile::tempdir().unwrap();
            std::env::set_var("THOUGHTTREE_LOCAL_STATE_DIR", directory.path());
            // These are local to this integration-test process. Neither an
            // inherited CLI override nor user config may call a real provider.
            std::env::set_var("CODEX_PATH", fixture());
            std::env::set_var("CODEX_CONFIG", "{}");
            directory
        });
        let directory = tempfile::tempdir().unwrap();
        let vault = directory.path().join("vault");
        let config = directory.path().join("config");
        fs::create_dir(&vault).unwrap();
        fs::create_dir(&config).unwrap();
        fs::write(
            vault.join("fixture-notes.md"),
            "Synthetic Vault file for ACP boundary tests.\n",
        )
        .unwrap();
        fs::write(
            config.join("config.json"),
            serde_json::to_vec(&serde_json::json!({
                "notes_directory": vault, "default_provider": "codex",
                "provider_paths": {"codex": fixture()},
            }))
            .unwrap(),
        )
        .unwrap();
        let (desktop, events) = Desktop::open(config).unwrap();
        Self {
            _directory: directory,
            desktop,
            events,
            runtime: tokio::runtime::Runtime::new().unwrap(),
        }
    }

    fn next(&self) -> DesktopEvent {
        self.runtime.block_on(async {
            tokio::time::timeout(Duration::from_secs(10), self.events.recv())
                .await
                .expect("fixture event timed out")
                .expect("fixture event channel closed")
        })
    }

    fn prompt(&self, node: &str, text: &str) {
        self.desktop.start_prompt(request(node, text)).unwrap();
    }
}

fn request(node: &str, text: &str) -> PromptRequest {
    PromptRequest {
        node_id: node.into(),
        turn_id: format!("turn-{node}"),
        provider: Some(AgentProvider::Codex),
        model_id: Some("fixture-alternate".into()),
        effort: Some(ReasoningEffort::XHigh),
        messages: vec![Message {
            role: "user".into(),
            content: text.into(),
            images: None,
            files: None,
        }],
    }
}

#[test]
fn subprocess_discovers_models_and_validates_the_explicit_executable() {
    let h = Harness::new();
    h.desktop.discover_models(AgentProvider::Codex).unwrap();
    let DesktopEvent::ModelsDiscovered { provider, result } = h.next() else {
        panic!("Expected models")
    };
    assert_eq!(provider, AgentProvider::Codex);
    let ids: Vec<_> = result
        .unwrap()
        .into_iter()
        .map(|model| model.model_id)
        .collect();
    assert_eq!(ids, ["fixture-model", "fixture-alternate"]);
    h.desktop.set_provider_path(
        AgentProvider::Codex,
        Some(fixture().to_string_lossy().into_owned()),
    );
    let DesktopEvent::ProviderPathValidated { result, .. } = h.next() else {
        panic!("Expected validation")
    };
    assert!(result.unwrap().contains("offline"));
}

#[test]
fn failed_executable_validation_preserves_the_path_and_reset_restores_discovery() {
    let h = Harness::new();
    let original = h
        .desktop
        .config()
        .provider_paths
        .get(&AgentProvider::Codex)
        .cloned();
    h.desktop
        .set_provider_path(AgentProvider::Codex, Some("relative/not-an-adapter".into()));
    let DesktopEvent::ProviderPathValidated { result, .. } = h.next() else {
        panic!("Expected validation failure");
    };
    assert!(result.unwrap_err().contains("absolute path"));
    assert_eq!(
        h.desktop.config().provider_paths.get(&AgentProvider::Codex),
        original.as_ref()
    );
    h.desktop.set_provider_path(AgentProvider::Codex, None);
    let DesktopEvent::ProviderPathValidated { result, .. } = h.next() else {
        panic!("Expected path reset");
    };
    assert!(result.unwrap().contains("automatic discovery"));
    assert!(h
        .desktop
        .config()
        .provider_paths
        .get(&AgentProvider::Codex)
        .is_none());
}

#[test]
fn reset_during_slow_validation_wins_and_the_superseded_request_stays_silent() {
    use std::os::unix::fs::PermissionsExt;

    let h = Harness::new();
    let slow = h._directory.path().join("slow-codex-acp");
    fs::write(
        &slow,
        "#!/bin/sh\nsleep 1\necho 'Usage: codex-acp [OPTIONS]'\n",
    )
    .unwrap();
    fs::set_permissions(&slow, fs::Permissions::from_mode(0o755)).unwrap();
    h.desktop.set_provider_path(
        AgentProvider::Codex,
        Some(slow.to_string_lossy().into_owned()),
    );
    h.desktop.set_provider_path(AgentProvider::Codex, None);
    let DesktopEvent::ProviderPathValidated { result, .. } = h.next() else {
        panic!("Expected path reset");
    };
    assert!(result.unwrap().contains("automatic discovery"));
    // Outlast the superseded probe: it must neither persist nor report.
    std::thread::sleep(Duration::from_secs(2));
    assert!(h.events.try_recv().is_err());
    assert!(h
        .desktop
        .config()
        .provider_paths
        .get(&AgentProvider::Codex)
        .is_none());
}

#[test]
fn streaming_and_provenance_arrive_before_prompt_completion() {
    let h = Harness::new();
    h.prompt("stream", "fixture:nopermission parity");
    let mut response = String::new();
    let mut chunks = 0;
    let mut provenance = None;
    loop {
        match h.next() {
            DesktopEvent::StreamChunk(event) => {
                assert_eq!(
                    (event.node_id.as_str(), event.turn_id.as_str()),
                    ("stream", "turn-stream")
                );
                assert!(provenance.is_none(), "No text after final provenance");
                response.push_str(&event.chunk);
                chunks += 1;
            }
            DesktopEvent::TurnProvenance(event) => {
                provenance = Some(serde_json::to_value(event.provenance).unwrap())
            }
            DesktopEvent::PromptFinished { result, .. } => {
                result.unwrap();
                break;
            }
            other => panic!("Unexpected {other:?}"),
        }
    }
    assert!(chunks >= 4);
    assert!(response.contains("fixture-alternate"));
    assert!(response.contains("xhigh"));
    assert!(response.contains("Deterministic fixture response"));
    let provenance = provenance.expect("provenance must precede completion");
    assert_eq!(provenance["completeness"], "complete");
    assert_eq!(provenance["references"][0]["path"], "fixture-notes.md");
    assert_eq!(provenance["references"][0]["scope"], "vault");
    assert_eq!(provenance["activity"][0]["kind"], "read");
    assert_eq!(provenance["activity"][0]["status"], "completed");
}

#[test]
fn parked_permissions_keep_streaming_and_two_turns_resume_independently() {
    let h = Harness::new();
    h.prompt("one", "parity");
    h.prompt("two", "parity");
    let mut permissions = BTreeMap::new();
    let mut streamed_while_parked = BTreeSet::new();
    while permissions.len() < 2 || streamed_while_parked.len() < 2 {
        match h.next() {
            DesktopEvent::PermissionRequest(event) => {
                assert_eq!(
                    event
                        .options
                        .iter()
                        .map(|o| o.id.as_str())
                        .collect::<Vec<_>>(),
                    ["allow-once", "reject-once"]
                );
                permissions.insert(event.node_id.clone(), event);
            }
            DesktopEvent::StreamChunk(event) => {
                if event.chunk.contains("Awaiting your choice") {
                    streamed_while_parked.insert(event.node_id);
                }
            }
            other => panic!("Turn must stay parked until explicitly answered: {other:?}"),
        }
    }
    assert!(h.desktop.start_prompt(request("one", "duplicate")).is_err());
    assert_ne!(permissions["one"].request_id, permissions["two"].request_id);
    h.desktop
        .respond_to_permission(permissions["two"].request_id.clone(), "reject-once".into());
    let mut second_response = String::new();
    loop {
        match h.next() {
            DesktopEvent::StreamChunk(event) => {
                assert_eq!(event.node_id, "two");
                second_response.push_str(&event.chunk);
            }
            DesktopEvent::TurnProvenance(event) => assert_eq!(event.node_id, "two"),
            DesktopEvent::PromptFinished {
                node_id, result, ..
            } => {
                assert_eq!(node_id, "two");
                result.unwrap();
                break;
            }
            other => panic!("Unexpected {other:?}"),
        }
    }
    assert!(second_response.contains("reject-once"));
    // The first Turn is still reserved after the second subprocess finished.
    assert!(h.desktop.start_prompt(request("one", "duplicate")).is_err());
    h.desktop
        .respond_to_permission(permissions["one"].request_id.clone(), "allow-once".into());
    let mut first_response = String::new();
    loop {
        match h.next() {
            DesktopEvent::StreamChunk(event) => {
                assert_eq!(event.node_id, "one");
                first_response.push_str(&event.chunk);
            }
            DesktopEvent::TurnProvenance(event) => assert_eq!(event.node_id, "one"),
            DesktopEvent::PromptFinished {
                node_id, result, ..
            } => {
                assert_eq!(node_id, "one");
                result.unwrap();
                break;
            }
            other => panic!("Unexpected {other:?}"),
        }
    }
    assert!(first_response.contains("allow-once"));
}

#[test]
fn failure_retains_partial_provenance_and_releases_the_turn() {
    let h = Harness::new();
    h.prompt("failed", "fixture:fail");
    let mut provenance = None;
    loop {
        match h.next() {
            DesktopEvent::StreamChunk(_) => {}
            DesktopEvent::TurnProvenance(event) => {
                provenance = Some(serde_json::to_value(event.provenance).unwrap())
            }
            DesktopEvent::PromptFinished { result, .. } => {
                assert!(result
                    .unwrap_err()
                    .contains("Deterministic fixture failure"));
                break;
            }
            other => panic!("Unexpected {other:?}"),
        }
    }
    let provenance = provenance.unwrap();
    assert_eq!(provenance["completeness"], "partial");
    assert_eq!(provenance["activity"][1]["status"], "incomplete");
    h.prompt("failed", "fixture:nopermission retry");
    loop {
        if let DesktopEvent::PromptFinished { result, .. } = h.next() {
            result.unwrap();
            break;
        }
    }
}

#[test]
fn summaries_use_the_ephemeral_fixture_cli_without_a_saved_session() {
    let h = Harness::new();
    h.desktop
        .generate_summary("summary".into(), "Synthetic text for a heading".into())
        .unwrap();
    let DesktopEvent::SummaryFinished { node_id, result } = h.next() else {
        panic!("Expected summary")
    };
    assert_eq!(node_id, "summary");
    assert_eq!(result.unwrap(), "Deterministic Fixture Response");
}
