//! Native provider controls and the model/effort sent across the ACP boundary.
//! The only subprocess is the deterministic offline fixture.

use std::{collections::BTreeMap, path::Path, time::Duration};

use gpui::{AppContext, Entity, Modifiers, TestAppContext, VisualTestContext};
use gpui_component::menu::PopupMenuItem;
use thoughttree_desktop::{AgentProvider, Desktop, DesktopEvent, ModelInfo, ReasoningEffort};
use thoughttree_gpui_model::{GraphProvider, NodeKind};

use crate::{dialogs::Modal, interaction_tests::workspace, workspace::Workspace};

fn click(cx: &mut VisualTestContext, selector: &'static str) {
    let bounds = cx.debug_bounds(selector).expect(selector);
    cx.simulate_click(bounds.center(), Modifiers::default());
}

fn next_event(events: &async_channel::Receiver<DesktopEvent>) -> DesktopEvent {
    smol::block_on(async {
        match futures::future::select(
            Box::pin(events.recv()),
            Box::pin(smol::Timer::after(Duration::from_secs(10))),
        )
        .await
        {
            futures::future::Either::Left((event, _)) => event.expect("fixture channel closed"),
            futures::future::Either::Right(_) => panic!("offline fixture event timed out"),
        }
    })
}

fn replace_desktop(
    directory: &Path,
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
) -> async_channel::Receiver<DesktopEvent> {
    let (desktop, events) = Desktop::open(directory.join("config")).unwrap();
    workspace.update(cx, |this, _| this.desktop = desktop);
    events
}

fn finish_fixture_turn(
    workspace: &Entity<Workspace>,
    cx: &mut VisualTestContext,
    events: &async_channel::Receiver<DesktopEvent>,
    native_events: &async_channel::Sender<DesktopEvent>,
) -> String {
    let id = workspace.read_with(cx, |this, _| this.preview_id.clone().unwrap());
    loop {
        let event = next_event(events);
        let finished = matches!(&event, DesktopEvent::PromptFinished { node_id, result, .. }
            if node_id == &id && { assert!(result.is_ok(), "{result:?}"); true });
        native_events.try_send(event).unwrap();
        cx.run_until_parked();
        if finished {
            break;
        }
    }
    workspace.read_with(cx, |this, _| {
        assert!(!this.editor.active_turns.contains_key(&id));
        this.editor.project.graph.nodes[&id].content.clone()
    })
}

#[gpui::test]
async fn unavailable_provider_stays_disabled_for_keyboard_and_exposes_its_discovery_error(
    cx: &mut TestAppContext,
) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    workspace.update(cx, |this, _| {
        let unavailable = this.providers.iter_mut().find(|s| s.provider == AgentProvider::ClaudeCode).unwrap();
        unavailable.available = false;
        unavailable.error_message = Some("Synthetic discovery failure: Claude Code executable not found. Install Claude Code to use this provider.".into());
    });
    workspace.read_with(cx, |this, _| {
        for provider in AgentProvider::ALL {
            let status = this
                .providers
                .iter()
                .find(|s| s.provider == *provider)
                .unwrap();
            let PopupMenuItem::Item {
                label,
                disabled,
                checked,
                ..
            } = crate::panel::provider_item(
                workspace.downgrade(),
                provider.clone(),
                *provider == this.provider,
                status.available,
            )
            else {
                panic!("provider item must expose its label and availability");
            };
            assert_eq!(disabled, *provider == AgentProvider::ClaudeCode);
            assert_eq!(checked, *provider == AgentProvider::Codex);
            assert_eq!(
                label.as_ref(),
                if disabled {
                    "Claude Code (unavailable)"
                } else {
                    "Codex"
                }
            );
        }
        assert!(this
            .providers
            .iter()
            .find(|s| s.provider == AgentProvider::ClaudeCode)
            .unwrap()
            .error_message
            .as_ref()
            .is_some_and(|error| !error.is_empty()));
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), false, window, cx);
        })
    });
    click(cx, "provider-selector");
    cx.simulate_keystrokes("down enter");
    assert_eq!(
        workspace.read_with(cx, |this, _| this.provider.clone()),
        AgentProvider::Codex
    );
    let statuses = workspace.read_with(cx, |this, _| this.providers.clone());
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| this.open_modal(Modal::Settings, window, cx))
    });
    cx.executor().allow_parking();
    cx.condition(&workspace, |this, _| !this.provider_scan_is_busy())
        .await;
    workspace.update(cx, |this, cx| {
        this.providers = statuses;
        cx.notify();
    });
    assert!(cx.debug_bounds("provider-error-claude-code").is_some());
    assert!(cx.debug_bounds("provider-error-codex").is_none());
    click(cx, "claude-code");
    assert_eq!(
        workspace.read_with(cx, |this, _| this.desktop.config().default_provider),
        AgentProvider::Codex
    );
}

#[gpui::test]
fn persisted_provider_initializes_generation_and_retired_attribution_remains_visible(
    cx: &mut TestAppContext,
) {
    let (_directory, workspace, cx, _events) = workspace(cx);
    let desktop = workspace.read_with(cx, |this, _| this.desktop.clone());
    for provider in AgentProvider::ALL {
        desktop.set_default_provider(provider.clone()).unwrap();
        let (_, events) = async_channel::unbounded();
        let initialized = cx
            .update(|window, cx| cx.new(|cx| Workspace::new(desktop.clone(), events, window, cx)));
        assert_eq!(
            initialized.read_with(cx, |this, _| this.provider.clone()),
            *provider
        );
    }
    workspace.update(cx, |this, _| {
        if let NodeKind::Assistant(data) = &mut this.editor.project.graph.nodes["answer"].kind {
            data.provider = Some(GraphProvider::GeminiCli);
        }
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("answer".into(), false, window, cx);
        })
    });
    assert!(cx.debug_bounds("panel-role-gemini-cli").is_some());
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), false, window, cx);
        })
    });
    assert!(cx.debug_bounds("provider-selector").is_some());
    assert_eq!(AgentProvider::ALL.len(), 2);
}

#[gpui::test]
fn model_catalog_loading_is_per_provider_and_completed_empty_catalogs_are_cached(
    cx: &mut TestAppContext,
) {
    let (_directory, workspace, cx, events) = workspace(cx);
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), false, window, cx);
            this.ensure_models(cx);
        })
    });
    assert!(workspace.read_with(cx, |this, _| this.discovering_models.contains("codex")));
    assert!(cx.debug_bounds("model-selector-loading").is_some());
    events
        .try_send(DesktopEvent::ModelsDiscovered {
            provider: AgentProvider::Codex,
            result: Ok(vec![ModelInfo {
                model_id: "fixture-model".into(),
                display_name: "Deterministic fixture".into(),
            }]),
        })
        .unwrap();
    cx.run_until_parked();
    assert!(workspace.read_with(cx, |this, _| this.discovering_models.is_empty()));
    assert!(cx.debug_bounds("model-selector-ready").is_some());
    workspace.update(cx, |this, cx| {
        // Model discovery has its own provider scope, independent of whether
        // this synthetic test installation can run that provider's CLI.
        this.providers
            .iter_mut()
            .find(|s| s.provider == AgentProvider::ClaudeCode)
            .unwrap()
            .available = true;
        this.selected_model = Some("fixture-model".into());
        cx.notify();
    });
    click(cx, "provider-selector");
    cx.simulate_keystrokes("down enter");
    workspace.read_with(cx, |this, _| {
        assert_eq!(this.provider, AgentProvider::ClaudeCode);
        assert!(this.selected_model.is_none());
        assert!(this.discovering_models.contains("claude-code"));
        assert_eq!(this.models["codex"].len(), 1);
    });
    assert!(cx.debug_bounds("model-selector-loading").is_some());
    events
        .try_send(DesktopEvent::ModelsDiscovered {
            provider: AgentProvider::ClaudeCode,
            result: Ok(vec![]),
        })
        .unwrap();
    cx.run_until_parked();
    click(cx, "provider-selector");
    cx.simulate_keystrokes("down down enter");
    workspace.update(cx, |this, cx| {
        assert_eq!(this.provider, AgentProvider::Codex);
        this.ensure_models(cx);
        assert!(this.discovering_models.is_empty());
        this.select_provider(AgentProvider::ClaudeCode, cx);
        this.ensure_models(cx);
        assert!(this.discovering_models.is_empty());
        assert!(this.models["claude-code"].is_empty());
    });
    assert!(workspace.read_with(cx, |this, _| this.discovering_models.is_empty()));
}

#[gpui::test]
fn native_generation_resolves_explicit_project_and_global_preferences_independently(
    cx: &mut TestAppContext,
) {
    let (directory, workspace, cx, native_events) = workspace(cx);
    let events = replace_desktop(directory.path(), &workspace, cx);
    workspace.update(cx, |this, _| {
        this.editor.project.graph.nodes["question"].content = "fixture:nopermission".into();
        this.desktop
            .set_model_preference(&AgentProvider::Codex, Some("fixture-model".into()))
            .unwrap();
        this.desktop
            .set_effort_preference(&AgentProvider::Codex, Some(ReasoningEffort::Low))
            .unwrap();
        this.editor.project.project_model_preferences = Some(BTreeMap::from([
            ("codex".into(), "fixture-alternate".into()),
            ("gemini-cli".into(), "retired-model-must-not-be-used".into()),
        ]));
        this.editor.project.project_effort_preferences = Some(BTreeMap::from([
            (
                "codex".into(),
                thoughttree_gpui_model::ReasoningEffort::High,
            ),
            (
                "gemini-cli".into(),
                thoughttree_gpui_model::ReasoningEffort::XHigh,
            ),
        ]));
        this.models.insert(
            "codex".into(),
            vec![
                ModelInfo {
                    model_id: "fixture-model".into(),
                    display_name: "Deterministic fixture".into(),
                },
                ModelInfo {
                    model_id: "fixture-alternate".into(),
                    display_name: "Alternate fixture".into(),
                },
            ],
        );
    });
    cx.update(|window, cx| {
        workspace.update(cx, |this, cx| {
            this.preview("question".into(), false, window, cx)
        })
    });
    click(cx, "model-selector");
    cx.simulate_keystrokes("down down enter");
    assert_eq!(
        workspace.read_with(cx, |this, _| this.selected_model.clone()),
        Some("fixture-model".into())
    );
    for (explicit, project, model, effort) in [
        (true, true, "fixture-model", "high"),
        (false, true, "fixture-alternate", "high"),
        (false, false, "fixture-model", "low"),
    ] {
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                if !explicit {
                    this.selected_model = None;
                }
                if !project {
                    this.editor
                        .project
                        .project_model_preferences
                        .as_mut()
                        .unwrap()
                        .remove("codex");
                    this.editor
                        .project
                        .project_effort_preferences
                        .as_mut()
                        .unwrap()
                        .remove("codex");
                }
                this.generate("question".into(), window, cx);
            })
        });
        let text = finish_fixture_turn(&workspace, cx, &events, &native_events);
        assert!(text.contains(&format!("Model: `{model}`")), "{text}");
        assert!(
            text.contains(&format!("Reasoning effort: `{effort}`")),
            "{text}"
        );
        assert!(!text.contains("retired-model-must-not-be-used"));
    }
}
