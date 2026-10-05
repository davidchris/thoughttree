use std::{fs, path::Path, sync::OnceLock, time::Duration};

use filetime::{set_file_mtime, FileTime};
use tempfile::TempDir;
use thoughttree_core::events::{PermissionRequestOption, SessionEventSink};

use super::*;

fn isolated() -> (TempDir, Desktop, async_channel::Receiver<DesktopEvent>) {
    // Recovery is shared by the two frontends, independently of Config. Keep
    // this entire test process inside a temporary machine-state directory.
    static LOCAL_STATE: OnceLock<TempDir> = OnceLock::new();
    LOCAL_STATE.get_or_init(|| {
        let state = tempfile::tempdir().unwrap();
        std::env::set_var("THOUGHTTREE_LOCAL_STATE_DIR", state.path());
        state
    });
    let root = tempfile::tempdir().unwrap();
    let (desktop, events) = Desktop::open(root.path().join("config")).unwrap();
    let vault = root.path().join("vault");
    fs::create_dir(&vault).unwrap();
    desktop.set_notes_directory(vault).unwrap();
    (root, desktop, events)
}

#[test]
fn config_round_trip_preserves_tauri_and_unknown_provider_keys() {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("config.json"), r#"{
        "default_provider":"claude-code", "model_preferences":{"codex":"chosen-model","future":"model"},
        "effort_preferences":{"codex":"high"}, "provider_paths":{"future":"/opt/adapter"},
        "recent_projects":["/vault/test.thoughttree"], "future_setting":{"enabled":true}
    }"#).unwrap();
    let (desktop, _) = Desktop::open(root.path().to_owned()).unwrap();
    assert_eq!(desktop.config().default_provider, AgentProvider::ClaudeCode);
    desktop.set_default_provider(AgentProvider::Codex).unwrap();
    desktop
        .set_effort_preference(&AgentProvider::Codex, Some(ReasoningEffort::XHigh))
        .unwrap();
    let (reopened, _) = Desktop::open(root.path().to_owned()).unwrap();
    assert_eq!(
        reopened
            .config()
            .effort_preferences
            .get(&AgentProvider::Codex),
        Some(&ReasoningEffort::XHigh)
    );
    let value = serde_json::to_value(reopened.config()).unwrap();
    assert_eq!(value["future_setting"]["enabled"], true);
    assert_eq!(value["model_preferences"]["future"], "model");
    assert_eq!(value["provider_paths"]["future"], "/opt/adapter");
}

#[test]
fn reload_adopts_external_settings_but_the_vault_waits_for_its_transition() {
    let root = tempfile::tempdir().unwrap();
    let (vault, next) = (root.path().join("vault"), root.path().join("next"));
    fs::create_dir(&vault).unwrap();
    fs::create_dir(&next).unwrap();
    let config = root.path().join("config");
    let (session, _) = Desktop::open(config.clone()).unwrap();
    session.set_notes_directory(vault.clone()).unwrap();
    let (other, _) = Desktop::open(config).unwrap();
    other
        .set_default_provider(AgentProvider::ClaudeCode)
        .unwrap();
    other.set_notes_directory(next.clone()).unwrap();

    let disk = session.reload_config().unwrap();
    assert_eq!(disk.notes_directory.as_deref(), Some(next.as_path()));
    assert_eq!(session.config().default_provider, AgentProvider::ClaudeCode);
    assert_eq!(session.notes_directory().unwrap(), vault);
    // An unrelated local write keeps the other frontend's Vault on disk.
    session
        .set_effort_preference(&AgentProvider::Codex, Some(ReasoningEffort::Low))
        .unwrap();
    assert_eq!(session.notes_directory().unwrap(), vault);
    assert_eq!(
        session.reload_config().unwrap().notes_directory.as_deref(),
        Some(next.as_path())
    );
    // Adoption never writes: a Vault chosen meanwhile is not reverted.
    let newest = root.path().join("newest");
    fs::create_dir(&newest).unwrap();
    other.set_notes_directory(newest.clone()).unwrap();
    assert!(session.adopt_notes_directory(&next).is_err());
    assert_eq!(session.notes_directory().unwrap(), vault);
    session.adopt_notes_directory(&newest).unwrap();
    assert_eq!(session.notes_directory().unwrap(), newest);
    assert_eq!(
        session.reload_config().unwrap().notes_directory.as_deref(),
        Some(newest.as_path())
    );
}

#[test]
fn independent_windows_do_not_overwrite_unrelated_native_preferences() {
    let root = tempfile::tempdir().unwrap();
    let (first, _) = Desktop::open(root.path().to_owned()).unwrap();
    let (second, _) = Desktop::open(root.path().to_owned()).unwrap();
    first
        .set_model_preference(&AgentProvider::Codex, Some("chosen".into()))
        .unwrap();
    second
        .set_effort_preference(&AgentProvider::Codex, Some(ReasoningEffort::Low))
        .unwrap();
    let (third, _) = Desktop::open(root.path().to_owned()).unwrap();
    assert_eq!(
        third
            .config()
            .model_preferences
            .get(&AgentProvider::Codex)
            .map(String::as_str),
        Some("chosen")
    );
    assert_eq!(
        third.config().effort_preferences.get(&AgentProvider::Codex),
        Some(&ReasoningEffort::Low)
    );
}

#[test]
fn retired_default_provider_and_malformed_keys_do_not_break_other_settings() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("config.json"),
        r#"{
        "default_provider":"gemini", "notes_directory":"/synthetic/vault",
        "model_preferences":{"codex":"chosen"}, "effort_preferences":{"codex":"unsupported"},
        "provider_paths":false, "recent_projects":[12,"/synthetic/vault/project.thoughttree",null]
    }"#,
    )
    .unwrap();
    let (desktop, _) = Desktop::open(root.path().to_owned()).unwrap();
    let config = desktop.config();
    assert_eq!(config.default_provider, AgentProvider::Codex);
    assert_eq!(
        config.notes_directory.as_deref(),
        Some(Path::new("/synthetic/vault"))
    );
    assert_eq!(
        config
            .model_preferences
            .get(&AgentProvider::Codex)
            .map(String::as_str),
        Some("chosen")
    );
    assert!(config
        .effort_preferences
        .get(&AgentProvider::Codex)
        .is_none());
    assert!(config.provider_paths.get(&AgentProvider::Codex).is_none());
    assert_eq!(
        config.recent_projects,
        ["/synthetic/vault/project.thoughttree"]
    );
}

#[test]
fn project_round_trip_detects_external_edits_and_keeps_recovery() {
    let (_root, desktop, _) = isolated();
    let path = Path::new("project.thoughttree");
    let first = desktop.save_project(path, "first", None).unwrap();
    assert_eq!(desktop.load_project(path).unwrap().revision, first);
    fs::write(desktop.project_path(path).unwrap(), "external change").unwrap();
    let error = desktop
        .save_project(path, "my edits", Some(&first))
        .unwrap_err();
    assert!(matches!(error, VaultError::Stale { .. }));
    assert_eq!(
        desktop.load_project(path).unwrap().content,
        "external change"
    );
    let entries = desktop.list_project_recovery().unwrap();
    assert!(entries
        .iter()
        .any(|entry| desktop.read_project_recovery(&entry.id).unwrap() == "my edits"));
    let (copy, _) = desktop.save_project_copy(path, "my edits").unwrap();
    assert_eq!(desktop.load_project(&copy).unwrap().content, "my edits");
}

#[test]
fn project_boundary_rejects_traversal_and_outside_picker_paths() {
    let (root, desktop, _) = isolated();
    let outside = root.path().join("outside.thoughttree");
    fs::write(&outside, "untouched").unwrap();
    for path in [outside.as_path(), Path::new("../outside.thoughttree")] {
        assert!(matches!(
            desktop.load_project(path),
            Err(VaultError::InvalidPath)
        ));
        assert!(matches!(
            desktop.save_project(path, "modified", None),
            Err(VaultError::InvalidPath)
        ));
    }
    assert_eq!(fs::read_to_string(outside).unwrap(), "untouched");
}

#[cfg(unix)]
#[test]
fn project_and_file_boundaries_reject_symlink_escapes() {
    let (root, desktop, _) = isolated();
    let outside = root.path().join("outside");
    fs::create_dir(&outside).unwrap();
    fs::write(outside.join("project.thoughttree"), "untouched").unwrap();
    std::os::unix::fs::symlink(&outside, desktop.notes_directory().unwrap().join("linked"))
        .unwrap();
    assert!(matches!(
        desktop.load_project(Path::new("linked/project.thoughttree")),
        Err(VaultError::InvalidPath)
    ));
    assert!(matches!(
        desktop.save_project(Path::new("linked/new.thoughttree"), "new", None),
        Err(VaultError::InvalidPath)
    ));
    assert_eq!(
        desktop
            .stat_vault_file("linked/project.thoughttree")
            .unwrap(),
        VaultFileStatus::Invalid
    );
    assert!(desktop
        .resolve_dropped_file(&outside.join("project.thoughttree"))
        .is_err());
}

#[test]
fn sidebar_lists_newest_projects_and_bounded_file_search() {
    let (_root, desktop, _) = isolated();
    let vault = desktop.notes_directory().unwrap();
    for name in ["alpha.thoughttree", "beta.thoughttree", "notes.md"] {
        fs::write(vault.join(name), "{}").unwrap();
        set_file_mtime(vault.join(name), FileTime::from_unix_time(100, 0)).unwrap();
    }
    fs::create_dir(vault.join("nested")).unwrap();
    fs::write(vault.join("nested/recent.thoughttree"), "{}").unwrap();
    let paths: Vec<_> = desktop
        .list_projects()
        .unwrap()
        .into_iter()
        .map(|entry| entry.relative_path)
        .collect();
    assert_eq!(
        paths,
        [
            "nested/recent.thoughttree",
            "alpha.thoughttree",
            "beta.thoughttree"
        ]
    );
    assert_eq!(desktop.search_files("NOTES", 20).unwrap(), ["notes.md"]);
    assert!(desktop.search_files("", 0).unwrap().is_empty());
}

#[test]
fn recent_projects_reorder_and_prune_without_deleting_files() {
    let (_root, desktop, _) = isolated();
    let vault = desktop.notes_directory().unwrap();
    let first = vault.join("first.thoughttree");
    let second = vault.join("second.thoughttree");
    fs::write(&first, "first Project").unwrap();
    fs::write(&second, "second Project").unwrap();
    desktop.add_recent_project(&first).unwrap();
    desktop.add_recent_project(&second).unwrap();
    desktop.add_recent_project(&first).unwrap();
    let recent = desktop.config().recent_projects;
    assert_eq!(recent.len(), 2);
    assert_eq!(Path::new(&recent[0]), first.canonicalize().unwrap());
    desktop.remove_recent_project(&recent[0]).unwrap();
    assert_eq!(fs::read_to_string(&first).unwrap(), "first Project");
    fs::remove_file(second).unwrap();
    desktop.prune_recent_projects().unwrap();
    assert!(desktop.config().recent_projects.is_empty());
    assert!(first.exists());
}

#[test]
fn file_preview_uses_live_stat_and_core_attachment_limits() {
    let (_root, desktop, _) = isolated();
    let path = desktop.notes_directory().unwrap().join("notes.md");
    fs::write(&path, "before").unwrap();
    let original = desktop.stat_vault_file("notes.md").unwrap();
    let preview = desktop.read_vault_file_preview("notes.md").unwrap();
    assert_eq!(preview.info.size, 6);
    fs::write(&path, "after and longer").unwrap();
    assert_ne!(original, desktop.stat_vault_file("notes.md").unwrap());
    assert_eq!(
        desktop
            .read_vault_file_preview("notes.md")
            .unwrap()
            .info
            .size,
        16
    );
    assert_eq!(
        desktop.attachment_limits().image_max_bytes,
        thoughttree_core::vault::files::limits::attachment_limits().image_max_bytes
    );
    assert_eq!(
        desktop.stat_vault_file("missing.md").unwrap(),
        VaultFileStatus::Missing
    );
    assert!(desktop
        .read_vault_file_preview("missing.md")
        .unwrap_err()
        .starts_with("missing:"));
}

#[test]
fn invalid_attachment_emits_completion_without_starting_an_adapter() {
    let (_root, desktop, events) = isolated();
    let request = PromptRequest {
        node_id: "node".into(),
        turn_id: "turn".into(),
        provider: None,
        model_id: None,
        effort: None,
        messages: vec![Message {
            role: "user".into(),
            content: "Read this".into(),
            images: None,
            files: Some(vec![MessageFile {
                path: "../outside.md".into(),
                name: "outside.md".into(),
                mime_type: "text/markdown".into(),
                size: 0,
            }]),
        }],
    };
    desktop.start_prompt(request.clone()).unwrap();
    let event = desktop.runtime().block_on(async {
        tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
            .unwrap()
    });
    assert!(matches!(
        event,
        DesktopEvent::PromptFinished { result: Err(_), .. }
    ));
    // Failure released the core reservation; retry is possible immediately.
    desktop.start_prompt(request).unwrap();
}

#[test]
fn permission_round_trip_preserves_the_offered_option() {
    let (_root, desktop, events) = isolated();
    let request = PermissionRequestEvent::new(
        "request".into(),
        "node".into(),
        "execute".into(),
        "Shell".into(),
        "Run build".into(),
        vec![PermissionRequestOption {
            id: "allow-once".into(),
            label: "Allow once".into(),
        }],
    );
    let broker = desktop.0.broker.clone();
    let sink = desktop.0.sink.clone();
    let pending = desktop
        .runtime()
        .spawn(async move { broker.request(request, &sink).await });
    let event = events.recv_blocking().unwrap();
    let DesktopEvent::PermissionRequest(event) = event else {
        panic!("Expected permission")
    };
    desktop.respond_to_permission(event.request_id, event.options[0].id.clone());
    assert_eq!(
        desktop.runtime().block_on(pending).unwrap().unwrap(),
        "allow-once"
    );
}

#[test]
fn closed_event_channel_marks_a_permission_undeliverable() {
    let (sender, receiver) = async_channel::unbounded();
    drop(receiver);
    let request = PermissionRequestEvent::default();
    NativeEventSink(sender).permission_request(request.clone());
    assert!(request.delivery_failed());
}
