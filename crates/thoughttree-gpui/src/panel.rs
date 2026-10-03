use std::{sync::Arc, time::Duration};

use base64::Engine;
use gpui::{prelude::*, *};
use gpui_component::{
    button::{Button, ButtonVariants},
    input::Input,
    menu::{DropdownMenu, PopupMenuItem},
    Disableable, Sizable,
};
use thoughttree_core::vault::files::FilePreview;
use thoughttree_desktop::AgentProvider;
use thoughttree_gpui_model::{
    FileTurnReference, GraphNode, NodeKind, ProvenanceCompleteness, Role, TurnActivity,
    TurnProvenance, TurnReference, TurnReferenceRelation,
};

use crate::{theme, workspace::Workspace};

impl Workspace {
    pub(crate) fn panel(&mut self, window: &mut Window, cx: &mut Context<Self>) -> AnyElement {
        let Some(node) = self
            .preview_id
            .as_ref()
            .and_then(|id| self.editor.project.graph.nodes.get(id))
            .cloned()
        else {
            return Empty.into_any_element();
        };
        let streaming = self.editor.active_turns.contains_key(&node.id);
        let max_width = f32::from(window.viewport_size().width) * 0.8;
        let width = self.panel_width.clamp(200., max_width.max(200.));
        div()
            .id("side-panel")
            .debug_selector(|| "side-panel".into())
            .relative()
            .w(px(width))
            .min_w(px(200.))
            .h_full()
            .flex_shrink_0()
            .bg(theme::surface())
            .border_l_1()
            .border_color(theme::border())
            .flex()
            .flex_col()
            .on_drop(cx.listener(|this, paths: &ExternalPaths, _, cx| {
                this.attach_images(paths.paths().to_vec(), cx)
            }))
            .child(panel_resize_events(cx))
            .child(
                div()
                    .id("panel-resize")
                    .debug_selector(|| "panel-resize".into())
                    .absolute()
                    .left(px(-4.))
                    .top_0()
                    .bottom_0()
                    .w(px(8.))
                    .cursor_col_resize()
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(|this, _, _, cx| {
                            this.resizing = true;
                            cx.stop_propagation();
                        }),
                    ),
            )
            .child(self.panel_header(&node, streaming, window, cx))
            .child(self.panel_content(&node, streaming, width, cx))
            .into_any_element()
    }

    fn panel_header(
        &self,
        node: &GraphNode,
        streaming: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let stamp = chrono::DateTime::from_timestamp_millis(node.timestamp as i64)
            .map(|time| {
                time.with_timezone(&chrono::Local)
                    .format("%d %b %Y, %H:%M")
                    .to_string()
            })
            .unwrap_or_default();
        div()
            .flex()
            .flex_col()
            .gap_3()
            .p_4()
            .border_b_1()
            .border_color(theme::border())
            .child(
                div()
                    .flex()
                    .items_center()
                    .justify_between()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .gap_2()
                            .flex_wrap()
                            .child(
                                div()
                                    .id("panel-role")
                                    .debug_selector(|| format!("panel-role-{}", role_label(node)))
                                    .text_xs()
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .bg(theme::raised())
                                    .text_color(theme::accent())
                                    .child(role_label(node)),
                            )
                            .when(streaming, |d| {
                                d.child(
                                    div()
                                        .id("generating-label")
                                        .debug_selector(|| "generating-label".into())
                                        .text_xs()
                                        .text_color(theme::accent())
                                        .child("Generating…"),
                                )
                            })
                            .child(
                                div()
                                    .id("panel-timestamp")
                                    .debug_selector(|| "panel-timestamp".into())
                                    .text_xs()
                                    .text_color(theme::muted())
                                    .child(stamp),
                            ),
                    )
                    .child(
                        Button::new("close-panel")
                            .debug_selector(|| "close-panel".into())
                            .ghost()
                            .small()
                            .label("×")
                            .tooltip("Close preview (Escape)")
                            .on_click(
                                cx.listener(|this, _, window, cx| this.close_panel(window, cx)),
                            ),
                    ),
            )
            .child(self.panel_actions(node, window, cx))
            .into_any_element()
    }

    fn panel_actions(
        &self,
        node: &GraphNode,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> AnyElement {
        let blocked = self.editor.is_node_blocked(&node.id);
        let edit_id = node.id.clone();
        let file_id = node.id.clone();
        let generate_id = node.id.clone();
        let blocker = self.send_blocker(&node.id);
        let mut actions = div().flex().gap_2().items_center().flex_wrap();
        if !node.content.is_empty() && !self.editing {
            actions = actions.child(copy_button(node.content.clone(), window, cx));
        }
        if node.role() == Role::User {
            actions = actions
                .child(
                    Button::new("edit-panel")
                        .debug_selector(|| "edit-panel".into())
                        .small()
                        .label(if self.editing { "Done" } else { "Edit" })
                        .disabled(blocked)
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.toggle_edit(edit_id.clone(), window, cx)
                        })),
                )
                .child(self.generation_controls(blocked, cx))
                .child(
                    Button::new("generate-panel")
                        .debug_selector(|| "generate-panel".into())
                        .primary()
                        .small()
                        .label("Generate")
                        .disabled(
                            blocker.is_some() || !self.editor.project.graph.can_generate(&node.id),
                        )
                        .tooltip(blocker.unwrap_or_else(|| "Generate response (⌘Enter)".into()))
                        .on_click(cx.listener(move |this, _, window, cx| {
                            this.generate(generate_id.clone(), window, cx)
                        })),
                );
        }
        if node.role() == Role::File {
            actions = actions.child(
                Button::new("refresh-file")
                    .small()
                    .label("Reload file")
                    .on_click(
                        cx.listener(move |this, _, _, cx| this.refresh_file(file_id.clone(), cx)),
                    ),
            );
        }
        actions.into_any_element()
    }

    fn panel_content(
        &self,
        node: &GraphNode,
        streaming: bool,
        width: f32,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut content = div()
            .id("panel-scroll")
            .debug_selector(|| "panel-scroll".into())
            .flex_1()
            .min_h_0()
            .overflow_y_scroll()
            .p_5()
            .flex()
            .flex_col()
            .gap_4();
        if self.editing {
            content = content
                .child(self.image_strip(node, cx))
                .child(Input::new(&self.input).appearance(false).w_full())
                .child(
                    Button::new("attach-images")
                        .label("Attach images…")
                        .small()
                        .on_click(cx.listener(|this, _, window, cx| this.pick_images(window, cx))),
                )
                .when(
                    self.mention_open()
                        && self.modal.is_none()
                        && self.permissions.is_empty()
                        && self.palette.is_none(),
                    |d| {
                        d.child(
                            deferred(
                                anchored()
                                    .snap_to_window_with_margin(px(8.))
                                    .child(self.mention_list(width - 40., cx)),
                            )
                            .with_priority(1),
                        )
                    },
                );
        } else if node.role() == Role::File {
            content = content.child(self.file_panel(node));
        } else if streaming {
            content = content.child(
                div()
                    .id("stream-answer")
                    .debug_selector(|| "stream-answer".into())
                    .font_family("Menlo")
                    .text_sm()
                    .child(if node.content.is_empty() {
                        "Waiting for response…".into()
                    } else {
                        node.content.clone()
                    }),
            );
        } else {
            content = content
                .child(self.image_strip(node, cx))
                .child(self.rich.clone())
                .child(self.provenance(node, cx));
        }
        content.into_any_element()
    }

    fn close_panel(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.preview_id = None;
        self.editing = false;
        self.resizing = false;
        self.dismiss_mentions();
        self.focus.focus(window);
        cx.notify();
    }

    fn toggle_edit(&mut self, id: String, window: &mut Window, cx: &mut Context<Self>) {
        if self.editing {
            self.editing = false;
            self.dismiss_mentions();
            self.focus.focus(window);
            cx.notify();
        } else {
            self.preview(id, true, window, cx);
        }
    }

    fn generation_controls(&self, blocked: bool, cx: &Context<Self>) -> AnyElement {
        let workspace = cx.entity().downgrade();
        let statuses = self.providers.clone();
        let current = self.provider.clone();
        let provider = Button::new("provider-selector")
            .debug_selector(|| "provider-selector".into())
            .small()
            .label(self.provider.display_name())
            .disabled(blocked)
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _, _| {
                for provider in AgentProvider::ALL {
                    let status = statuses.iter().find(|status| status.provider == *provider);
                    let available = status.is_some_and(|status| status.available);
                    menu = menu.item(provider_item(
                        workspace.clone(),
                        provider.clone(),
                        *provider == current,
                        available,
                    ));
                }
                menu
            });
        let workspace = cx.entity().downgrade();
        let models = self
            .models
            .get(self.provider.descriptor().id)
            .cloned()
            .unwrap_or_default();
        let selected = self.selected_model.clone();
        let label = selected
            .as_ref()
            .and_then(|id| {
                models
                    .iter()
                    .find(|model| model.model_id == *id)
                    .map(|model| model.display_name.clone())
            })
            .or_else(|| selected.clone())
            .unwrap_or_else(|| "Default model".into());
        let discovering = self
            .discovering_models
            .contains(self.provider.descriptor().id);
        let model = Button::new("model-selector")
            .debug_selector(|| "model-selector".into())
            .small()
            .label(label)
            .loading(discovering)
            .disabled(blocked)
            .dropdown_caret(true)
            .dropdown_menu(move |mut menu, _, _| {
                menu = menu.item(model_item(
                    workspace.clone(),
                    "Default model".into(),
                    None,
                    selected.is_none(),
                ));
                for model in &models {
                    menu = menu.item(model_item(
                        workspace.clone(),
                        model.display_name.clone(),
                        Some(model.model_id.clone()),
                        selected.as_ref() == Some(&model.model_id),
                    ));
                }
                menu
            });
        div()
            .flex()
            .gap_1()
            .child(provider)
            .child(
                div()
                    .id("model-control")
                    .flex()
                    .items_center()
                    .debug_selector(move || {
                        if discovering {
                            "model-selector-loading".into()
                        } else {
                            "model-selector-ready".into()
                        }
                    })
                    .child(model),
            )
            .into_any_element()
    }

    pub(crate) fn select_provider(&mut self, provider: AgentProvider, cx: &mut Context<Self>) {
        // gpui-component 0.5.1 can confirm a disabled menu item by keyboard.
        // Recheck live availability at the mutation boundary as well.
        if !self
            .providers
            .iter()
            .any(|status| status.provider == provider && status.available)
        {
            return;
        }
        self.provider = provider;
        self.selected_model = None;
        self.ensure_models(cx);
        cx.notify();
    }

    fn select_model(&mut self, model: Option<String>, cx: &mut Context<Self>) {
        self.selected_model = model;
        cx.notify();
    }

    fn image_strip(&self, node: &GraphNode, cx: &Context<Self>) -> AnyElement {
        let images = node
            .images()
            .iter()
            .enumerate()
            .filter_map(|(index, image)| {
                let image = decoded_image(&image.data, &image.mime_type)?;
                let id = node.id.clone();
                let card = div()
                    .id(SharedString::from(format!("attachment-{index}")))
                    .debug_selector(move || format!("attachment-{index}"))
                    .relative()
                    .w(px(100.))
                    .h(px(90.))
                    .rounded_md()
                    .overflow_hidden()
                    .child(img(image).size_full().object_fit(ObjectFit::Contain))
                    .when(self.editing, |d| {
                        d.child(
                            Button::new(SharedString::from(format!("remove-image-{index}")))
                                .debug_selector(move || format!("remove-image-{index}"))
                                .absolute()
                                .top_0()
                                .right_0()
                                .small()
                                .label("×")
                                .on_click(cx.listener(move |this, _, _, cx| {
                                    this.remove_image(&id, index, cx)
                                })),
                        )
                    });
                Some(card.into_any_element())
            });
        div()
            .flex()
            .flex_wrap()
            .gap_2()
            .children(images)
            .into_any_element()
    }

    fn remove_image(&mut self, id: &str, index: usize, cx: &mut Context<Self>) {
        if !self.editing || self.editor.is_node_blocked(id) {
            return;
        }
        self.change(
            |project| {
                let Some(NodeKind::User(data)) =
                    project.graph.nodes.get_mut(id).map(|node| &mut node.kind)
                else {
                    return;
                };
                if index < data.images.len() {
                    data.images.remove(index);
                }
            },
            cx,
        );
    }

    fn file_panel(&self, node: &GraphNode) -> AnyElement {
        let Some(file) = node.file_data() else {
            return Empty.into_any_element();
        };
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_3()
            .child(
                div()
                    .id("file-name")
                    .debug_selector(|| "file-name".into())
                    .text_lg()
                    .child(file.name.clone()),
            )
            .child(
                div()
                    .id("file-metadata")
                    .debug_selector(|| "file-metadata".into())
                    .text_sm()
                    .text_color(theme::muted())
                    .child(format!(
                        "{} · {} bytes\n{}",
                        file.mime_type, file.size, file.path
                    )),
            );
        let warning = match self.file_status.get(&node.id) {
            None => Some("Checking file…".into()),
            Some(Ok(thoughttree_desktop::VaultFileStatus::Ok { stat, .. }))
                if stat.modified_epoch_ms != file.seen_mtime || stat.size != file.seen_size =>
            {
                Some("Changed on disk — Reload file to adopt this version".into())
            }
            Some(Ok(thoughttree_desktop::VaultFileStatus::Missing)) => {
                Some("File missing — restore the file or remove this node".into())
            }
            Some(Ok(thoughttree_desktop::VaultFileStatus::Invalid)) => {
                Some("File is outside the notes directory".into())
            }
            Some(Err(error)) => Some(error.clone()),
            _ => None,
        };
        if let Some(warning) = warning {
            panel = panel.child(warning_text(warning));
        }
        panel
            .child(file_preview(self.file_preview.as_ref()))
            .into_any_element()
    }

    fn provenance(&self, node: &GraphNode, cx: &Context<Self>) -> AnyElement {
        let Some(provenance) = node.provenance() else {
            return Empty.into_any_element();
        };
        let details = ProvenanceDetails::new(provenance, &node.content);
        let label = format!(
            "{} Provenance · {} · {} · {}",
            disclosure(self.activity_expanded),
            count_label(details.source_count, "source", "sources"),
            count_label(details.file_count, "file", "files"),
            count_label(provenance.activity.len(), "activity", "activities")
        );
        let mut panel = div()
            .flex()
            .flex_col()
            .gap_3()
            .border_t_1()
            .border_color(theme::border())
            .pt_4()
            .child(
                Button::new("provenance-toggle")
                    .debug_selector(|| "provenance-toggle".into())
                    .ghost()
                    .small()
                    .label(label)
                    .on_click(cx.listener(|this, _, _, cx| {
                        this.activity_expanded = !this.activity_expanded;
                        cx.notify();
                    })),
            );
        if !self.activity_expanded {
            return panel.into_any_element();
        }
        if provenance.completeness != ProvenanceCompleteness::Complete {
            panel = panel.child(
                warning_text("Some Turn evidence may be missing.".into())
                    .id("provenance-completeness")
                    .debug_selector(|| "provenance-completeness".into()),
            );
        }
        panel = panel.child(section_label("References"));
        for (index, reference) in details.references.iter().enumerate() {
            panel = panel.child(reference_row(index, reference, &details.cited_indexes));
        }
        for index in details.missing_indexes {
            panel = panel.child(
                warning_text(format!(
                    "Citation marker 【{index}】 has no matching reference."
                ))
                .id(SharedString::from(format!("citation-missing-{index}")))
                .debug_selector(move || format!("citation-missing-{index}")),
            );
        }
        if provenance.references.is_empty() {
            panel = panel.child(
                muted_text("No references recorded.")
                    .id("no-references")
                    .debug_selector(|| "no-references".into()),
            );
        }
        panel = panel.child(section_label("Turn activity"));
        for (index, activity) in provenance.activity.iter().enumerate() {
            panel = panel.child(self.activity_row(index, activity, cx));
        }
        if provenance.activity.is_empty() {
            panel = panel.child(
                muted_text("No Turn activity recorded.")
                    .id("no-activity")
                    .debug_selector(|| "no-activity".into()),
            );
        }
        panel.into_any_element()
    }

    fn activity_row(
        &self,
        index: usize,
        activity: &TurnActivity,
        cx: &Context<Self>,
    ) -> AnyElement {
        if let TurnActivity::Unknown {
            provider_type,
            label,
            ..
        } = activity
        {
            return muted_text(&format!("{provider_type} · {label}"))
                .id(SharedString::from(format!("activity-{index}")))
                .debug_selector(move || format!("activity-{index}"))
                .into_any_element();
        }
        let key = index.to_string();
        let open = self.raw_expanded.contains(&key);
        let (title, detail) = activity_details(activity);
        div()
            .flex()
            .flex_col()
            .gap_2()
            .child(
                Button::new(SharedString::from(format!("activity-{key}")))
                    .debug_selector(|| format!("activity-{key}"))
                    .small()
                    .ghost()
                    .label(format!("{} {title}", disclosure(open)))
                    .on_click(cx.listener(move |this, _, _, cx| this.toggle_activity(&key, cx))),
            )
            .when(open, |d| {
                d.child(
                    muted_text(&detail)
                        .id(SharedString::from(format!("activity-detail-{index}")))
                        .debug_selector(move || format!("activity-detail-{index}")),
                )
            })
            .into_any_element()
    }

    fn toggle_activity(&mut self, key: &str, cx: &mut Context<Self>) {
        if !self.raw_expanded.remove(key) {
            self.raw_expanded.insert(key.to_owned());
        }
        cx.notify();
    }

    fn mention_list(&self, width: f32, cx: &Context<Self>) -> AnyElement {
        let mut list = div()
            .id("file-completion")
            .debug_selector(|| "file-completion".into())
            .max_h(px(240.))
            .w(px(width.max(160.)))
            .occlude()
            .overflow_y_scroll()
            .track_scroll(&self.mention_state.scroll)
            .flex()
            .flex_col()
            .rounded_md()
            .border_1()
            .border_color(theme::border())
            .bg(theme::raised());
        if self.mention_state.loading {
            return list
                .child(div().p_2().child("Searching…"))
                .into_any_element();
        }
        if let Some(error) = &self.mention_state.error {
            return list
                .child(div().p_2().child(warning_text(error.clone())))
                .into_any_element();
        }
        if self.mentions.is_empty() {
            return list
                .child(div().p_2().child("No files found"))
                .into_any_element();
        }
        for (index, path) in self.mentions.iter().enumerate() {
            list = list.child(
                div()
                    .id(SharedString::from(format!("mention-{index}")))
                    .debug_selector(move || format!("mention-{index}"))
                    .p_2()
                    .flex_shrink_0()
                    .truncate()
                    .cursor_pointer()
                    .when(index == self.mention_index, |d| d.bg(theme::border()))
                    .child(path.clone())
                    .on_mouse_move(cx.listener(move |this, _, _, cx| {
                        if this.mention_index != index {
                            this.mention_index = index;
                            cx.notify();
                        }
                    }))
                    .on_mouse_down(
                        MouseButton::Left,
                        cx.listener(move |this, _, window, cx| {
                            cx.stop_propagation();
                            this.mention_index = index;
                            this.accept_mention(window, cx);
                        }),
                    ),
            );
        }
        list.into_any_element()
    }

    pub(crate) fn render_permission(&self, cx: &Context<Self>) -> AnyElement {
        let Some(permission) = self.permissions.front().cloned() else {
            return Empty.into_any_element();
        };
        let options = permission.options.iter().map(|option| {
            let id = option.id.clone();
            let request = permission.request_id.clone();
            Button::new(SharedString::from(format!("permission-{id}")))
                .label(option.label.clone())
                .on_click(
                    cx.listener(move |this, _, _, cx| this.answer_permission(&request, &id, cx)),
                )
                .into_any_element()
        });
        div()
            .absolute()
            .inset_0()
            .bg(rgba(0x00000099))
            .flex()
            .items_center()
            .justify_center()
            .child(
                div()
                    .w(px(540.))
                    .max_h(px(650.))
                    .p_6()
                    .rounded_lg()
                    .bg(theme::surface())
                    .border_1()
                    .border_color(theme::border())
                    .flex()
                    .flex_col()
                    .gap_4()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .justify_between()
                            .child(div().text_xl().child("Permission requested"))
                            .child(
                                div()
                                    .text_xs()
                                    .px_2()
                                    .py_1()
                                    .rounded_sm()
                                    .bg(theme::raised())
                                    .child(permission.tool_type),
                            ),
                    )
                    .child(
                        div()
                            .text_color(theme::accent())
                            .child(permission.tool_name),
                    )
                    .child(
                        div()
                            .id("permission-description")
                            .max_h(px(380.))
                            .overflow_y_scroll()
                            .child(permission.description),
                    )
                    .child(div().flex().flex_wrap().gap_2().children(options)),
            )
            .into_any_element()
    }

    fn answer_permission(&mut self, request: &str, option: &str, cx: &mut Context<Self>) {
        // A queued request must not consume a newer front request after a rerender.
        if !self
            .permissions
            .front()
            .is_some_and(|pending| pending.request_id == request)
        {
            return;
        }
        self.desktop
            .respond_to_permission(request.to_owned(), option.to_owned());
        self.permissions.pop_front();
        cx.notify();
    }

    fn resize_panel(
        &mut self,
        event: &MouseMoveEvent,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.resizing || !event.dragging() {
            return;
        }
        let viewport = f32::from(window.viewport_size().width);
        self.panel_width =
            (viewport - f32::from(event.position.x)).clamp(200., (viewport * 0.8).max(200.));
        cx.stop_propagation();
        cx.notify();
    }
}

pub(crate) fn provider_item(
    workspace: WeakEntity<Workspace>,
    provider: AgentProvider,
    selected: bool,
    available: bool,
) -> PopupMenuItem {
    let label = format!(
        "{}{}",
        provider.display_name(),
        if available { "" } else { " (unavailable)" }
    );
    PopupMenuItem::new(label)
        .checked(selected)
        .disabled(!available)
        .on_click(move |_, _, cx| {
            let _ = workspace.update(cx, |this, cx| this.select_provider(provider.clone(), cx));
        })
}

fn model_item(
    workspace: WeakEntity<Workspace>,
    label: String,
    model: Option<String>,
    selected: bool,
) -> PopupMenuItem {
    PopupMenuItem::new(label)
        .checked(selected)
        .on_click(move |_, _, cx| {
            let _ = workspace.update(cx, |this, cx| this.select_model(model.clone(), cx));
        })
}

fn panel_resize_events(cx: &Context<Workspace>) -> AnyElement {
    let workspace = cx.entity().downgrade();
    canvas(
        |_, _, _| (),
        move |_, _, window, _| {
            let resize = workspace.clone();
            window.on_mouse_event(move |event: &MouseMoveEvent, phase, window, cx| {
                if !phase.capture() {
                    return;
                }
                let _ = resize.update(cx, |this, cx| this.resize_panel(event, window, cx));
            });
            let release = workspace.clone();
            window.on_mouse_event(move |_: &MouseUpEvent, phase, _, cx| {
                if !phase.capture() {
                    return;
                }
                let _ = release.update(cx, |this, _| this.resizing = false);
            });
        },
    )
    .absolute()
    .size_full()
    .into_any_element()
}

struct CopyFeedback {
    source: String,
    copied: bool,
    reset: Option<Task<()>>,
}

fn copy_button(source: String, window: &mut Window, cx: &mut Context<Workspace>) -> Button {
    let feedback = window.use_keyed_state("panel-copy-feedback", cx, |_, _| CopyFeedback {
        source: String::new(),
        copied: false,
        reset: None,
    });
    let copied = feedback.read(cx).copied && feedback.read(cx).source == source;
    Button::new("copy-node")
        .debug_selector(|| "copy-node".into())
        .small()
        .label(if copied { "Copied!" } else { "Copy" })
        .tooltip("Copy source Markdown")
        .on_click(move |_, _, cx| {
            cx.write_to_clipboard(ClipboardItem::new_string(source.clone()));
            feedback.update(cx, |state, cx| {
                state.source = source.clone();
                state.copied = true;
                state.reset = Some(cx.spawn(async move |state, cx| {
                    cx.background_executor().timer(Duration::from_secs(2)).await;
                    let _ = state.update(cx, |state, cx| {
                        state.copied = false;
                        cx.notify();
                    });
                }));
                cx.notify();
            });
        })
}

fn decoded_image(data: &str, mime_type: &str) -> Option<Arc<Image>> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data)
        .ok()?;
    let format = ImageFormat::from_mime_type(mime_type)?;
    Some(Arc::new(Image::from_bytes(format, bytes)))
}

fn file_preview(
    preview: Option<&Result<thoughttree_core::vault::files::FilePreviewResponse, String>>,
) -> AnyElement {
    match preview {
        None => muted_text("Loading preview…").into_any_element(),
        Some(Err(error)) => warning_text(error.clone()).into_any_element(),
        Some(Ok(response)) => match &response.preview {
            FilePreview::Text { excerpt, truncated } => div()
                .id("file-preview-text")
                .debug_selector(|| "file-preview-text".into())
                .flex()
                .flex_col()
                .gap_2()
                .child(div().font_family("Menlo").text_sm().child(excerpt.clone()))
                .when(*truncated, |d| {
                    d.child(muted_text(
                        "Preview truncated to 16 KB. The agent can read the full file.",
                    ))
                })
                .into_any_element(),
            FilePreview::Image {
                data, mime_type, ..
            } => div()
                .id("file-preview-image")
                .debug_selector(|| "file-preview-image".into())
                .when_some(decoded_image(data, mime_type), |d, image| {
                    d.child(
                        img(image)
                            .w_full()
                            .max_h(px(540.))
                            .object_fit(ObjectFit::Contain),
                    )
                })
                .into_any_element(),
            FilePreview::None => {
                muted_text("No preview for this file type. The agent receives a file pointer.")
                    .into_any_element()
            }
        },
    }
}

struct ProvenanceDetails<'a> {
    references: Vec<&'a TurnReference>,
    cited_indexes: Vec<f64>,
    missing_indexes: Vec<f64>,
    source_count: usize,
    file_count: usize,
}

impl<'a> ProvenanceDetails<'a> {
    fn new(provenance: &'a TurnProvenance, content: &str) -> Self {
        let mut references: Vec<_> = provenance.references.iter().collect();
        references.sort_by(
            |left, right| match (reference_index(left), reference_index(right)) {
                (Some(left), Some(right)) => left.total_cmp(&right),
                (Some(_), None) => std::cmp::Ordering::Less,
                (None, Some(_)) => std::cmp::Ordering::Greater,
                (None, None) => std::cmp::Ordering::Equal,
            },
        );
        let mut cited_indexes: Vec<f64> = content
            .split('【')
            .skip(1)
            .filter_map(|tail| tail.split_once('】').map(|(index, _)| index))
            .filter(|index| !index.is_empty() && index.bytes().all(|byte| byte.is_ascii_digit()))
            .filter_map(|index| index.parse::<f64>().ok())
            .filter(|index| index.is_finite())
            .collect();
        cited_indexes.sort_by(f64::total_cmp);
        cited_indexes.dedup();
        let missing_indexes = cited_indexes
            .iter()
            .copied()
            .filter(|index| {
                !references
                    .iter()
                    .any(|reference| reference_index(reference) == Some(*index))
            })
            .collect();
        let source_count = references
            .iter()
            .filter(|reference| matches!(reference, TurnReference::Url { .. }))
            .count();
        Self {
            references,
            cited_indexes,
            missing_indexes,
            source_count,
            file_count: provenance.references.len() - source_count,
        }
    }
}

fn reference_index(reference: &TurnReference) -> Option<f64> {
    match reference {
        TurnReference::Url { index, .. } => *index,
        _ => None,
    }
}

fn reference_row(index: usize, reference: &TurnReference, cited_indexes: &[f64]) -> AnyElement {
    let mut row = div().flex().flex_col().gap_1();
    match reference {
        TurnReference::Url {
            url,
            title,
            relations,
            index: reference_index,
            is_search_result,
            ..
        } => {
            if let Some(index) = reference_index {
                row = row.child(muted_text(&format!(
                    "Reference {index} · {} · {}",
                    if cited_indexes.contains(index) {
                        "Cited"
                    } else {
                        "Consulted"
                    },
                    if *is_search_result == Some(true) {
                        "Search result"
                    } else {
                        "Fetched page"
                    }
                )));
            }
            let label = title
                .as_ref()
                .filter(|title| !title.is_empty())
                .unwrap_or(url)
                .clone();
            let safe_url = thoughttree_gpui_model::is_web_url(url).then(|| url.clone());
            let debug_url = url.clone();
            row = row.child(
                div()
                    .id(SharedString::from(format!("reference-{index}")))
                    .debug_selector(move || format!("reference-{debug_url}"))
                    .child(label)
                    .when_some(safe_url, |d, url| {
                        d.text_color(theme::accent())
                            .cursor_pointer()
                            .on_click(move |_, _, cx| cx.open_url(&url))
                    }),
            );
            if title.as_ref().is_some_and(|title| !title.is_empty()) {
                row = row.child(muted_text(url));
            }
            row = row.child(muted_text(&relation_label(relations)));
        }
        TurnReference::File(FileTurnReference::Vault {
            path, relations, ..
        }) => {
            row = row
                .child(path.clone())
                .child(muted_text(&relation_label(relations)))
        }
        TurnReference::File(FileTurnReference::External {
            display_name,
            relations,
            ..
        }) => {
            row = row
                .child(display_name.clone())
                .child(muted_text(&relation_label(relations)))
        }
    }
    row.into_any_element()
}

fn relation_label(relations: &[TurnReferenceRelation]) -> String {
    relations
        .iter()
        .map(|relation| match relation {
            TurnReferenceRelation::Consulted => "consulted",
            TurnReferenceRelation::Cited => "cited",
            TurnReferenceRelation::Read => "read",
            TurnReferenceRelation::Created => "created",
            TurnReferenceRelation::Updated => "updated",
            TurnReferenceRelation::Deleted => "deleted",
            TurnReferenceRelation::Moved => "moved",
            TurnReferenceRelation::Searched => "searched",
            TurnReferenceRelation::Fetched => "fetched",
        })
        .collect::<Vec<_>>()
        .join(" · ")
}

fn activity_details(activity: &TurnActivity) -> (String, String) {
    match activity {
        TurnActivity::Commentary { content, .. } => {
            ("Assistant commentary".into(), content.clone())
        }
        TurnActivity::Tool {
            kind,
            status,
            title,
            title_truncated,
            title_redacted,
            ..
        } => (
            format!("{kind:?} · {status:?}"),
            format!(
                "{}{}{}",
                title.chars().take(200).collect::<String>(),
                if *title_truncated == Some(true) || title.chars().count() > 200 {
                    "\nTitle truncated"
                } else {
                    ""
                },
                if *title_redacted == Some(true) {
                    "\nTitle replaced by a summary"
                } else {
                    ""
                }
            ),
        ),
        TurnActivity::Unknown {
            provider_type,
            label,
            ..
        } => (provider_type.clone(), label.clone()),
    }
}

fn disclosure(open: bool) -> &'static str {
    if open {
        "▾"
    } else {
        "▸"
    }
}
fn count_label(count: usize, singular: &str, plural: &str) -> String {
    format!("{count} {}", if count == 1 { singular } else { plural })
}
fn muted_text(text: &str) -> Div {
    div()
        .text_xs()
        .text_color(theme::muted())
        .child(text.to_owned())
}
fn warning_text(text: String) -> Div {
    div().text_sm().text_color(theme::danger()).child(text)
}
fn section_label(label: &str) -> Div {
    div()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(label.to_owned())
}
fn role_label(node: &GraphNode) -> String {
    match node.role() {
        Role::User => "User".into(),
        Role::Assistant => node
            .assistant_data()
            .and_then(|assistant| assistant.provider)
            .map(|provider| provider.as_str())
            .unwrap_or("Assistant")
            .into(),
        Role::File => node
            .file_data()
            .and_then(|file| file.name.rsplit('.').next())
            .unwrap_or("FILE")
            .to_uppercase(),
    }
}

#[cfg(test)]
mod tests {
    use super::{activity_details, reference_index, ProvenanceDetails};
    use gpui::{px, size, Modifiers, TestAppContext, VisualTestContext};
    use thoughttree_gpui_model::{TurnActivity, TurnProvenance};

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let bounds = cx.debug_bounds(selector).expect(selector);
        cx.simulate_click(bounds.center(), Modifiers::default());
        cx.run_until_parked();
    }

    #[gpui::test]
    fn native_editor_buttons_keep_unicode_caret_edits_and_selection(cx: &mut TestAppContext) {
        let (_directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        workspace.update(cx, |this, _| {
            this.editor.project.graph.nodes["question"].content = "First line\n研究".into();
            this.editor.selection.insert("question".into());
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("question".into(), false, window, cx)
            });
        });
        cx.run_until_parked();
        let copy_width = cx.debug_bounds("copy-node").unwrap().size.width;
        click(cx, "copy-node");
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("First line\n研究")
        );
        assert!(cx.debug_bounds("copy-node").unwrap().size.width > copy_width);
        cx.executor()
            .advance_clock(std::time::Duration::from_secs(2));
        cx.run_until_parked();
        assert_eq!(cx.debug_bounds("copy-node").unwrap().size.width, copy_width);
        click(cx, "edit-panel");
        assert!(workspace.read_with(cx, |this, _| this.editing));
        workspace.read_with(cx, |this, cx| {
            assert_eq!(this.input.read(cx).cursor(), "First line\n研究".len());
        });
        cx.simulate_input("追加\nThird line");
        assert_eq!(
            workspace.read_with(cx, |this, _| this.editor.project.graph.nodes["question"]
                .content
                .clone()),
            "First line\n研究追加\nThird line"
        );
        cx.simulate_keystrokes("cmd-a");
        cx.simulate_input("Replacement 日本語");
        click(cx, "edit-panel");
        assert!(!workspace.read_with(cx, |this, _| this.editing));
        assert_eq!(
            workspace.read_with(cx, |this, _| this.editor.project.graph.nodes["question"]
                .content
                .clone()),
            "Replacement 日本語"
        );
        click(cx, "close-panel");
        workspace.read_with(cx, |this, _| {
            assert!(this.preview_id.is_none());
            assert!(this.editor.selection.contains("question"));
        });
    }

    #[gpui::test]
    fn native_file_completion_pointer_tab_cap_and_bounds_preserve_surrounding_text(
        cx: &mut TestAppContext,
    ) {
        let (directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        for index in 0..20 {
            std::fs::write(
                directory.path().join(format!("vault/ref-{index:02}.md")),
                "Reference",
            )
            .unwrap();
        }
        cx.simulate_resize(size(px(1000.), px(540.)));
        workspace.update(cx, |this, _| {
            this.editor.project.graph.nodes["question"].content = "Préface  suffix".into();
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("question".into(), true, window, cx);
                this.input.update(cx, |input, cx| {
                    input.set_cursor_position(
                        gpui_component::input::Position::new(0, 8),
                        window,
                        cx,
                    )
                });
            });
        });
        cx.simulate_input("@ref");
        assert!(workspace.read_with(cx, |this, _| this.mention_state.loading));
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
        let mentions = workspace.read_with(cx, |this, _| this.mentions.clone());
        assert_eq!(mentions.len(), 15);
        let bounds = cx
            .debug_bounds("file-completion")
            .expect("completion popup");
        assert!(bounds.size.height >= px(200.), "{bounds:?}");
        assert!(
            bounds.top() >= px(0.) && bounds.bottom() <= px(540.),
            "{bounds:?}"
        );
        assert!(
            bounds.left() >= px(0.) && bounds.right() <= px(1000.),
            "{bounds:?}"
        );
        click(cx, "mention-2");
        assert_eq!(
            workspace.read_with(cx, |this, _| this.editor.project.graph.nodes["question"]
                .content
                .clone()),
            format!("Préface @/{} suffix", mentions[2])
        );
        cx.simulate_keystrokes("cmd-a");
        cx.simulate_input("@ref");
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
        cx.simulate_keystrokes("down down up tab");
        assert_eq!(
            workspace.read_with(cx, |this, _| this.editor.project.graph.nodes["question"]
                .content
                .clone()),
            format!("@/{}", mentions[1])
        );
        assert!(!workspace.read_with(cx, |this, _| this.mention_open()));
        cx.simulate_keystrokes("cmd-a");
        cx.simulate_input("@ref");
        cx.executor()
            .advance_clock(std::time::Duration::from_millis(100));
        cx.run_until_parked();
        let hovered = cx.debug_bounds("mention-3").unwrap().center();
        cx.simulate_mouse_move(hovered, None, gpui::Modifiers::default());
        cx.simulate_keystrokes("enter");
        assert_eq!(
            workspace.read_with(cx, |this, _| this.editor.project.graph.nodes["question"]
                .content
                .clone()),
            format!("@/{}", mentions[3])
        );
    }

    #[gpui::test]
    fn native_image_drop_route_displays_a_thumbnail_and_removal_is_persistent(
        cx: &mut TestAppContext,
    ) {
        let (directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        let path = directory.path().join("dropped.png");
        image::RgbImage::from_pixel(40, 20, image::Rgb([20, 80, 100]))
            .save(&path)
            .unwrap();
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("question".into(), true, window, cx);
                this.attach_images(vec![path], cx);
            });
        });
        cx.run_until_parked();
        let bounds = cx
            .debug_bounds("attachment-0")
            .expect("decoded image thumbnail");
        assert!(bounds.size.width > px(0.) && bounds.size.height > px(0.));
        assert_eq!(
            workspace.read_with(cx, |this, _| this.editor.project.graph.nodes["question"]
                .images()
                .len()),
            1
        );
        click(cx, "remove-image-0");
        workspace.read_with(cx, |this, _| {
            let saved =
                thoughttree_gpui_model::Project::from_json(&this.editor.project.to_json().unwrap())
                    .unwrap();
            assert!(saved.graph.nodes["question"].images().is_empty());
            assert!(this.editor.is_dirty());
        });
    }

    #[gpui::test]
    fn native_provenance_renders_warning_order_and_independent_tool_disclosures(
        cx: &mut TestAppContext,
    ) {
        let (_directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        cx.simulate_resize(size(px(1440.), px(1500.)));
        workspace.update(cx, |this, _| {
            let node = &mut this.editor.project.graph.nodes["answer"];
            node.content = "Answer 【1】 with missing citation 【9】.".into();
            let thoughttree_gpui_model::NodeKind::Assistant(assistant) = &mut node.kind else { unreachable!() };
            assistant.provenance = thoughttree_gpui_model::normalize_provenance(&serde_json::json!({
                "completeness":"partial",
                "references":[
                    {"type":"url","url":"https://example.org/second","index":2,"relations":["consulted"]},
                    {"type":"url","url":"https://example.org/unindexed","relations":["fetched"]},
                    {"type":"url","url":"https://example.org/first","index":1,"relations":["cited"]}
                ],
                "activity":[
                    {"type":"commentary","content":"First activity"},
                    {"type":"tool","kind":"search","status":"completed","title":"Second activity","titleTruncated":true,"titleRedacted":true},
                    {"type":"unknown","providerType":"historical","label":"Third activity"}
                ]
            }));
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("answer".into(), false, window, cx)
            })
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("provenance-completeness").is_none());
        click(cx, "provenance-toggle");
        assert!(cx.debug_bounds("provenance-completeness").is_some());
        assert!(cx.debug_bounds("citation-missing-9").is_some());
        assert!(cx.debug_bounds("citation-missing-1").is_none());
        let first = cx
            .debug_bounds("reference-https://example.org/first")
            .unwrap();
        let second = cx
            .debug_bounds("reference-https://example.org/second")
            .unwrap();
        let unindexed = cx
            .debug_bounds("reference-https://example.org/unindexed")
            .unwrap();
        assert!(first.top() < second.top() && second.top() < unindexed.top());
        click(cx, "reference-https://example.org/first");
        assert_eq!(
            cx.opened_url().as_deref(),
            Some("https://example.org/first")
        );
        let commentary = cx.debug_bounds("activity-0").unwrap();
        let tool = cx.debug_bounds("activity-1").unwrap();
        let unknown = cx.debug_bounds("activity-2").unwrap();
        assert!(commentary.top() < tool.top() && tool.top() < unknown.top());
        click(cx, "activity-0");
        click(cx, "activity-1");
        assert!(cx.debug_bounds("activity-detail-0").is_some());
        assert!(cx.debug_bounds("activity-detail-1").is_some());
        click(cx, "activity-0");
        workspace.read_with(cx, |this, _| {
            assert!(!this.raw_expanded.contains("0"));
            assert!(this.raw_expanded.contains("1"));
        });
        for completeness in ["unknown", "complete"] {
            workspace.update(cx, |this, _| {
                let node = &mut this.editor.project.graph.nodes["answer"];
                node.content = "Answer without citation markers.".into();
                let thoughttree_gpui_model::NodeKind::Assistant(assistant) = &mut node.kind else {
                    unreachable!()
                };
                assistant.provenance = thoughttree_gpui_model::normalize_provenance(
                    &serde_json::json!({"completeness":completeness,"references":[],"activity":[]}),
                );
            });
            cx.update(|window, cx| {
                workspace.update(cx, |this, cx| {
                    this.preview("answer".into(), false, window, cx)
                })
            });
            cx.run_until_parked();
            click(cx, "provenance-toggle");
            if completeness == "unknown" {
                assert!(cx.debug_bounds("provenance-completeness").is_some());
            }
            assert!(cx.debug_bounds("no-references").is_some());
            assert!(cx.debug_bounds("no-activity").is_some());
        }
    }

    #[gpui::test]
    fn native_complete_empty_provenance_has_no_incomplete_warning(cx: &mut TestAppContext) {
        let (_directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        workspace.update(cx, |this, _| {
            let node = &mut this.editor.project.graph.nodes["answer"];
            let thoughttree_gpui_model::NodeKind::Assistant(assistant) = &mut node.kind else {
                unreachable!()
            };
            assistant.provenance = thoughttree_gpui_model::normalize_provenance(
                &serde_json::json!({"completeness":"complete","references":[],"activity":[]}),
            );
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("answer".into(), false, window, cx)
            })
        });
        cx.run_until_parked();
        click(cx, "provenance-toggle");
        assert!(cx.debug_bounds("no-references").is_some());
        assert!(cx.debug_bounds("no-activity").is_some());
        assert!(cx.debug_bounds("provenance-completeness").is_none());
    }

    #[gpui::test]
    fn native_long_answer_scroll_does_not_pan_or_zoom_the_graph(cx: &mut TestAppContext) {
        let (_directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        workspace.update(cx, |this, _| {
            this.editor.project.graph.nodes["answer"].content = (0..60).map(|index| format!("Paragraph {index}: This is a long answer that must scroll inside its panel.")).collect::<Vec<_>>().join("\n\n");
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("answer".into(), false, window, cx)
            })
        });
        cx.run_until_parked();
        let graph_before = workspace.read_with(cx, |this, cx| this.canvas.read(cx).viewport);
        let before = cx.debug_bounds("rich-answer").unwrap();
        let scroll = cx.debug_bounds("panel-scroll").unwrap();
        cx.simulate_event(gpui::ScrollWheelEvent {
            position: scroll.center(),
            delta: gpui::ScrollDelta::Pixels(gpui::point(px(0.), px(-220.))),
            ..Default::default()
        });
        cx.run_until_parked();
        let after = cx.debug_bounds("rich-answer").unwrap();
        assert!(
            after.top() < before.top(),
            "before={before:?}, after={after:?}"
        );
        workspace.read_with(cx, |this, cx| {
            let graph_after = this.canvas.read(cx).viewport;
            assert_eq!(graph_before.pan, graph_after.pan);
            assert_eq!(graph_before.zoom, graph_after.zoom);
        });
    }

    #[gpui::test]
    fn native_stream_completion_replaces_plain_content_with_selectable_rich_content(
        cx: &mut TestAppContext,
    ) {
        let (_directory, workspace, cx, events) = crate::interaction_tests::workspace(cx);
        workspace.update(cx, |this, _| {
            this.editor.project.graph.nodes["answer"].content.clear();
            this.editor.start_turn("answer", "render-turn").unwrap();
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.preview("answer".into(), false, window, cx)
            })
        });
        cx.run_until_parked();
        assert!(cx.debug_bounds("stream-answer").is_some());
        assert!(cx.debug_bounds("rich-answer").is_none());
        let source = "**Live** answer";
        events
            .try_send(thoughttree_desktop::DesktopEvent::StreamChunk(
                thoughttree_core::events::StreamChunkEvent {
                    node_id: "answer".into(),
                    turn_id: "render-turn".into(),
                    chunk: source.into(),
                },
            ))
            .unwrap();
        events
            .try_send(thoughttree_desktop::DesktopEvent::PromptFinished {
                node_id: "answer".into(),
                turn_id: "render-turn".into(),
                result: Ok(source.into()),
            })
            .unwrap();
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert!(!this.editor.active_turns.contains_key("answer"));
            assert_eq!(this.editor.project.graph.nodes["answer"].content, source);
        });
        let bounds = cx
            .debug_bounds("rich-answer")
            .expect("completed rich content");
        let start = bounds.origin + gpui::point(px(1.), px(8.));
        let end = bounds.origin + gpui::point(px(250.), px(18.));
        cx.simulate_mouse_down(start, gpui::MouseButton::Left, Modifiers::default());
        cx.simulate_mouse_move(end, Some(gpui::MouseButton::Left), Modifiers::default());
        cx.simulate_mouse_up(end, gpui::MouseButton::Left, Modifiers::default());
        cx.simulate_keystrokes("cmd-c");
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("Live answer")
        );
    }

    #[::core::prelude::v1::test]
    fn citations_sort_numeric_references_keep_unindexed_order_and_report_only_missing_markers() {
        let provenance:TurnProvenance=serde_json::from_value(serde_json::json!({
            "completeness":"partial",
            "references":[
                {"type":"file","scope":"vault","path":"notes.md","displayName":"notes.md","relations":["read"]},
                {"type":"url","url":"https://example.org/second","index":2,"relations":["consulted"]},
                {"type":"url","url":"https://example.org/unindexed","relations":["fetched"]},
                {"type":"url","url":"https://example.org/first","index":1,"relations":["cited"]}
            ],
            "activity":[]
        })).unwrap();
        let details = ProvenanceDetails::new(
            &provenance,
            "Cited 【9】, 【01】 and 【3】. Repeat 【9】. Invalid 【-1】 【2.5】 【no】 【】.",
        );
        assert_eq!(
            details
                .references
                .iter()
                .map(|reference| reference_index(reference))
                .collect::<Vec<_>>(),
            vec![Some(1.), Some(2.), None, None]
        );
        assert_eq!(details.references[2], &provenance.references[0]);
        assert_eq!(details.references[3], &provenance.references[2]);
        assert_eq!(details.cited_indexes, vec![1., 3., 9.]);
        assert_eq!(details.missing_indexes, vec![3., 9.]);
        assert_eq!((details.source_count, details.file_count), (3, 1));
    }

    #[::core::prelude::v1::test]
    fn tool_details_bound_unicode_titles_without_losing_redaction_or_truncation_notice() {
        let activity = TurnActivity::Tool {
            kind: thoughttree_gpui_model::ToolActivityKind::Search,
            title: "é".repeat(220),
            title_truncated: None,
            title_redacted: Some(true),
            status: thoughttree_gpui_model::ToolActivityStatus::Completed,
            completed_at: None,
            timestamp: None,
        };
        let (heading, body) = activity_details(&activity);
        assert_eq!(heading, "Search · Completed");
        let mut lines = body.lines();
        assert_eq!(lines.next().unwrap(), "é".repeat(200));
        assert_eq!(
            lines.collect::<Vec<_>>(),
            ["Title truncated", "Title replaced by a summary"]
        );
    }
}
