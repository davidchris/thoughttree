use std::{
    collections::{BTreeMap, VecDeque},
    path::PathBuf,
};

use gpui::{prelude::*, *};
use gpui_component::input::{Input, InputEvent, InputState};
use thoughttree_desktop::{
    AgentProvider, Config, Desktop, ProjectEntry, ReasoningEffort, RecoveryEntry,
};
use thoughttree_gpui_model::{self as model, Position, Role};

use crate::{theme, workspace::Workspace};

#[derive(Clone)]
pub(crate) enum Modal {
    Setup,
    ChangingVault,
    Projects,
    Settings,
    Recovery,
    Conflict,
    Edge {
        id: String,
    },
    Context {
        node: Option<String>,
        position: Position,
    },
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Scope {
    Global,
    Project,
}

pub(crate) struct DialogState {
    filter: Entity<InputState>,
    path_inputs: BTreeMap<String, Entity<InputState>>,
    projects: Vec<ProjectEntry>,
    recovery: Vec<RecoveryEntry>,
    recent: bool,
    scope: Scope,
    expanded_models: Option<String>,
    compare: bool,
    disk: Option<String>,
    local: Option<String>,
    compare_task: Option<Task<()>>,
    listing_request: u64,
    listing_task: Option<Task<()>>,
    loading: bool,
    config_queue: VecDeque<ConfigWrite>,
    config_writing: bool,
    /// Executable-path requests per provider still being validated and saved.
    provider_paths_saving: BTreeMap<&'static str, usize>,
    /// One config reload runs at a time; activations meanwhile queue one more.
    config_reloading: bool,
    config_reload_again: bool,
    config_error: Option<String>,
    provider_scan: u64,
    providers_loading: bool,
}

/// Who committed the Vault a transition moves to.
#[derive(Clone, Copy)]
enum VaultSource {
    /// This window chose it and writes it.
    Chosen,
    /// The other frontend already committed it.
    Adopted,
}

pub(crate) enum ConfigWrite {
    Provider(AgentProvider),
    Model(AgentProvider, Option<String>),
    Effort(AgentProvider, Option<ReasoningEffort>),
    AddRecent(PathBuf),
    RemoveRecent(String),
}

impl ConfigWrite {
    fn save(self, desktop: &Desktop) -> Result<(), String> {
        match self {
            Self::Provider(provider) => desktop.set_default_provider(provider),
            Self::Model(provider, model) => desktop.set_model_preference(&provider, model),
            Self::Effort(provider, effort) => desktop.set_effort_preference(&provider, effort),
            Self::AddRecent(path) => desktop.add_recent_project(&path),
            Self::RemoveRecent(path) => desktop.remove_recent_project(&path),
        }
    }
}

enum DialogListing {
    Projects(PathBuf, Vec<ProjectEntry>),
    Recovery(Vec<RecoveryEntry>),
}

impl DialogState {
    pub(crate) fn reset_project(&mut self) {
        self.listing_request = self.listing_request.wrapping_add(1);
        self.listing_task = None;
        self.loading = false;
        self.projects.clear();
        self.recovery.clear();
        self.reset_conflict();
    }

    pub(crate) fn reset_conflict(&mut self) {
        self.compare = false;
        self.disk = None;
        self.local = None;
        self.compare_task = None;
    }

    pub(crate) fn new(window: &mut Window, cx: &mut Context<Workspace>) -> Self {
        let filter = cx.new(|cx| InputState::new(window, cx).placeholder("Filter projects…"));
        cx.subscribe(&filter, |_, _, _: &InputEvent, cx| cx.notify())
            .detach();
        let path_inputs = AgentProvider::ALL
            .iter()
            .map(|provider| {
                let input =
                    cx.new(|cx| InputState::new(window, cx).placeholder("Automatic discovery"));
                (provider.descriptor().id.to_owned(), input)
            })
            .collect();
        Self {
            filter,
            path_inputs,
            projects: Vec::new(),
            recovery: Vec::new(),
            recent: true,
            scope: Scope::Global,
            expanded_models: None,
            compare: false,
            disk: None,
            local: None,
            compare_task: None,
            listing_request: 0,
            listing_task: None,
            loading: false,
            config_queue: VecDeque::new(),
            config_writing: false,
            provider_paths_saving: BTreeMap::new(),
            config_reloading: false,
            config_reload_again: false,
            config_error: None,
            provider_scan: 0,
            providers_loading: false,
        }
    }
}

impl Workspace {
    pub(crate) fn config_is_busy(&self) -> bool {
        self.dialogs.config_writing
            || !self.dialogs.config_queue.is_empty()
            || !self.dialogs.provider_paths_saving.is_empty()
    }

    /// Desktop reports every request exactly once, so a count per provider
    /// tracks every in-flight save.
    fn save_provider_path(&mut self, provider: AgentProvider, path: Option<String>) {
        *self
            .dialogs
            .provider_paths_saving
            .entry(provider.descriptor().id)
            .or_default() += 1;
        self.desktop.set_provider_path(provider, path);
    }

    pub(crate) fn provider_path_saved(&mut self, provider: &AgentProvider, cx: &mut Context<Self>) {
        let id = provider.descriptor().id;
        if let Some(pending) = self.dialogs.provider_paths_saving.get_mut(id) {
            *pending -= 1;
            if *pending == 0 {
                self.dialogs.provider_paths_saving.remove(id);
            }
        }
        self.resume_queued_reload(cx);
    }

    /// Runs a reload that activation queued while settings were busy.
    fn resume_queued_reload(&mut self, cx: &mut Context<Self>) {
        if self.config_is_busy()
            || self.dialogs.config_reloading
            || !std::mem::take(&mut self.dialogs.config_reload_again)
        {
            return;
        }
        let window = self.window_handle;
        let this = cx.entity().downgrade();
        cx.defer(move |cx| {
            let _ = window.update(cx, |_, window, cx| {
                let _ = this.update(cx, |this, cx| this.reload_config(window, cx));
            });
        });
    }

    pub(crate) fn provider_scan_is_busy(&self) -> bool {
        self.dialogs.providers_loading
    }

    pub(crate) fn enqueue_config(&mut self, change: ConfigWrite, cx: &mut Context<Self>) {
        if !self.config_is_busy() {
            self.dialogs.config_error = None;
        }
        self.dialogs.config_queue.push_back(change);
        self.write_next_config(cx);
    }

    fn write_next_config(&mut self, cx: &mut Context<Self>) {
        if self.dialogs.config_writing {
            return;
        }
        let Some(change) = self.dialogs.config_queue.pop_front() else {
            return;
        };
        let provider = if let ConfigWrite::Provider(provider) = &change {
            Some(provider.clone())
        } else {
            None
        };
        let previous_provider = self.provider.clone();
        let desktop = self.desktop.clone();
        self.dialogs.config_writing = true;
        cx.spawn(async move |this, cx| {
            let result = smol::unblock(move || change.save(&desktop)).await;
            let _ = this.update(cx, |this, cx| {
                this.complete_config_write(result, provider, previous_provider, cx)
            });
        })
        .detach();
        cx.notify();
    }

    fn complete_config_write(
        &mut self,
        result: Result<(), String>,
        provider: Option<AgentProvider>,
        previous: AgentProvider,
        cx: &mut Context<Self>,
    ) {
        self.dialogs.config_writing = false;
        match result {
            // The default applies unless the user picked another provider
            // meanwhile. Project replacement does not choose a provider.
            Ok(()) if self.provider == previous => {
                if let Some(provider) = provider {
                    self.provider = provider;
                    self.selected_model = None;
                    self.ensure_models(cx);
                }
            }
            Ok(()) => {}
            Err(error) => self.dialogs.config_error = Some(error),
        }
        self.write_next_config(cx);
        self.resume_queued_reload(cx);
        cx.notify();
    }

    /// Adopts settings the other frontend committed while this window was in
    /// the background. A different Vault takes the guarded Vault transition.
    pub(crate) fn reload_config(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return;
        }
        // A busy write would race the reload; run it once settings are idle.
        if self.config_is_busy() {
            self.dialogs.config_reload_again = true;
            return;
        }
        // Overlapping reloads would compare against the same `previous`, so a
        // later one could refuse a change the earlier one already applied.
        if self.dialogs.config_reloading {
            self.dialogs.config_reload_again = true;
            return;
        }
        self.dialogs.config_reloading = true;
        let previous = self.desktop.config();
        let desktop = self.desktop.clone();
        let generation = self.generation;
        cx.spawn_in(window, async move |this, cx| {
            let result = cx
                .background_spawn(async move { desktop.reload_config() })
                .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.dialogs.config_reloading = false;
                let same_project = this.generation == generation;
                this.adopt_config(previous, result, same_project, window, cx);
                if std::mem::take(&mut this.dialogs.config_reload_again) {
                    this.reload_config(window, cx);
                }
            });
        })
        .detach();
    }

    fn adopt_config(
        &mut self,
        previous: Config,
        result: Result<Config, String>,
        same_project: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        // The cache already holds these settings, so they apply even if the
        // Project changed meanwhile; a later reload would see no difference.
        let config = match result {
            Ok(config) => config,
            Err(error) => {
                self.notice = Some(error);
                cx.notify();
                return;
            }
        };
        if config.default_provider != previous.default_provider
            && self.provider == previous.default_provider
        {
            self.provider = config.default_provider;
            self.selected_model = None;
            self.ensure_models(cx);
        }
        if config.provider_paths != previous.provider_paths {
            self.refresh_provider_statuses(cx);
            for provider in AgentProvider::ALL {
                if config.provider_paths.get(provider) != previous.provider_paths.get(provider) {
                    self.invalidate_models(provider, cx);
                }
            }
        }
        // A replaced Project, Turn or save in progress keeps the current
        // Vault; the session Vault stays pinned, so the next activation
        // retries.
        let idle = same_project && !self.vault_change_blocked();
        match config.notes_directory {
            Some(vault) if idle && previous.notes_directory.as_ref() != Some(&vault) => {
                self.transition_vault(vault, VaultSource::Adopted, window, cx)
            }
            _ => cx.notify(),
        }
    }

    pub(crate) fn refresh_provider_statuses(&mut self, cx: &mut Context<Self>) {
        self.dialogs.provider_scan = self.dialogs.provider_scan.wrapping_add(1);
        self.dialogs.providers_loading = true;
        let request = self.dialogs.provider_scan;
        let desktop = self.desktop.clone();
        cx.spawn(async move |this, cx| {
            let providers = cx
                .background_spawn(async move { desktop.available_providers() })
                .await;
            let _ = this.update(cx, |this, cx| {
                if request == this.dialogs.provider_scan {
                    this.dialogs.providers_loading = false;
                    this.providers = providers;
                    cx.notify();
                }
            });
        })
        .detach();
    }

    pub(crate) fn open_modal(&mut self, modal: Modal, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return;
        }
        self.notice = None;
        self.palette = None;
        self.dismiss_mentions();
        self.focus.focus(window, cx);
        self.dialogs.listing_request = self.dialogs.listing_request.wrapping_add(1);
        self.dialogs.listing_task = None;
        self.dialogs.loading = false;
        match &modal {
            Modal::Projects => {
                self.dialogs.projects.clear();
                self.load_modal_listing(false, cx);
                self.dialogs
                    .filter
                    .update(cx, |input, cx| input.set_value("", window, cx));
            }
            Modal::Settings => {
                let config = self.desktop.config();
                for provider in AgentProvider::ALL {
                    let input = &self.dialogs.path_inputs[provider.descriptor().id];
                    let value = config
                        .provider_paths
                        .get(provider)
                        .cloned()
                        .unwrap_or_default();
                    input.update(cx, |input, cx| input.set_value(value, window, cx));
                }
                self.refresh_provider_statuses(cx);
            }
            Modal::Recovery => {
                self.dialogs.recovery.clear();
                self.load_modal_listing(true, cx);
            }
            Modal::Conflict => {
                self.dialogs.reset_conflict();
            }
            Modal::Context { node: Some(id), .. } => {
                self.selected_edge = None;
                if !self.editor.selection.contains(id) {
                    self.editor.selection.clear();
                    self.editor.selection.insert(id.clone());
                }
                self.refresh(cx);
            }
            _ => {}
        }
        self.modal = Some(modal);
        cx.notify();
    }

    fn load_modal_listing(&mut self, recovery: bool, cx: &mut Context<Self>) {
        self.dialogs.loading = true;
        let request = self.dialogs.listing_request;
        let generation = self.generation;
        let vault = self.desktop.notes_directory().ok();
        let desktop = self.desktop.clone();
        let job = cx.background_executor().spawn(async move {
            if recovery {
                desktop
                    .list_project_recovery()
                    .map(DialogListing::Recovery)
                    .map_err(|e| e.to_string())
            } else {
                desktop.prune_recent_projects()?;
                desktop
                    .list_projects_with_root()
                    .map(|(root, entries)| DialogListing::Projects(root, entries))
            }
        });
        self.dialogs.listing_task = Some(cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                if request != this.dialogs.listing_request || generation != this.generation {
                    return;
                }
                let still_open = matches!(
                    (&this.modal, recovery),
                    (Some(Modal::Recovery), true) | (Some(Modal::Projects), false)
                );
                if !still_open || (!recovery && vault != this.desktop.notes_directory().ok()) {
                    return;
                }
                this.dialogs.loading = false;
                match result {
                    Ok(DialogListing::Projects(root, entries)) if Some(&root) == vault.as_ref() => {
                        this.dialogs.projects = entries
                    }
                    Ok(DialogListing::Recovery(entries)) => this.dialogs.recovery = entries,
                    Ok(_) => {}
                    Err(error) => this.notice = Some(error),
                }
                cx.notify();
            });
        }));
    }

    pub(crate) fn render_modal(
        &mut self,
        _window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Option<AnyElement> {
        let modal = self.modal.clone()?;
        let (title, content) = match &modal {
            Modal::Setup => ("Welcome to ThoughtTree", self.setup_dialog(cx)),
            Modal::ChangingVault => (
                "Changing notes directory",
                hint("Saving a Recovery snapshot and updating the Vault…").into_any_element(),
            ),
            Modal::Projects => ("Open Project", self.projects_dialog(cx)),
            Modal::Settings => ("Settings", self.settings_dialog(cx)),
            Modal::Recovery => ("Recovery snapshots", self.recovery_dialog(cx)),
            Modal::Conflict => ("Project changed on disk", self.conflict_dialog(cx)),
            Modal::Edge { id } => {
                let id = id.clone();
                (
                    "Edge actions",
                    button(
                        "edge-delete",
                        "Delete edge",
                        false,
                        cx,
                        move |this, _, cx| this.delete_context_edge(&id, cx),
                    ),
                )
            }
            Modal::Context { node, position } => (
                "Graph actions",
                self.context_dialog(node.as_deref(), *position, cx),
            ),
        };
        let setup = matches!(modal, Modal::Setup | Modal::ChangingVault);
        Some(
            div()
                .debug_selector(|| "desktop-overlay".into())
                .occlude()
                .absolute()
                .inset_0()
                .flex()
                .items_center()
                .justify_center()
                .bg(rgba(0x00000099))
                .on_mouse_down(
                    MouseButton::Left,
                    cx.listener(move |this, _, _, cx| {
                        if !setup {
                            this.modal = None;
                            cx.notify();
                        }
                    }),
                )
                .child(
                    div()
                        .id("desktop-dialog")
                        .debug_selector(|| "desktop-dialog".into())
                        .w(px(if matches!(modal, Modal::Conflict) {
                            960.
                        } else {
                            700.
                        }))
                        .max_h(relative(0.9))
                        .flex()
                        .flex_col()
                        .bg(theme::surface())
                        .border_1()
                        .border_color(theme::border())
                        .rounded_lg()
                        .shadow_lg()
                        .overflow_hidden()
                        .on_mouse_down(MouseButton::Left, |_, _, cx| cx.stop_propagation())
                        .on_mouse_down_out(cx.listener(|this, _, _, cx| {
                            if matches!(
                                this.modal,
                                Some(Modal::Context { .. } | Modal::Edge { .. })
                            ) {
                                this.modal = None;
                                cx.notify();
                            }
                        }))
                        .child(
                            div()
                                .flex()
                                .items_center()
                                .justify_between()
                                .p_5()
                                .border_b_1()
                                .border_color(theme::border())
                                .child(
                                    div()
                                        .text_lg()
                                        .font_weight(FontWeight::SEMIBOLD)
                                        .child(title),
                                )
                                .when(!setup, |row| {
                                    row.child(button(
                                        "dialog-close",
                                        "Close  Esc",
                                        false,
                                        cx,
                                        |this, _, cx| {
                                            this.modal = None;
                                            cx.notify();
                                        },
                                    ))
                                }),
                        )
                        .child(
                            div()
                                .id("dialog-scroll")
                                .debug_selector(|| "dialog-scroll".into())
                                .overflow_y_scroll()
                                .min_h_0()
                                .p_5()
                                .child(content),
                        )
                        .when_some(self.notice.clone(), |d, message| {
                            d.child(
                                div()
                                    .p_4()
                                    .border_t_1()
                                    .border_color(theme::border())
                                    .text_sm()
                                    .text_color(theme::accent())
                                    .child(message),
                            )
                        })
                        .when(self.config_is_busy(), |d| {
                            d.child(
                                hint("Saving settings…")
                                    .debug_selector(|| "settings-writing".into())
                                    .p_4(),
                            )
                        })
                        .when(self.provider_scan_is_busy(), |d| {
                            d.child(hint("Checking provider availability…").p_4())
                        })
                        .when_some(self.dialogs.config_error.clone(), |d, error| {
                            d.child(
                                hint(&error)
                                    .debug_selector(|| "settings-write-error".into())
                                    .p_4(),
                            )
                        }),
                )
                .into_any_element(),
        )
    }

    fn delete_context_edge(&mut self, id: &str, cx: &mut Context<Self>) {
        let blocked = self
            .editor
            .project
            .graph
            .edges
            .iter()
            .find(|edge| edge.id == id)
            .is_some_and(|edge| {
                self.editor.is_node_blocked(&edge.source)
                    || self.editor.is_node_blocked(&edge.target)
            });
        if blocked {
            return;
        }
        self.change(
            |project| {
                project.graph.remove_edge(id);
            },
            cx,
        );
        self.modal = None;
        self.selected_edge = None;
        cx.notify();
    }

    pub(crate) fn change_notes_directory(
        &mut self,
        path: PathBuf,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        self.transition_vault(path, VaultSource::Chosen, window, cx);
    }

    fn transition_vault(
        &mut self,
        path: PathBuf,
        source: VaultSource,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return;
        }
        if self.vault_change_blocked() || self.config_is_busy() {
            self.notice =
                Some("Wait for the active Turn or save before changing the notes directory".into());
            cx.notify();
            return;
        }
        let previous_modal = self.modal.clone();
        let dirty = self.editor.is_dirty();
        let content = if dirty {
            match self.editor.project.to_json() {
                Ok(content) => Some(content),
                Err(error) => {
                    self.notice = Some(error.to_string());
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        let source_path = self.project_path.clone();
        let desktop = self.desktop.clone();
        // Invalidate operations started for the old Vault. The progress modal
        // keeps edits and replacement actions parked until the commit finishes.
        self.generation = self.generation.wrapping_add(1);
        self.transition_request = self.transition_request.wrapping_add(1);
        self.autosave = None;
        self.reset_transient_jobs();
        let generation = self.generation;
        self.modal = Some(Modal::ChangingVault);
        self.focus.focus(window, cx);
        self.notice = None;
        cx.spawn_in(window, async move |this, cx| {
            let result = smol::unblock(move || {
                if !path.is_absolute() || !path.is_dir() {
                    return Err("Select an existing absolute path for the notes directory".into());
                }
                if let Some(content) = content {
                    desktop
                        .snapshot_project(source_path.as_deref(), &content)
                        .map_err(|e| e.to_string())?;
                }
                match source {
                    VaultSource::Chosen => desktop.set_notes_directory(path)?,
                    VaultSource::Adopted => desktop.adopt_notes_directory(&path)?,
                }
                Ok(source_path
                    .as_ref()
                    .is_some_and(|path| desktop.project_path(path).is_err()))
            })
            .await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.complete_vault_change(result, generation, previous_modal, dirty, window, cx)
            });
        })
        .detach();
        cx.notify();
    }

    fn complete_vault_change(
        &mut self,
        result: Result<bool, String>,
        generation: u64,
        previous_modal: Option<Modal>,
        dirty: bool,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if self.generation != generation {
            return;
        }
        let setup = matches!(previous_modal, Some(Modal::Setup));
        self.modal = previous_modal;
        match result {
            Ok(detached) => {
                let path = if detached {
                    None
                } else {
                    self.project_path.clone()
                };
                let revision = if detached {
                    None
                } else {
                    self.revision.clone()
                };
                self.install_project(
                    self.editor.project.clone(),
                    path,
                    revision,
                    self.title.clone(),
                    window,
                    cx,
                );
                if dirty || detached {
                    self.editor.touch();
                    self.save_current(cx);
                }
                self.notice = Some(if detached {
                    "Notes directory updated. Save this Project in the new Vault; the original file is preserved."
                } else { "Notes directory updated" }.into());
                if setup {
                    self.open_modal(Modal::Projects, window, cx);
                }
            }
            Err(error) => {
                self.save_current(cx);
                self.notice = Some(error);
                self.schedule_summaries(cx);
                // The transition invalidated inspections of the unchanged Vault.
                self.refresh_files(cx);
            }
        }
        self.refresh(cx);
    }

    fn remove_recent(&mut self, path: &str, cx: &mut Context<Self>) {
        self.enqueue_config(ConfigWrite::RemoveRecent(path.into()), cx);
    }

    fn setup_dialog(&self, cx: &Context<Self>) -> AnyElement {
        column().gap_4()
            .child(div().text_color(theme::muted()).child("Choose the Vault where your Project files and context files live. ThoughtTree uses your existing Codex or Claude Code installation."))
            .child(button("setup-vault", "Choose notes directory…", true, cx, |this, window, cx| this.choose_notes_directory(window, cx)))
            .child(button("setup-recovery", "Recover a previous Project", false, cx, |this, window, cx| this.open_modal(Modal::Recovery, window, cx)))
            .into_any_element()
    }

    pub(crate) fn choose_notes_directory(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: false,
            directories: true,
            multiple: false,
            prompt: Some("Choose notes directory".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = prompt.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.change_notes_directory(path, window, cx);
                    }
                    cx.notify();
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    this.notice = Some(error.to_string());
                    cx.notify();
                }
                Err(error) => {
                    this.notice = Some(error.to_string());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn projects_dialog(&self, cx: &Context<Self>) -> AnyElement {
        let query = self.dialogs.filter.read(cx).value().to_lowercase();
        let mut body = column()
            .gap_3()
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(button(
                        "projects-vault",
                        "All projects",
                        !self.dialogs.recent,
                        cx,
                        |this, _, cx| {
                            this.dialogs.recent = false;
                            cx.notify();
                        },
                    ))
                    .child(button(
                        "projects-recent",
                        "Recent",
                        self.dialogs.recent,
                        cx,
                        |this, _, cx| {
                            this.dialogs.recent = true;
                            cx.notify();
                        },
                    ))
                    .child(button(
                        "projects-browse",
                        "Browse…",
                        false,
                        cx,
                        |this, window, cx| this.browse_project(window, cx),
                    )),
            )
            .child(Input::new(&self.dialogs.filter));
        if self.dialogs.recent {
            for path in self
                .desktop
                .config()
                .recent_projects
                .into_iter()
                .filter(|path| !self.dialogs.loading && path.to_lowercase().contains(&query))
            {
                let open = PathBuf::from(&path);
                let remove = path.clone();
                body = body.child(
                    div()
                        .flex()
                        .gap_2()
                        .items_center()
                        .child(div().flex_1().min_w_0().child(button(
                            &format!("open-{path}"),
                            &path,
                            false,
                            cx,
                            move |this, window, cx| this.open_path(open.clone(), window, cx),
                        )))
                        .child(button(
                            &format!("remove-{path}"),
                            "Remove",
                            false,
                            cx,
                            move |this, _, cx| this.remove_recent(&remove, cx),
                        )),
                );
            }
            if self.dialogs.loading {
                body = body.child(hint("Loading recent Projects…"));
            } else if self.desktop.config().recent_projects.is_empty() {
                body = body.child(hint("No recent Projects yet."));
            }
        } else {
            for entry in self
                .dialogs
                .projects
                .iter()
                .filter(|entry| entry.relative_path.to_lowercase().contains(&query))
            {
                let path = PathBuf::from(&entry.relative_path);
                let label = format!(
                    "{}   ·   {}",
                    entry.relative_path,
                    date(entry.modified_epoch_ms)
                );
                body = body.child(button(
                    &format!("project-{}", entry.relative_path),
                    &label,
                    false,
                    cx,
                    move |this, window, cx| this.open_path(path.clone(), window, cx),
                ));
            }
            if self.dialogs.loading {
                body = body.child(hint("Loading Projects…"));
            } else if self.dialogs.projects.is_empty() {
                body = body.child(hint("No Project files in this Vault yet."));
            }
        }
        body.child(
            div()
                .border_t_1()
                .border_color(theme::border())
                .pt_3()
                .flex()
                .gap_2()
                .child(button(
                    "new-project",
                    "New Project",
                    true,
                    cx,
                    |this, window, cx| this.new_project(window, cx),
                ))
                .child(button(
                    "import-kagi",
                    "Import Kagi…",
                    false,
                    cx,
                    |this, window, cx| this.import_dialog(window, cx),
                )),
        )
        .into_any_element()
    }

    pub(crate) fn browse_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Open a .thoughttree Project".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = prompt.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        this.open_path(path, window, cx);
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    this.notice = Some(error.to_string());
                    cx.notify();
                }
                Err(error) => {
                    this.notice = Some(error.to_string());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn settings_dialog(&self, cx: &Context<Self>) -> AnyElement {
        let config = self.desktop.config();
        let mut body = column()
            .gap_4()
            .child(section("Notes directory"))
            .child(
                div().text_sm().text_color(theme::muted()).child(
                    config
                        .notes_directory
                        .as_ref()
                        .map(|path| path.display().to_string())
                        .unwrap_or_else(|| "Not configured".into()),
                ),
            )
            .child(button(
                "settings-vault",
                "Change directory…",
                false,
                cx,
                |this, window, cx| this.choose_notes_directory(window, cx),
            ))
            .child(section("Default provider"));
        let mut providers = div().flex().gap_2();
        for provider in AgentProvider::ALL {
            let selected = config.default_provider == *provider;
            let choice = provider.clone();
            let available = self
                .providers
                .iter()
                .any(|status| status.provider == *provider && status.available);
            let label = format!(
                "{}{}",
                provider.display_name(),
                if available { "" } else { " (unavailable)" }
            );
            providers = providers.child(button_enabled(
                provider.descriptor().id,
                &label,
                selected,
                available,
                cx,
                move |this, _, cx| {
                    if !this
                        .providers
                        .iter()
                        .any(|status| status.provider == choice && status.available)
                    {
                        return;
                    }
                    this.enqueue_config(ConfigWrite::Provider(choice.clone()), cx);
                },
            ));
        }
        body = body.child(providers)
            .child(section("Model and reasoning effort"))
            .child(div().flex().gap_2()
                .child(button("scope-global", "Global defaults", self.dialogs.scope == Scope::Global, cx,
                    |this, _, cx| { this.dialogs.scope = Scope::Global; this.dialogs.expanded_models = None; cx.notify(); }))
                .child(button("scope-project", "This Project", self.dialogs.scope == Scope::Project, cx,
                    |this, _, cx| { this.dialogs.scope = Scope::Project; this.dialogs.expanded_models = None; cx.notify(); })))
            .child(hint("Project preferences override global defaults. An unset global preference uses the Provider's CLI default."));
        for provider in AgentProvider::ALL {
            body = body.child(self.provider_preferences(provider, cx));
        }
        body = body.child(section("Provider executables"));
        for provider in AgentProvider::ALL {
            body = body.child(self.provider_path_row(provider, cx));
        }
        body.into_any_element()
    }

    fn provider_preferences(&self, provider: &AgentProvider, cx: &Context<Self>) -> AnyElement {
        let config = self.desktop.config();
        let key = provider.descriptor().id;
        let selected_model = if self.dialogs.scope == Scope::Global {
            config.model_preferences.get(provider).cloned()
        } else {
            self.editor
                .project
                .project_model_preferences
                .as_ref()
                .and_then(|p| p.get(key))
                .cloned()
        };
        let effort = if self.dialogs.scope == Scope::Global {
            config.effort_preferences.get(provider).map(|e| e.as_str())
        } else {
            self.editor
                .project
                .project_effort_preferences
                .as_ref()
                .and_then(|p| p.get(key))
                .map(|e| e.as_str())
        };
        let toggle_key = key.to_owned();
        let discover = provider.clone();
        let mut row = column()
            .gap_2()
            .p_3()
            .bg(theme::raised())
            .rounded_md()
            .child(
                div()
                    .font_weight(FontWeight::SEMIBOLD)
                    .child(provider.display_name()),
            )
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(button(
                        &format!("model-{key}"),
                        &format!(
                            "Model: {} ▾",
                            selected_model.as_deref().unwrap_or("Default")
                        ),
                        false,
                        cx,
                        move |this, _, cx| {
                            this.dialogs.expanded_models =
                                if this.dialogs.expanded_models.as_ref() == Some(&toggle_key) {
                                    None
                                } else {
                                    Some(toggle_key.clone())
                                };
                            cx.notify();
                        },
                    ))
                    .child(button(
                        &format!("discover-{key}"),
                        "Refresh models",
                        false,
                        cx,
                        move |this, _, cx| {
                            this.notice = match this.desktop.discover_models(discover.clone()) {
                                Ok(()) => Some("Discovering available models…".into()),
                                Err(error) => Some(error),
                            };
                            cx.notify();
                        },
                    )),
            );
        if self.dialogs.expanded_models.as_deref() == Some(key) {
            let mut choices = column().gap_1();
            let default_provider = provider.clone();
            choices = choices.child(button(
                &format!("model-default-{key}"),
                "Default",
                selected_model.is_none(),
                cx,
                move |this, _, cx| this.choose_model(default_provider.clone(), None, cx),
            ));
            for model in self.models.get(key).into_iter().flatten() {
                let provider = provider.clone();
                let id = model.model_id.clone();
                choices = choices.child(button(
                    &format!("model-{key}-{id}"),
                    &model.display_name,
                    selected_model.as_ref() == Some(&id),
                    cx,
                    move |this, _, cx| this.choose_model(provider.clone(), Some(id.clone()), cx),
                ));
            }
            row = row.child(choices);
        }
        let mut efforts = div().flex().gap_1().flex_wrap();
        for (label, value) in [
            ("Default", None),
            ("Low", Some(ReasoningEffort::Low)),
            ("Medium", Some(ReasoningEffort::Medium)),
            ("High", Some(ReasoningEffort::High)),
            ("Extra high", Some(ReasoningEffort::XHigh)),
        ] {
            let provider = provider.clone();
            efforts = efforts.child(button(
                &format!("effort-{key}-{label}"),
                label,
                effort == value.map(|e| e.as_str()),
                cx,
                move |this, _, cx| this.choose_effort(provider.clone(), value, cx),
            ));
        }
        row.child(hint("Reasoning effort"))
            .child(efforts)
            .into_any_element()
    }

    fn choose_model(
        &mut self,
        provider: AgentProvider,
        model: Option<String>,
        cx: &mut Context<Self>,
    ) {
        if self.dialogs.scope == Scope::Global {
            self.enqueue_config(ConfigWrite::Model(provider, model), cx);
        } else {
            self.change(
                |project| {
                    let preferences = project
                        .project_model_preferences
                        .get_or_insert_with(BTreeMap::new);
                    if let Some(model) = model {
                        preferences.insert(provider.descriptor().id.into(), model);
                    } else {
                        preferences.remove(provider.descriptor().id);
                    }
                },
                cx,
            );
        }
        self.dialogs.expanded_models = None;
        self.refresh(cx);
    }

    fn choose_effort(
        &mut self,
        provider: AgentProvider,
        effort: Option<ReasoningEffort>,
        cx: &mut Context<Self>,
    ) {
        if self.dialogs.scope == Scope::Global {
            self.enqueue_config(ConfigWrite::Effort(provider, effort), cx);
        } else {
            self.change(
                |project| {
                    let preferences = project
                        .project_effort_preferences
                        .get_or_insert_with(BTreeMap::new);
                    let effort = effort.map(|effort| match effort {
                        ReasoningEffort::Low => model::ReasoningEffort::Low,
                        ReasoningEffort::Medium => model::ReasoningEffort::Medium,
                        ReasoningEffort::High => model::ReasoningEffort::High,
                        ReasoningEffort::XHigh => model::ReasoningEffort::XHigh,
                    });
                    if let Some(effort) = effort {
                        preferences.insert(provider.descriptor().id.into(), effort);
                    } else {
                        preferences.remove(provider.descriptor().id);
                    }
                },
                cx,
            );
        }
        self.refresh(cx);
    }

    fn provider_path_row(&self, provider: &AgentProvider, cx: &Context<Self>) -> AnyElement {
        let key = provider.descriptor().id;
        let input = self.dialogs.path_inputs[key].clone();
        let validate = provider.clone();
        let browse = provider.clone();
        let reset = provider.clone();
        let status = self
            .providers
            .iter()
            .find(|status| status.provider == *provider);
        column()
            .gap_2()
            .p_3()
            .border_1()
            .border_color(theme::border())
            .rounded_md()
            .child(div().font_weight(FontWeight::MEDIUM).child(format!(
                "{}   {}",
                provider.display_name(),
                if status.is_some_and(|s| s.available) {
                    "✓ Available"
                } else {
                    "Not found"
                }
            )))
            .when_some(status.and_then(|s| s.error_message.clone()), |d, error| {
                d.child(hint(&error).debug_selector(|| format!("provider-error-{key}")))
            })
            .child(Input::new(&input))
            .child(
                div()
                    .flex()
                    .gap_2()
                    .child(button(
                        &format!("path-validate-{key}"),
                        "Validate & save",
                        true,
                        cx,
                        move |this, _, cx| {
                            let path = this.dialogs.path_inputs[validate.descriptor().id]
                                .read(cx)
                                .value()
                                .to_string();
                            this.save_provider_path(
                                validate.clone(),
                                (!path.trim().is_empty()).then(|| path.trim().to_owned()),
                            );
                            this.notice = Some("Checking executable…".into());
                            cx.notify();
                        },
                    ))
                    .child(button(
                        &format!("path-browse-{key}"),
                        "Browse…",
                        false,
                        cx,
                        move |this, window, cx| this.browse_provider(browse.clone(), window, cx),
                    ))
                    .child(button(
                        &format!("path-reset-{key}"),
                        "Reset",
                        false,
                        cx,
                        move |this, window, cx| {
                            this.dialogs.path_inputs[reset.descriptor().id]
                                .update(cx, |input, cx| input.set_value("", window, cx));
                            this.save_provider_path(reset.clone(), None);
                            cx.notify();
                        },
                    )),
            )
            .into_any_element()
    }

    fn browse_provider(
        &mut self,
        provider: AgentProvider,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let prompt = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some(format!("Choose {} executable", provider.display_name()).into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = prompt.await;
            let _ = this.update_in(cx, |this, window, cx| match result {
                Ok(Ok(Some(paths))) => {
                    if let Some(path) = paths.into_iter().next() {
                        let path = path.to_string_lossy().to_string();
                        this.dialogs.path_inputs[provider.descriptor().id]
                            .update(cx, |input, cx| input.set_value(path.clone(), window, cx));
                        this.save_provider_path(provider, Some(path));
                        this.notice = Some("Checking executable…".into());
                        cx.notify();
                    }
                }
                Ok(Ok(None)) => {}
                Ok(Err(error)) => {
                    this.notice = Some(error.to_string());
                    cx.notify();
                }
                Err(error) => {
                    this.notice = Some(error.to_string());
                    cx.notify();
                }
            });
        })
        .detach();
    }

    fn recovery_dialog(&self, cx: &Context<Self>) -> AnyElement {
        let mut body = column().gap_3().child(hint(
            "Open a saved version as a new, unsaved Project. The source file is preserved.",
        ));
        for entry in &self.dialogs.recovery {
            let id = entry.id.clone();
            let source = entry
                .source_path
                .as_ref()
                .map(|p| p.display().to_string())
                .unwrap_or_else(|| "Unsaved Project".into());
            let label = format!("{}   ·   {}", source, date(entry.created_epoch_ms));
            body = body.child(button(
                &format!("recovery-{id}"),
                &label,
                false,
                cx,
                move |this, window, cx| this.restore_recovery(&id, window, cx),
            ));
        }
        if self.dialogs.loading {
            body = body.child(hint("Loading recovery snapshots…"));
        } else if self.dialogs.recovery.is_empty() {
            body = body.child(hint("No recovery snapshots are available."));
        }
        body.into_any_element()
    }

    fn conflict_dialog(&self, cx: &Context<Self>) -> AnyElement {
        let mut body = column().gap_4()
            .child(hint("Another writer changed this Project after you opened it. Your edits have a Recovery snapshot. Choose which version to keep."))
            .child(div().flex().gap_2()
                .child(button("conflict-reload", "Reload disk version", false, cx, |this, window, cx| {
                    if let Some(path) = this.project_path.clone() { this.open_path(path, window, cx); }
                }))
                .child(button("conflict-compare", "Compare versions", false, cx, |this, _, cx| {
                    this.compare_versions(cx);
                }))
                .child(button("conflict-copy", "Save my edits as a copy", true, cx, |this, window, cx| this.save_conflict_copy(window, cx))));
        if self.dialogs.compare {
            body = body.child(
                div()
                    .flex()
                    .gap_3()
                    .child(comparison(
                        "Your edits at comparison",
                        self.dialogs.local.as_deref().unwrap_or("Unavailable"),
                    ))
                    .when(self.dialogs.disk.is_none(), |body| {
                        body.child(hint("Loading disk version…"))
                    })
                    .children(
                        self.dialogs
                            .disk
                            .as_ref()
                            .map(|project| comparison("On disk at comparison", project)),
                    ),
            );
        }
        body.into_any_element()
    }

    fn compare_versions(&mut self, cx: &mut Context<Self>) {
        let Some(path) = self.project_path.clone() else {
            return;
        };
        let local = match self.editor.project.to_json() {
            Ok(content) => content,
            Err(error) => {
                self.notice = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        self.dialogs.reset_conflict();
        self.dialogs.local = Some(local);
        self.dialogs.compare = true;
        let generation = self.generation;
        let desktop = self.desktop.clone();
        let job = cx
            .background_executor()
            .spawn(async move { desktop.load_project(&path) });
        self.dialogs.compare_task = Some(cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                if this.generation != generation || !matches!(this.modal, Some(Modal::Conflict)) {
                    return;
                }
                match result {
                    Ok(disk) => this.dialogs.disk = Some(disk.content),
                    Err(error) => this.notice = Some(error.to_string()),
                }
                cx.notify();
            });
        }));
        cx.notify();
    }

    fn context_dialog(
        &self,
        node: Option<&str>,
        position: Position,
        cx: &Context<Self>,
    ) -> AnyElement {
        let mut body = column().gap_2();
        if let Some(id) = node {
            let preview = id.to_owned();
            let edit = id.to_owned();
            let generate = id.to_owned();
            let reply = id.to_owned();
            let descendants = id.to_owned();
            if let Some(file) = self
                .editor
                .project
                .graph
                .nodes
                .get(id)
                .and_then(|node| node.file_data())
            {
                let path = file.path.clone();
                let reload = id.to_owned();
                body = body
                    .child(button(
                        "context-reload-file",
                        "Reload file",
                        false,
                        cx,
                        move |this, _, cx| {
                            this.modal = None;
                            this.refresh_file(reload.clone(), cx);
                        },
                    ))
                    .child(button(
                        "context-copy-path",
                        "Copy path",
                        false,
                        cx,
                        move |this, _, cx| {
                            cx.write_to_clipboard(ClipboardItem::new_string(path.clone()));
                            this.modal = None;
                            cx.notify();
                        },
                    ));
            }
            body = body.child(button(
                "context-preview",
                "Preview",
                false,
                cx,
                move |this, window, cx| {
                    this.modal = None;
                    this.preview(preview.clone(), false, window, cx);
                },
            ));
            if self
                .editor
                .project
                .graph
                .nodes
                .get(id)
                .is_some_and(|node| node.role() == Role::User)
            {
                body = body
                    .child(button(
                        "context-edit",
                        "Edit",
                        false,
                        cx,
                        move |this, window, cx| {
                            this.modal = None;
                            this.preview(edit.clone(), true, window, cx);
                        },
                    ))
                    .child(button(
                        "context-generate",
                        "Generate response",
                        false,
                        cx,
                        move |this, window, cx| {
                            this.modal = None;
                            this.generate(generate.clone(), window, cx);
                        },
                    ));
            }
            body = body
                .child(button(
                    "context-reply",
                    "Continue from this node",
                    false,
                    cx,
                    move |this, window, cx| {
                        this.modal = None;
                        this.reply(reply.clone(), window, cx);
                    },
                ))
                .child(button(
                    "context-descendants",
                    "Select node and descendants",
                    false,
                    cx,
                    move |this, _, cx| {
                        this.editor.selection = this.editor.project.graph.descendants(&descendants);
                        this.editor.selection.insert(descendants.clone());
                        this.modal = None;
                        this.refresh(cx);
                    },
                ));
        }
        body.child(button(
            "context-new",
            "New user node here",
            false,
            cx,
            move |this, window, cx| {
                this.modal = None;
                this.create_user(position, Vec::new(), window, cx);
            },
        ))
        .child(button(
            "context-add-file",
            "Add file…",
            false,
            cx,
            move |this, window, cx| {
                this.modal = None;
                this.pick_file(position, window, cx);
            },
        ))
        .child(button(
            "context-export",
            "Export selection as Markdown…",
            false,
            cx,
            |this, window, cx| {
                this.modal = None;
                this.export(false, window, cx);
            },
        ))
        .child(button(
            "context-delete",
            "Delete selected nodes",
            false,
            cx,
            |this, _, cx| {
                this.modal = None;
                this.delete_selected(cx);
            },
        ))
        .into_any_element()
    }
}

fn column() -> Div {
    div().flex().flex_col()
}
fn hint(text: &str) -> Div {
    div()
        .text_sm()
        .text_color(theme::muted())
        .child(text.to_owned())
}
fn section(text: &'static str) -> Div {
    div()
        .pt_2()
        .text_sm()
        .font_weight(FontWeight::SEMIBOLD)
        .child(text)
}

fn button(
    id: &str,
    label: &str,
    selected: bool,
    cx: &Context<Workspace>,
    action: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> AnyElement {
    button_enabled(id, label, selected, true, cx, action)
}

fn button_enabled(
    id: &str,
    label: &str,
    selected: bool,
    enabled: bool,
    cx: &Context<Workspace>,
    action: impl Fn(&mut Workspace, &mut Window, &mut Context<Workspace>) + 'static,
) -> AnyElement {
    div()
        .id(SharedString::from(id.to_owned()))
        .debug_selector(|| id.to_owned())
        .px_3()
        .py_2()
        .rounded_md()
        .text_sm()
        .when(enabled, |d| d.cursor_pointer())
        .when(!enabled, |d| d.opacity(0.5))
        .bg(if selected {
            theme::accent()
        } else {
            theme::raised()
        })
        .text_color(if selected { theme::bg() } else { theme::text() })
        .when(enabled, |d| d.hover(|style| style.opacity(0.8)))
        .child(label.to_owned())
        .on_mouse_down(
            MouseButton::Left,
            cx.listener(move |this, _, window, cx| {
                cx.stop_propagation();
                if enabled {
                    action(this, window, cx);
                }
            }),
        )
        .into_any_element()
}

fn date(epoch_ms: u64) -> String {
    chrono::DateTime::from_timestamp_millis(epoch_ms.min(i64::MAX as u64) as i64)
        .map(|date| {
            date.with_timezone(&chrono::Local)
                .format("%Y-%m-%d %H:%M")
                .to_string()
        })
        .unwrap_or_else(|| "Unknown date".into())
}

fn comparison(title: &'static str, content: &str) -> AnyElement {
    let text = serde_json::from_str::<serde_json::Value>(content)
        .and_then(|value| serde_json::to_string_pretty(&value))
        .unwrap_or_else(|_| content.to_owned());
    column()
        .flex_1()
        .min_w_0()
        .gap_2()
        .p_3()
        .rounded_md()
        .bg(theme::bg())
        .child(section(title))
        .child(div().text_xs().font_family("Menlo").child(text))
        .into_any_element()
}

#[cfg(test)]
mod tests {
    use gpui::{point, px, Modifiers, TestAppContext, VisualTestContext};
    use thoughttree_desktop::{AgentProvider, DesktopEvent, ReasoningEffort};
    use thoughttree_gpui_model::{Position, Project};

    use super::{ConfigWrite, Modal, Scope};
    use crate::interaction_tests::workspace;

    fn click(cx: &mut VisualTestContext, selector: &'static str) {
        let point = cx.debug_bounds(selector).expect(selector).center();
        cx.simulate_click(point, Modifiers::default());
    }

    // Release on a deadline even if a regression blocks the UI thread. Tests
    // fail instead of hanging, and never depend on an arbitrary sleep.
    fn hold_config_lock(
        directory: &std::path::Path,
    ) -> (std::sync::mpsc::Sender<()>, std::thread::JoinHandle<bool>) {
        let lock = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .read(true)
            .write(true)
            .open(directory.join("config/config.lock"))
            .unwrap();
        lock.lock().unwrap();
        let (release, wait) = std::sync::mpsc::channel();
        let thread = std::thread::spawn(move || {
            let responsive = wait.recv_timeout(std::time::Duration::from_secs(3)).is_ok();
            lock.unlock().unwrap();
            responsive
        });
        (release, thread)
    }

    #[gpui::test]
    async fn held_config_lock_keeps_native_input_responsive_and_settings_writes_ordered(
        cx: &mut TestAppContext,
    ) {
        let (directory, workspace, cx, _) = workspace(cx);
        cx.executor().allow_parking();
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| this.open_modal(Modal::Settings, window, cx))
        });
        cx.condition(&workspace, |this, _| !this.provider_scan_is_busy())
            .await;
        let (release, holder) = hold_config_lock(directory.path());
        click(cx, "effort-codex-High");
        assert!(cx.debug_bounds("settings-writing").is_some());
        click(cx, "effort-codex-Low");
        workspace.update(cx, |this, cx| {
            this.choose_model(AgentProvider::Codex, Some("first-model".into()), cx);
            this.choose_model(AgentProvider::Codex, Some("last-model".into()), cx);
            assert!(this.config_is_busy());
            assert!(this
                .desktop
                .config()
                .effort_preferences
                .get(&AgentProvider::Codex)
                .is_none());
        });
        cx.simulate_keystrokes("escape cmd-k");
        cx.simulate_input("Rust");
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| assert!(!this.request_close(window, cx)))
        });
        workspace.read_with(cx, |this, _| {
            assert!(this.palette.is_some());
            assert!(this.config_is_busy());
        });
        let _ = release.send(());
        assert!(
            holder.join().unwrap(),
            "Settings blocked native input while waiting for a config file lock"
        );
        cx.condition(&workspace, |this, _| !this.config_is_busy())
            .await;
        workspace.read_with(cx, |this, _| {
            let config = this.desktop.config();
            assert_eq!(
                config.effort_preferences.get(&AgentProvider::Codex),
                Some(&ReasoningEffort::Low)
            );
            assert_eq!(
                config
                    .model_preferences
                    .get(&AgentProvider::Codex)
                    .map(String::as_str),
                Some("last-model")
            );
            assert!(this.dialogs.config_error.is_none());
        });
    }

    #[gpui::test]
    async fn failed_settings_write_retains_committed_values_and_reports_then_clears_error(
        cx: &mut TestAppContext,
    ) {
        let (directory, workspace, cx, _) = workspace(cx);
        cx.executor().allow_parking();
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| this.open_modal(Modal::Settings, window, cx))
        });
        let config = directory.path().join("config");
        let original = directory.path().join("config-original");
        std::fs::rename(&config, &original).unwrap();
        std::fs::write(&config, "not a directory").unwrap();
        click(cx, "effort-codex-High");
        cx.condition(&workspace, |this, _| !this.config_is_busy())
            .await;
        workspace.read_with(cx, |this, _| {
            assert!(this.dialogs.config_error.is_some());
            assert!(this
                .desktop
                .config()
                .effort_preferences
                .get(&AgentProvider::Codex)
                .is_none());
        });
        assert!(cx.debug_bounds("settings-write-error").is_some());
        std::fs::remove_file(&config).unwrap();
        std::fs::rename(&original, &config).unwrap();
        click(cx, "effort-codex-Low");
        cx.condition(&workspace, |this, _| !this.config_is_busy())
            .await;
        workspace.read_with(cx, |this, _| {
            assert!(this.dialogs.config_error.is_none());
            assert_eq!(
                this.desktop
                    .config()
                    .effort_preferences
                    .get(&AgentProvider::Codex),
                Some(&ReasoningEffort::Low)
            );
        });
    }

    #[gpui::test]
    async fn pending_vault_switch_guards_native_actions_and_failed_switch_resumes_recovery(
        cx: &mut TestAppContext,
    ) {
        let (directory, workspace, cx, _) = workspace(cx);
        cx.executor().allow_parking();
        let next = directory.path().join("next-vault");
        std::fs::create_dir(&next).unwrap();
        let original = workspace.read_with(cx, |this, _| this.editor.project.clone());
        let (release, holder) = hold_config_lock(directory.path());
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.editor.selection.insert("answer".into());
                this.preview("question".into(), true, window, cx);
                this.change_notes_directory(next.clone(), window, cx);
                assert!(this.focus.is_focused(window));
            })
        });
        cx.simulate_keystrokes("escape");
        click(cx, "tidy");
        click(cx, "reply");
        cx.dispatch_action(crate::commands::TidyGraph);
        cx.dispatch_action(crate::commands::NewProject);
        cx.dispatch_action(crate::commands::OpenProject);
        cx.dispatch_action(crate::commands::ImportConversation);
        cx.dispatch_action(crate::commands::SaveProjectAs);
        cx.dispatch_action(crate::commands::SaveProject);
        cx.dispatch_action(crate::commands::Quit);
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                assert!(!this.request_close(window, cx));
                assert!(matches!(this.modal, Some(Modal::ChangingVault)));
                assert_eq!(this.editor.project, original);
                assert_eq!(
                    this.desktop.notes_directory().unwrap(),
                    directory.path().join("vault")
                );
            })
        });
        let _ = release.send(());
        assert!(
            holder.join().unwrap(),
            "Vault switching blocked native input"
        );
        cx.condition(&workspace, |this, _| {
            !matches!(this.modal, Some(Modal::ChangingVault))
        })
        .await;
        workspace.update(cx, |this, _| {
            this.editor.project.graph.nodes["question"].content =
                "Recovery must resume after failure".into();
            this.editor.touch();
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.change_notes_directory("invalid-relative-path".into(), window, cx)
            })
        });
        cx.condition(&workspace, |this, _| {
            !matches!(this.modal, Some(Modal::ChangingVault)) && !this.save_in_flight
        })
        .await;
        workspace.read_with(cx, |this, _| {
            assert!(this.editor.is_dirty());
            assert!(this
                .notice
                .as_ref()
                .unwrap()
                .contains("existing absolute path"));
            assert_eq!(this.desktop.notes_directory().unwrap(), next);
            assert_eq!(this.snapshot_revision, Some(this.editor.edit_revision));
            assert!(this
                .desktop
                .list_project_recovery()
                .unwrap()
                .iter()
                .any(|entry| this
                    .desktop
                    .read_project_recovery(&entry.id)
                    .unwrap()
                    .contains("Recovery must resume after failure")));
        });
    }

    #[gpui::test]
    fn file_context_actions_copy_relative_path_and_reload_live_metadata(cx: &mut TestAppContext) {
        let (directory, workspace, cx, _) = workspace(cx);
        let path = directory.path().join("vault/reference.md");
        std::fs::write(&path, "before").unwrap();
        workspace.update(cx, |this, cx| {
            this.add_files(vec![path.clone()], Position::default(), cx)
        });
        cx.run_until_parked();
        let id = workspace.read_with(cx, |this, _| {
            this.editor
                .project
                .graph
                .nodes
                .values()
                .find(|node| node.file_data().is_some())
                .unwrap()
                .id
                .clone()
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.open_modal(
                    Modal::Context {
                        node: Some(id.clone()),
                        position: Position::default(),
                    },
                    window,
                    cx,
                )
            });
        });
        cx.run_until_parked();
        let copy = cx
            .debug_bounds("context-copy-path")
            .expect("Copy path is visible")
            .center();
        cx.simulate_click(copy, Modifiers::default());
        assert_eq!(
            cx.read_from_clipboard()
                .and_then(|item| item.text())
                .as_deref(),
            Some("reference.md")
        );
        workspace.read_with(cx, |this, _| assert!(this.modal.is_none()));

        std::fs::write(&path, "updated content is longer").unwrap();
        let before = workspace.read_with(cx, |this, _| this.editor.edit_revision);
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.open_modal(
                    Modal::Context {
                        node: Some(id.clone()),
                        position: Position::default(),
                    },
                    window,
                    cx,
                )
            });
        });
        cx.run_until_parked();
        let reload = cx
            .debug_bounds("context-reload-file")
            .expect("Reload is visible")
            .center();
        cx.simulate_click(reload, Modifiers::default());
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            let file = this.editor.project.graph.nodes[&id].file_data().unwrap();
            assert_eq!(file.size, "updated content is longer".len() as u64);
            assert_eq!(file.seen_size, file.size);
            assert!(this.editor.edit_revision > before);
            assert!(this.editor.is_dirty());
            assert!(this.modal.is_none());
        });
    }

    #[gpui::test]
    fn settings_closes_with_its_button_escape_and_outside_click(cx: &mut TestAppContext) {
        let (_directory, workspace, cx, _) = workspace(cx);
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| this.open_modal(Modal::Settings, window, cx));
        });
        cx.run_until_parked();
        let close = cx
            .debug_bounds("dialog-close")
            .expect("Close button is visible")
            .center();
        cx.simulate_click(close, Modifiers::default());
        workspace.read_with(cx, |this, _| assert!(this.modal.is_none()));
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| this.open_modal(Modal::Settings, window, cx));
        });
        cx.simulate_keystrokes("escape");
        workspace.read_with(cx, |this, _| assert!(this.modal.is_none()));
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| this.open_modal(Modal::Settings, window, cx));
        });
        cx.run_until_parked();
        cx.simulate_click(point(px(2.), px(300.)), Modifiers::default());
        workspace.read_with(cx, |this, _| assert!(this.modal.is_none()));
    }

    #[gpui::test]
    async fn setup_keeps_invalid_directory_errors_visible_and_advances_after_a_valid_choice(
        cx: &mut TestAppContext,
    ) {
        let (directory, workspace, cx, _) = workspace(cx);
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.open_modal(Modal::Setup, window, cx);
                this.change_notes_directory("not-an-absolute-directory".into(), window, cx);
                assert!(matches!(this.modal, Some(Modal::ChangingVault)));
            });
        });
        cx.executor().allow_parking();
        cx.condition(&workspace, |this, _| {
            matches!(this.modal, Some(Modal::Setup))
        })
        .await;
        assert!(workspace.read_with(cx, |this, _| this.notice.is_some()));
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.change_notes_directory(directory.path().join("vault"), window, cx)
            })
        });
        cx.condition(&workspace, |this, _| {
            matches!(this.modal, Some(Modal::Projects))
        })
        .await;
        workspace.read_with(cx, |this, _| {
            assert_eq!(
                this.desktop.notes_directory().unwrap(),
                directory.path().join("vault")
            );
            assert!(matches!(this.modal, Some(Modal::Projects)));
            assert!(this.notice.is_none());
        });
    }

    #[gpui::test]
    fn project_chooser_loads_in_background_and_ignores_a_closed_dialog(cx: &mut TestAppContext) {
        let (directory, workspace, cx, _) = workspace(cx);
        let path = directory.path().join("vault/listed.thoughttree");
        std::fs::write(&path, Project::default().to_json().unwrap()).unwrap();
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.open_modal(Modal::Projects, window, cx);
                assert!(this.dialogs.loading);
                this.open_modal(Modal::Settings, window, cx);
            });
        });
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert!(matches!(this.modal, Some(Modal::Settings)));
            assert!(this.dialogs.projects.is_empty());
            assert!(this.notice.is_none());
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| this.open_modal(Modal::Projects, window, cx));
        });
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert!(!this.dialogs.loading);
            assert_eq!(this.dialogs.projects.len(), 1);
            assert_eq!(this.dialogs.projects[0].relative_path, "listed.thoughttree");
            assert!(this.dialogs.projects[0].modified_epoch_ms > 0);
        });
    }

    #[gpui::test]
    async fn settings_preferences_are_independent_and_survive_save_and_project_replacement(
        cx: &mut TestAppContext,
    ) {
        let (directory, workspace, cx, _) = workspace(cx);
        // Config writes run on a real thread so a held cross-process lock cannot block input.
        cx.executor().allow_parking();
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.open_modal(Modal::Settings, window, cx);
                this.choose_model(AgentProvider::Codex, Some("global-model".into()), cx);
                this.choose_effort(AgentProvider::Codex, Some(ReasoningEffort::XHigh), cx);
                this.dialogs.scope = Scope::Project;
                this.choose_model(AgentProvider::Codex, Some("project-model".into()), cx);
                this.choose_effort(AgentProvider::Codex, Some(ReasoningEffort::Low), cx);
                this.choose_model(AgentProvider::Codex, None, cx);
                assert_eq!(
                    this.editor
                        .project
                        .project_effort_preferences
                        .as_ref()
                        .unwrap()["codex"],
                    thoughttree_gpui_model::ReasoningEffort::Low
                );
                this.choose_model(AgentProvider::Codex, Some("project-model".into()), cx);
                this.save(window, cx);
            });
        });
        cx.simulate_new_path_selection(|directory| Some(directory.join("preferences.thoughttree")));
        cx.run_until_parked();
        cx.executor().allow_parking();
        cx.condition(&workspace, |this, _| !this.config_is_busy())
            .await;
        let path = directory.path().join("vault/preferences.thoughttree");
        let project = Project::from_json(&std::fs::read_to_string(&path).unwrap()).unwrap();
        assert_eq!(
            project.project_model_preferences.as_ref().unwrap()["codex"],
            "project-model"
        );
        assert_eq!(
            project.project_effort_preferences.as_ref().unwrap()["codex"],
            thoughttree_gpui_model::ReasoningEffort::Low
        );
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| this.open_path(path, window, cx));
        });
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert!(matches!(this.modal, Some(Modal::Settings)));
            let config = this.desktop.config();
            assert_eq!(
                config.model_preferences.get(&AgentProvider::Codex).unwrap(),
                "global-model"
            );
            assert_eq!(
                config.effort_preferences.get(&AgentProvider::Codex),
                Some(&ReasoningEffort::XHigh)
            );
            assert_eq!(
                this.editor.project.project_model_preferences,
                project.project_model_preferences
            );
            assert_eq!(
                this.editor.project.project_effort_preferences,
                project.project_effort_preferences
            );
        });
    }

    #[gpui::test]
    fn compare_keeps_both_snapshots_without_adopting_the_disk_revision(cx: &mut TestAppContext) {
        let (directory, workspace, cx, _) = workspace(cx);
        let path = directory.path().join("vault/compare.thoughttree");
        let desktop = workspace.read_with(cx, |this, _| this.desktop.clone());
        let original = workspace.read_with(cx, |this, _| this.editor.project.clone());
        let revision = desktop
            .save_project(&path, &original.to_json().unwrap(), None)
            .unwrap();
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.install_project(
                    original.clone(),
                    Some(path.clone()),
                    Some(revision.clone()),
                    "Compare".into(),
                    window,
                    cx,
                );
                this.editor
                    .edit(|project| {
                        project.graph.nodes["question"].content = "Local change".into();
                        Ok(())
                    })
                    .unwrap();
            });
        });
        let mut external = original;
        external.graph.nodes["question"].content = "External change".into();
        std::fs::write(&path, external.to_json().unwrap()).unwrap();
        workspace.update(cx, |this, cx| {
            this.modal = Some(Modal::Conflict);
            this.compare_versions(cx);
        });
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert!(this.dialogs.compare);
            assert!(this
                .dialogs
                .local
                .as_ref()
                .unwrap()
                .contains("Local change"));
            assert!(this
                .dialogs
                .disk
                .as_ref()
                .unwrap()
                .contains("External change"));
            assert_eq!(this.revision.as_ref(), Some(&revision));
            assert!(this.editor.is_dirty());
            assert_eq!(
                this.editor.project.graph.nodes["question"].content,
                "Local change"
            );
        });
        assert_eq!(
            desktop.load_project(&path).unwrap().content,
            external.to_json().unwrap()
        );
    }

    #[gpui::test]
    fn close_waits_for_an_in_flight_provider_path_save(cx: &mut TestAppContext) {
        let (_directory, workspace, cx, events) = workspace(cx);
        workspace.update(cx, |this, _| {
            this.save_provider_path(AgentProvider::Codex, None);
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                assert!(this.config_is_busy());
                assert!(!this.request_close(window, cx));
            })
        });
        events
            .try_send(DesktopEvent::ProviderPathValidated {
                provider: AgentProvider::Codex,
                result: Ok("Using automatic discovery".into()),
                current: true,
            })
            .unwrap();
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| assert!(!this.config_is_busy()));
    }

    #[gpui::test]
    async fn activation_adopts_settings_and_vault_from_the_other_frontend(cx: &mut TestAppContext) {
        let (directory, workspace, cx, _) = workspace(cx);
        cx.executor().allow_parking();
        let next = directory.path().join("next");
        std::fs::create_dir(&next).unwrap();
        let (other, _) =
            thoughttree_desktop::Desktop::open(directory.path().join("config")).unwrap();
        other
            .set_default_provider(AgentProvider::ClaudeCode)
            .unwrap();
        other.set_notes_directory(next.clone()).unwrap();
        cx.update(|window, cx| workspace.update(cx, |this, cx| this.reload_config(window, cx)));
        cx.condition(&workspace, |this, _| {
            this.desktop.notes_directory().ok().as_ref() == Some(&next)
                && !matches!(this.modal, Some(Modal::ChangingVault))
        })
        .await;
        workspace.read_with(cx, |this, _| {
            assert_eq!(this.provider, AgentProvider::ClaudeCode);
            assert_eq!(
                this.desktop.config().default_provider,
                AgentProvider::ClaudeCode
            );
        });
    }

    #[gpui::test]
    async fn a_delayed_default_provider_write_applies_after_project_replacement(
        cx: &mut TestAppContext,
    ) {
        let (directory, workspace, cx, _) = workspace(cx);
        cx.executor().allow_parking();
        let (release, holder) = hold_config_lock(directory.path());
        workspace.update(cx, |this, cx| {
            this.provider = AgentProvider::Codex;
            this.enqueue_config(ConfigWrite::Provider(AgentProvider::ClaudeCode), cx);
            // Opening another Project while the write waits for the lock.
            this.generation = this.generation.wrapping_add(1);
        });
        let _ = release.send(());
        holder.join().unwrap();
        cx.condition(&workspace, |this, _| !this.config_is_busy())
            .await;
        workspace.read_with(cx, |this, _| {
            assert_eq!(this.provider, AgentProvider::ClaudeCode);
        });
    }

    #[gpui::test]
    fn close_stays_possible_after_a_project_change_during_the_close_snapshot(
        cx: &mut TestAppContext,
    ) {
        let (_directory, workspace, cx, _) = workspace(cx);
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.editor.touch();
                assert!(!this.request_close(window, cx));
                assert!(this.closing);
                // A Save As or Project replacement finishing meanwhile.
                this.generation = this.generation.wrapping_add(1);
            })
        });
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| assert!(!this.closing));
    }

    #[gpui::test]
    async fn reloaded_settings_apply_even_when_the_project_changes_meanwhile(
        cx: &mut TestAppContext,
    ) {
        let (directory, workspace, cx, _) = workspace(cx);
        cx.executor().allow_parking();
        let (other, _) =
            thoughttree_desktop::Desktop::open(directory.path().join("config")).unwrap();
        other
            .set_default_provider(AgentProvider::ClaudeCode)
            .unwrap();
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.provider = AgentProvider::Codex;
                this.reload_config(window, cx);
                // Opening another Project before the reload completes.
                this.generation = this.generation.wrapping_add(1);
            })
        });
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert_eq!(this.provider, AgentProvider::ClaudeCode);
        });
    }

    #[gpui::test]
    async fn activations_during_a_reload_queue_exactly_one_more(cx: &mut TestAppContext) {
        let (directory, workspace, cx, _) = workspace(cx);
        cx.executor().allow_parking();
        let (other, _) =
            thoughttree_desktop::Desktop::open(directory.path().join("config")).unwrap();
        other
            .set_default_provider(AgentProvider::ClaudeCode)
            .unwrap();
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.provider = AgentProvider::Codex;
                this.reload_config(window, cx);
                this.reload_config(window, cx);
                this.reload_config(window, cx);
                assert!(this.dialogs.config_reloading && this.dialogs.config_reload_again);
            })
        });
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert!(!this.dialogs.config_reloading && !this.dialogs.config_reload_again);
            assert_eq!(this.provider, AgentProvider::ClaudeCode);
        });
    }

    #[gpui::test]
    async fn a_failed_vault_transition_restarts_file_inspection(cx: &mut TestAppContext) {
        let (directory, workspace, cx, _) = workspace(cx);
        cx.executor().allow_parking();
        let file = directory.path().join("vault/notes.md");
        std::fs::write(&file, "Vault file").unwrap();
        workspace.update(cx, |this, cx| {
            this.canvas.update(cx, |_, cx| {
                cx.emit(crate::canvas::GraphEvent::DropFiles(
                    vec![file],
                    Position { x: 0., y: 0. },
                ));
            });
        });
        cx.run_until_parked();
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                assert_eq!(this.file_status.len(), 1);
                this.refresh_files(cx);
                this.change_notes_directory(directory.path().join("unmounted"), window, cx);
            })
        });
        cx.condition(&workspace, |this, _| {
            !matches!(this.modal, Some(Modal::ChangingVault)) && !this.saving()
        })
        .await;
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert!(this.notice.is_some());
            assert_eq!(this.file_status.len(), 1);
        });
    }

    #[gpui::test]
    async fn a_reload_skipped_while_settings_are_busy_runs_when_they_finish(
        cx: &mut TestAppContext,
    ) {
        let (directory, workspace, cx, events) = workspace(cx);
        cx.executor().allow_parking();
        let (other, _) =
            thoughttree_desktop::Desktop::open(directory.path().join("config")).unwrap();
        other
            .set_default_provider(AgentProvider::ClaudeCode)
            .unwrap();
        let validated = |current| DesktopEvent::ProviderPathValidated {
            provider: AgentProvider::Codex,
            result: Ok("Using automatic discovery".into()),
            current,
        };
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.provider = AgentProvider::Codex;
                this.save_provider_path(AgentProvider::Codex, None);
                this.save_provider_path(AgentProvider::Codex, None);
                this.reload_config(window, cx);
                assert!(this.dialogs.config_reload_again && !this.dialogs.config_reloading);
            })
        });
        // Every request reports once; the first, superseded one keeps
        // settings busy until the latest reports too.
        events.try_send(validated(false)).unwrap();
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert!(this.config_is_busy());
            assert_eq!(this.provider, AgentProvider::Codex);
        });
        events.try_send(validated(true)).unwrap();
        cx.condition(&workspace, |this, _| {
            this.provider == AgentProvider::ClaudeCode
        })
        .await;
    }

    #[gpui::test]
    fn a_vault_change_waits_for_attachments_being_prepared(cx: &mut TestAppContext) {
        let (directory, workspace, cx, _) = workspace(cx);
        let image = directory.path().join("figure.png");
        image::RgbImage::from_pixel(2, 2, image::Rgb([1, 2, 3]))
            .save(&image)
            .unwrap();
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| {
                this.attach_images_to("question".into(), vec![image.clone()], cx);
                this.change_notes_directory(directory.path().to_owned(), window, cx);
                assert!(!matches!(this.modal, Some(Modal::ChangingVault)));
                assert!(this.notice.as_deref().unwrap().contains("Wait"));
            })
        });
    }
}
