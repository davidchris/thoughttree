//! Native Project lifecycle against real, isolated files and the test platform's
//! save picker. These checks exercise asynchronous UI completion bookkeeping.
use std::{fs, path::Path};

use gpui::{Entity, TestAppContext, VisualTestContext};
use thoughttree_gpui_model::Project;

use crate::{dialogs::Modal, interaction_tests::workspace, workspace::Workspace};

fn edit_question(workspace: &Entity<Workspace>, text: &str, cx: &mut VisualTestContext) {
    workspace.update(cx, |this, _| {
        this.editor
            .edit(|project| {
                project.graph.nodes["question"].content = text.into();
                Ok(())
            })
            .unwrap();
    });
}

fn read_project(path: &Path) -> Project {
    Project::from_json(&fs::read_to_string(path).unwrap()).unwrap()
}

#[gpui::test]
fn continuous_unsaved_edits_and_streaming_receive_recovery_checkpoints(cx: &mut TestAppContext) {
    use std::time::Duration;
    use thoughttree_core::events::StreamChunkEvent;
    use thoughttree_desktop::DesktopEvent;

    let (_directory, workspace, cx, events) = workspace(cx);
    for index in 0..12 {
        workspace.update(cx, |this, cx| {
            this.change(
                |project| {
                    project.graph.nodes["question"].content = format!("Continuous edit {index}");
                },
                cx,
            );
        });
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
    }
    workspace.read_with(cx, |this, _| {
        assert!(this.project_path.is_none());
        assert!(this.editor.is_dirty());
        assert!(
            this.snapshot_revision.is_some(),
            "Idle debounce never fired, but the periodic checkpoint did"
        );
    });
    cx.executor().advance_clock(Duration::from_millis(900));
    cx.run_until_parked();
    workspace.update(cx, |this, _| {
        assert_eq!(this.snapshot_revision, Some(this.editor.edit_revision));
        this.editor
            .start_turn("answer", "checkpoint-stream")
            .unwrap();
    });
    for index in 0..12 {
        events
            .try_send(DesktopEvent::StreamChunk(StreamChunkEvent {
                node_id: "answer".into(),
                turn_id: "checkpoint-stream".into(),
                chunk: format!(" checkpoint-chunk-{index}"),
            }))
            .unwrap();
        cx.run_until_parked();
        cx.executor().advance_clock(Duration::from_millis(500));
        cx.run_until_parked();
    }
    workspace.read_with(cx, |this, _| {
        assert!(this.editor.active_turns.contains_key("answer"));
        assert!(
            this.desktop
                .list_project_recovery()
                .unwrap()
                .iter()
                .any(|entry| {
                    this.desktop
                        .read_project_recovery(&entry.id)
                        .unwrap()
                        .contains("checkpoint-chunk-")
                }),
            "Partial responses reach durable recovery before the Turn ends"
        );
    });
}

#[gpui::test]
fn new_project_writes_empty_file_and_clears_transient_state(cx: &mut TestAppContext) {
    let (directory, workspace, cx, _) = workspace(cx);
    // Config writes run on a real thread so a held cross-process lock cannot block input.
    cx.executor().allow_parking();
    edit_question(&workspace, "Snapshot before New", cx);
    workspace.update(cx, |this, _| {
        this.editor.selection.insert("question".into());
        this.preview_id = Some("question".into());
        this.editing = true;
        this.permissions
            .push_back(thoughttree_core::events::PermissionRequestEvent::default());
        this.file_preview_errors
            .insert("old-file".into(), "Old error".into());
        this.file_status
            .insert("old-file".into(), Err("Old status".into()));
        this.mentions.push("old-reference.md".into());
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.new_project(window, cx));
    });
    cx.simulate_new_path_selection(|directory| Some(directory.join("new-project.thoughttree")));
    cx.run_until_parked();
    let path = directory
        .path()
        .join("vault/new-project.thoughttree")
        .canonicalize()
        .unwrap();
    assert!(read_project(&path).graph.nodes.is_empty());
    workspace.read_with(cx, |this, _| {
        assert_eq!(this.project_path.as_ref(), Some(&path));
        assert!(!this.editor.is_dirty());
        assert!(this.editor.selection.is_empty());
        assert!(this.preview_id.is_none());
        assert!(!this.editing);
        assert!(this.permissions.is_empty());
        assert!(this.file_preview_errors.is_empty());
        assert!(this.file_status.is_empty());
        assert!(this.mentions.is_empty());
    });
}

#[gpui::test]
fn recovery_refuses_active_turns_and_snapshot_failure_then_restores_preferences(
    cx: &mut TestAppContext,
) {
    let (directory, workspace, cx, _) = workspace(cx);
    // Config writes run on a real thread so a held cross-process lock cannot block input.
    cx.executor().allow_parking();
    edit_question(&workspace, "Original saved Project", cx);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.save(window, cx));
    });
    cx.simulate_new_path_selection(|directory| Some(directory.join("original.thoughttree")));
    cx.run_until_parked();
    let recovery = workspace.read_with(cx, |this, _| {
        let mut project = this.editor.project.clone();
        project.graph.nodes["question"].content = "Recovered other Project".into();
        project.project_model_preferences =
            Some([("codex".into(), "recovered-model".into())].into());
        project.project_effort_preferences = Some(
            [(
                "codex".into(),
                thoughttree_gpui_model::ReasoningEffort::High,
            )]
            .into(),
        );
        this.desktop
            .snapshot_project(None, &project.to_json().unwrap())
            .unwrap()
    });
    edit_question(&workspace, "Current unsaved Project before recovery", cx);
    workspace.update(cx, |this, _| {
        this.editor.start_turn("answer", "active").unwrap()
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.restore_recovery(&recovery, window, cx));
    });
    workspace.update(cx, |this, _| {
        assert!(this.notice.as_deref().unwrap().contains("active Turn"));
        assert_eq!(
            this.editor.project.graph.nodes["question"].content,
            "Current unsaved Project before recovery"
        );
        assert!(this.editor.finish_turn("answer", "active"));
    });
    let vault = directory.path().join("vault");
    let offline = directory.path().join("offline-vault");
    fs::rename(&vault, &offline).unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.restore_recovery(&recovery, window, cx));
    });
    cx.run_until_parked();
    workspace.read_with(cx, |this, _| {
        assert!(this.project_path.is_some());
        assert!(this.notice.is_some());
        assert_eq!(
            this.editor.project.graph.nodes["question"].content,
            "Current unsaved Project before recovery"
        );
    });
    fs::rename(&offline, &vault).unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.restore_recovery(&recovery, window, cx));
    });
    cx.run_until_parked();
    workspace.read_with(cx, |this, _| {
        assert!(this.project_path.is_none());
        assert!(this.editor.is_dirty());
        assert_eq!(
            this.editor
                .project
                .project_model_preferences
                .as_ref()
                .unwrap()["codex"],
            "recovered-model"
        );
        assert_eq!(
            this.editor
                .project
                .project_effort_preferences
                .as_ref()
                .unwrap()["codex"],
            thoughttree_gpui_model::ReasoningEffort::High
        );
        assert!(this
            .desktop
            .list_project_recovery()
            .unwrap()
            .iter()
            .any(|entry| {
                this.desktop
                    .read_project_recovery(&entry.id)
                    .unwrap()
                    .contains("Current unsaved Project before recovery")
            }));
    });
}

#[gpui::test]
fn unavailable_vault_errors_clear_after_a_successful_save_without_clearing_other_notices(
    cx: &mut TestAppContext,
) {
    let (directory, workspace, cx, _) = workspace(cx);
    // Config writes run on a real thread so a held cross-process lock cannot block input.
    cx.executor().allow_parking();
    edit_question(&workspace, "Initial saved content", cx);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.save(window, cx));
    });
    cx.simulate_new_path_selection(|directory| Some(directory.join("project.thoughttree")));
    cx.run_until_parked();
    let vault = directory.path().join("vault");
    let disconnected = directory.path().join("disconnected");
    fs::rename(&vault, &disconnected).unwrap();
    edit_question(&workspace, "Edits while Vault is unavailable", cx);
    workspace.update(cx, |this, cx| this.save_current(cx));
    cx.run_until_parked();
    workspace.read_with(cx, |this, _| {
        assert!(this.persistence_error.is_some());
        assert_eq!(this.persistence_error, this.notice);
        assert!(this.editor.is_dirty());
    });
    fs::rename(&disconnected, &vault).unwrap();
    workspace.update(cx, |this, cx| this.save_current(cx));
    cx.run_until_parked();
    workspace.read_with(cx, |this, _| {
        assert!(this.persistence_error.is_none());
        assert!(this.notice.is_none());
        assert!(!this.editor.is_dirty());
    });
    edit_question(&workspace, "One more edit", cx);
    workspace.update(cx, |this, cx| {
        this.notice = Some("A different operation needs attention".into());
        this.save_current(cx);
    });
    cx.run_until_parked();
    workspace.read_with(cx, |this, _| {
        assert_eq!(
            this.notice.as_deref(),
            Some("A different operation needs attention")
        );
    });
}

#[gpui::test]
fn invalid_open_or_edits_during_loading_keep_the_current_graph(cx: &mut TestAppContext) {
    let (directory, workspace, cx, _) = workspace(cx);
    edit_question(&workspace, "Current graph", cx);
    let invalid = directory.path().join("vault/invalid.thoughttree");
    fs::write(&invalid, "{malformed").unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.open_path(invalid, window, cx));
    });
    cx.run_until_parked();
    workspace.read_with(cx, |this, _| {
        assert!(this.notice.is_some());
        assert_eq!(
            this.editor.project.graph.nodes["question"].content,
            "Current graph"
        );
    });
    let valid = directory.path().join("vault/valid.thoughttree");
    fs::write(&valid, Project::default().to_json().unwrap()).unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.open_path(valid, window, cx));
    });
    edit_question(&workspace, "An edit after Open", cx);
    cx.run_until_parked();
    workspace.read_with(cx, |this, _| {
        assert!(this.notice.as_deref().unwrap().contains("graph changed"));
        assert!(this.project_path.is_none());
        assert_eq!(
            this.editor.project.graph.nodes["question"].content,
            "An edit after Open"
        );
        assert!(this.editor.is_dirty());
    });
}

#[gpui::test]
async fn cancelled_pickers_preserve_unsaved_work_and_save_as_adopts_only_written_path(
    cx: &mut TestAppContext,
) {
    let (directory, workspace, cx, _) = workspace(cx);
    edit_question(&workspace, "Unsaved work", cx);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.save(window, cx));
    });
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.new_project(window, cx));
    });
    cx.simulate_new_path_selection(|_| None);
    cx.run_until_parked();
    workspace.read_with(cx, |this, _| {
        assert_eq!(this.title, "Native interaction fixture");
        assert!(this.project_path.is_none());
        assert!(this.editor.is_dirty());
        assert_eq!(
            this.editor.project.graph.nodes["question"].content,
            "Unsaved work"
        );
    });

    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.save(window, cx));
    });
    cx.simulate_new_path_selection(|directory| Some(directory.join("first.thoughttree")));
    cx.run_until_parked();
    cx.executor().allow_parking();
    cx.condition(&workspace, |this, _| !this.config_is_busy())
        .await;
    let first = directory
        .path()
        .join("vault/first.thoughttree")
        .canonicalize()
        .unwrap();
    assert_eq!(
        read_project(&first).graph.nodes["question"].content,
        "Unsaved work"
    );
    workspace.read_with(cx, |this, _| {
        assert_eq!(this.project_path.as_deref(), Some(first.as_path()));
        assert!(!this.editor.is_dirty());
        assert!(this.revision.is_some());
        assert_eq!(this.desktop.config().recent_projects.len(), 1);
    });

    // Choosing an existing file never replaces that file or changes the source.
    let existing = directory.path().join("vault/existing.thoughttree");
    fs::write(&existing, "external file must remain intact").unwrap();
    edit_question(&workspace, "Later edits", cx);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.save_as(window, cx));
    });
    cx.simulate_new_path_selection(|_| Some(existing.clone()));
    cx.run_until_parked();
    assert_eq!(
        fs::read_to_string(existing).unwrap(),
        "external file must remain intact"
    );
    workspace.read_with(cx, |this, _| {
        assert_eq!(this.project_path.as_deref(), Some(first.as_path()));
        assert!(this.editor.is_dirty());
        assert!(this.notice.as_deref().unwrap().contains("already exists"));
    });
}

#[gpui::test]
async fn close_waits_for_save_as_and_save_as_keeps_the_project_generation(cx: &mut TestAppContext) {
    let (directory, workspace, cx, _) = workspace(cx);
    cx.executor().allow_parking();
    let path = directory.path().join("vault/named.thoughttree");
    let generation = workspace.read_with(cx, |this, _| this.generation);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.save_to_new_path(path.clone(), cx);
            assert!(!this.request_close(window, cx));
        })
    });
    cx.condition(&workspace, |this, _| {
        this.project_path.is_some() && !this.config_is_busy()
    })
    .await;
    workspace.read_with(cx, |this, _| {
        // Same graph under a new name: attachment, file and summary jobs
        // scoped to this Project generation stay valid.
        assert_eq!(this.generation, generation);
        assert!(!this.editor.is_dirty());
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| assert!(this.request_close(window, cx)))
    });
}

#[gpui::test]
fn queued_saves_keep_the_latest_edit_and_conflict_copy_preserves_both_versions(
    cx: &mut TestAppContext,
) {
    let (directory, workspace, cx, _) = workspace(cx);
    // Config writes run on a real thread so a held cross-process lock cannot block input.
    cx.executor().allow_parking();
    edit_question(&workspace, "Initial", cx);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.save(window, cx));
    });
    cx.simulate_new_path_selection(|directory| Some(directory.join("project.thoughttree")));
    cx.run_until_parked();
    let path = directory.path().join("vault/project.thoughttree");

    edit_question(&workspace, "First queued edit", cx);
    workspace.update(cx, |this, cx| this.save_current(cx));
    edit_question(&workspace, "Latest queued edit", cx);
    workspace.update(cx, |this, cx| this.save_current(cx));
    cx.run_until_parked();
    assert_eq!(
        read_project(&path).graph.nodes["question"].content,
        "Latest queued edit"
    );
    workspace.read_with(cx, |this, _| assert!(!this.editor.is_dirty()));

    let mut external = read_project(&path);
    external.graph.nodes["question"].content = "Another writer".into();
    fs::write(&path, external.to_json().unwrap()).unwrap();
    edit_question(&workspace, "My conflicting edit", cx);
    workspace.update(cx, |this, cx| this.save_current(cx));
    cx.run_until_parked();
    workspace.read_with(cx, |this, _| {
        assert!(matches!(this.modal, Some(Modal::Conflict)));
        assert!(this.editor.is_dirty());
    });
    assert_eq!(
        read_project(&path).graph.nodes["question"].content,
        "Another writer"
    );

    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.save_conflict_copy(window, cx));
    });
    cx.run_until_parked();
    let copy = workspace.read_with(cx, |this, _| {
        assert!(!this.editor.is_dirty());
        assert!(this.modal.is_none());
        this.project_path.clone().unwrap()
    });
    assert_ne!(copy, path);
    assert_eq!(
        read_project(&copy).graph.nodes["question"].content,
        "My conflicting edit"
    );
    assert_eq!(
        read_project(&path).graph.nodes["question"].content,
        "Another writer"
    );
}

#[gpui::test]
fn open_snapshots_dirty_work_and_recovery_restores_an_unsaved_project(cx: &mut TestAppContext) {
    let (directory, workspace, cx, _) = workspace(cx);
    // Config writes run on a real thread so a held cross-process lock cannot block input.
    cx.executor().allow_parking();
    edit_question(&workspace, "Recover this unsaved work", cx);
    let destination = directory.path().join("vault/empty.thoughttree");
    fs::write(&destination, Project::default().to_json().unwrap()).unwrap();
    let destination = destination.canonicalize().unwrap();
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.open_path(destination.clone(), window, cx)
        });
    });
    cx.run_until_parked();
    let recovery = workspace.read_with(cx, |this, _| {
        assert_eq!(this.project_path.as_deref(), Some(destination.as_path()));
        assert!(this.editor.project.graph.nodes.is_empty());
        this.desktop
            .list_project_recovery()
            .unwrap()
            .into_iter()
            .find(|entry| {
                this.desktop
                    .read_project_recovery(&entry.id)
                    .unwrap()
                    .contains("Recover this unsaved work")
            })
            .expect("Replacement must preserve the dirty graph")
            .id
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.restore_recovery(&recovery, window, cx));
    });
    cx.run_until_parked();
    workspace.read_with(cx, |this, _| {
        assert_eq!(this.title, "Recovered Project");
        assert!(this.project_path.is_none());
        assert!(this.revision.is_none());
        assert!(this.editor.is_dirty());
        assert_eq!(
            this.editor.project.graph.nodes["question"].content,
            "Recover this unsaved work"
        );
        assert!(this.file_previews.is_empty());
        assert!(this.file_preview_errors.is_empty());
    });
    assert!(read_project(&destination).graph.nodes.is_empty());
}

#[gpui::test]
async fn vault_change_preserves_the_old_file_and_keeps_current_edits_as_unsaved(
    cx: &mut TestAppContext,
) {
    let (directory, workspace, cx, _) = workspace(cx);
    edit_question(&workspace, "Saved in the original Vault", cx);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.save(window, cx));
    });
    cx.simulate_new_path_selection(|directory| Some(directory.join("original.thoughttree")));
    cx.run_until_parked();
    cx.executor().allow_parking();
    cx.condition(&workspace, |this, _| !this.config_is_busy())
        .await;
    let original = directory.path().join("vault/original.thoughttree");
    let next_vault = directory.path().join("another-vault");
    fs::create_dir(&next_vault).unwrap();
    workspace.update(cx, |this, _| {
        this.editor.start_turn("answer", "active").unwrap()
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.change_notes_directory(next_vault.clone(), window, cx)
        });
    });
    workspace.update(cx, |this, _| {
        assert_eq!(
            this.desktop.notes_directory().unwrap(),
            directory.path().join("vault")
        );
        assert!(this.editor.finish_turn("answer", "active"));
    });

    edit_question(&workspace, "Unsaved edits at Vault change", cx);
    workspace.update(cx, |this, _| {
        this.file_preview_errors
            .insert("old-file".into(), "old error".into());
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.change_notes_directory(next_vault.clone(), window, cx)
        });
    });
    cx.condition(&workspace, |this, _| {
        !matches!(this.modal, Some(Modal::ChangingVault))
    })
    .await;
    workspace.read_with(cx, |this, _| {
        assert_eq!(this.desktop.notes_directory().unwrap(), next_vault);
        assert!(this.project_path.is_none());
        assert!(this.revision.is_none());
        assert!(this.editor.is_dirty());
        assert_eq!(
            this.editor.project.graph.nodes["question"].content,
            "Unsaved edits at Vault change"
        );
        assert!(this.file_preview_errors.is_empty());
        assert!(this
            .desktop
            .list_project_recovery()
            .unwrap()
            .iter()
            .any(|entry| {
                this.desktop
                    .read_project_recovery(&entry.id)
                    .unwrap()
                    .contains("Unsaved edits at Vault change")
            }));
    });
    assert_eq!(
        read_project(&original).graph.nodes["question"].content,
        "Saved in the original Vault"
    );
}
