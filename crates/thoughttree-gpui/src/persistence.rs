//! Window persistence: every background result is tied to a Project generation
//! and a captured edit revision. Replacing a Project always snapshots dirty
//! work first; a late result cannot replace edits made while I/O was running.
use std::path::PathBuf;

use gpui::{Context, PathPromptOptions, Window};
use thoughttree_desktop::{Desktop, Revision, VaultError};
use thoughttree_gpui_model::{now_ms, Project, TurnRange};

use crate::{dialogs::Modal, workspace::Workspace};

struct LoadedProject {
    project: Project,
    path: Option<PathBuf>,
    revision: Option<Revision>,
    title: String,
    unsaved: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct Transition {
    generation: u64,
    edit: u64,
    request: u64,
}

impl Transition {
    fn matches_project(self, generation: u64, request: u64) -> bool {
        self.generation == generation && self.request == request
    }

    fn may_replace(self, generation: u64, edit: u64, request: u64) -> bool {
        self.matches_project(generation, request) && self.edit == edit
    }
}

impl Workspace {
    pub(crate) fn save(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return;
        }
        if self.project_path.is_none() {
            self.save_as(window, cx);
        } else {
            self.save_current(cx);
        }
    }

    pub(crate) fn save_current(&mut self, cx: &mut Context<Self>) {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return;
        }
        if !self.editor.is_dirty() {
            return;
        }
        if self.save_in_flight {
            self.save_queued = true;
            return;
        }
        let write_file =
            self.project_path.is_some() && !matches!(self.modal, Some(Modal::Conflict));
        if !write_file && self.snapshot_revision == Some(self.editor.edit_revision) {
            return;
        }
        let data = match self.editor.project.to_json() {
            Ok(data) => data,
            Err(error) => {
                self.set_persistence_error(error.to_string());
                cx.notify();
                return;
            }
        };
        let desktop = self.desktop.clone();
        let path = self.project_path.clone();
        let base = self.revision.clone();
        let edit = self.editor.edit_revision;
        let generation = self.generation;
        let epoch = self.save_epoch;
        self.save_in_flight = true;
        self.save_queued = false;
        let job = cx.background_executor().spawn(async move {
            desktop.snapshot_project(path.as_deref(), &data)?;
            if write_file {
                desktop
                    .save_project(
                        path.as_deref().expect("Named Project"),
                        &data,
                        base.as_ref(),
                    )
                    .map(Some)
            } else {
                Ok(None)
            }
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                this.complete_save(generation, epoch, edit, result, cx)
            });
        })
        .detach();
    }

    fn complete_save(
        &mut self,
        generation: u64,
        epoch: u64,
        edit: u64,
        result: Result<Option<Revision>, VaultError>,
        cx: &mut Context<Self>,
    ) {
        // Check before changing *any* bookkeeping. A new Project or path may
        // already have its own save in flight with the same field.
        if generation != self.generation || epoch != self.save_epoch {
            return;
        }
        self.save_in_flight = false;
        let queued = std::mem::take(&mut self.save_queued);
        let succeeded = result.is_ok();
        match result {
            Ok(revision) => {
                self.clear_persistence_error();
                self.snapshot_revision = Some(edit);
                if let Some(revision) = revision {
                    self.revision = Some(revision);
                    self.editor.mark_saved(edit);
                    self.saved_at = Some(now_ms());
                }
            }
            Err(VaultError::Stale { .. }) => {
                self.dialogs.reset_conflict();
                self.modal = Some(Modal::Conflict);
            }
            Err(error) => self.set_persistence_error(error.to_string()),
        }
        if queued || (succeeded && self.editor.edit_revision != edit) {
            self.save_current(cx);
        }
        cx.notify();
    }

    /// A save or Save As is writing the open graph.
    pub(crate) fn saving(&self) -> bool {
        self.save_in_flight || self.save_as_jobs > 0
    }

    fn set_persistence_error(&mut self, error: String) {
        self.notice = Some(error.clone());
        self.persistence_error = Some(error);
    }

    fn clear_persistence_error(&mut self) {
        if let Some(error) = self.persistence_error.take() {
            if self.notice.as_ref() == Some(&error) {
                self.notice = None;
            }
        }
    }

    pub(crate) fn install_project(
        &mut self,
        project: Project,
        path: Option<PathBuf>,
        revision: Option<Revision>,
        title: String,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if let Err(error) = self.editor.replace_project(project) {
            self.notice = Some(error.to_string());
            cx.notify();
            return;
        }
        self.generation = self.generation.wrapping_add(1);
        self.clear_persistence_error();
        self.notice = None;
        self.transition_request = self.transition_request.wrapping_add(1);
        self.autosave = None;
        self.save_in_flight = false;
        self.save_queued = false;
        self.snapshot_revision = None;
        self.closing = false;
        self.project_path = path;
        self.revision = revision;
        self.title = title;
        self.preview_id = None;
        self.selected_edge = None;
        self.selected_model = None;
        self.editing = false;
        if !matches!(self.modal, Some(Modal::Settings)) {
            self.modal = None;
        }
        self.palette = None;
        self.permissions.clear();
        self.dismiss_mentions();
        self.reset_transient_jobs();
        self.dialogs.reset_project();
        self.file_refresh = None;
        self.file_status.clear();
        self.file_previews.clear();
        self.file_preview_errors.clear();
        self.file_preview = None;
        self.saved_at = None;
        self.focus.focus(window, cx);
        self.refresh(cx);
        self.refresh_files(cx);
        self.schedule_summaries(cx);
        self.canvas.update(cx, |canvas, cx| canvas.fit(cx));
    }

    pub(crate) fn open_path(&mut self, path: PathBuf, window: &mut Window, cx: &mut Context<Self>) {
        self.replace_after_io(
            move |desktop| {
                let path = desktop.project_path(&path).map_err(|e| e.to_string())?;
                let doc = desktop.load_project(&path).map_err(|e| e.to_string())?;
                let project = Project::from_json(&doc.content).map_err(|e| e.to_string())?;
                Ok(LoadedProject {
                    project,
                    title: project_title(&path),
                    path: Some(path),
                    revision: Some(doc.revision),
                    unsaved: false,
                })
            },
            window,
            cx,
        );
    }

    pub(crate) fn restore_recovery(
        &mut self,
        id: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let id = id.to_owned();
        self.replace_after_io(
            move |desktop| {
                let content = desktop
                    .read_project_recovery(&id)
                    .map_err(|e| e.to_string())?;
                let project = Project::from_json(&content).map_err(|e| e.to_string())?;
                Ok(LoadedProject {
                    project,
                    path: None,
                    revision: None,
                    title: "Recovered Project".into(),
                    unsaved: true,
                })
            },
            window,
            cx,
        );
    }

    pub(crate) fn save_conflict_copy(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(source) = self.project_path.clone() else {
            self.notice = Some("Project has no path".into());
            cx.notify();
            return;
        };
        let project = self.editor.project.clone();
        self.replace_after_io(
            move |desktop| {
                let content = project.to_json().map_err(|e| e.to_string())?;
                let (path, revision) = desktop
                    .save_project_copy(&source, &content)
                    .map_err(|e| e.to_string())?;
                Ok(LoadedProject {
                    project,
                    title: project_title(&path),
                    path: Some(path),
                    revision: Some(revision),
                    unsaved: false,
                })
            },
            window,
            cx,
        );
    }

    /// Save As captures only after the picker returns. A cancelled picker has
    /// no effects on the open Project. Destination changes become visible only
    /// after its guarded write succeeds; later edits stay dirty and are queued.
    pub(crate) fn save_as(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return;
        }
        let directory = match self.desktop.notes_directory() {
            Ok(path) => path,
            Err(error) => {
                self.notice = Some(error);
                self.open_modal(Modal::Setup, window, cx);
                return;
            }
        };
        let picker = cx.prompt_for_new_path(&directory, Some("Untitled.thoughttree"));
        let generation = self.generation;
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            let _ = this.update_in(cx, |this, _, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(Ok(Some(mut path))) => {
                        path.set_extension("thoughttree");
                        this.save_to_new_path(path, cx);
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
                }
            });
        })
        .detach();
    }

    pub(crate) fn save_to_new_path(&mut self, path: PathBuf, cx: &mut Context<Self>) {
        if self.project_path.as_ref() == Some(&path) {
            self.save_current(cx);
            return;
        }
        let data = match self.editor.project.to_json() {
            Ok(data) => data,
            Err(error) => {
                self.notice = Some(error.to_string());
                cx.notify();
                return;
            }
        };
        let ticket = self.transition();
        let desktop = self.desktop.clone();
        self.save_as_jobs += 1;
        let job = cx.background_executor().spawn(async move {
            let path = desktop.project_path(&path).map_err(|e| e.to_string())?;
            let revision = desktop.save_project(&path, &data, None).map_err(|error| match error {
                VaultError::Stale { .. } => "That Project file already exists. Choose a new name to preserve both versions.".into(),
                other => other.to_string(),
            })?;
            Ok::<_, String>((path, revision))
        });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| this.complete_save_as(ticket, result, cx));
        })
        .detach();
    }

    fn complete_save_as(
        &mut self,
        ticket: Transition,
        result: Result<(PathBuf, Revision), String>,
        cx: &mut Context<Self>,
    ) {
        self.save_as_jobs -= 1;
        if !ticket.matches_project(self.generation, self.transition_request) {
            cx.notify();
            return;
        }
        match result {
            Ok((path, revision)) => {
                self.clear_persistence_error();
                // The graph is the same even if streaming advanced. Only
                // callbacks still saving its former path are stale.
                self.save_epoch = self.save_epoch.wrapping_add(1);
                self.save_in_flight = false;
                self.save_queued = false;
                self.autosave = None;
                self.snapshot_revision = Some(ticket.edit);
                self.title = project_title(&path);
                self.project_path = Some(path.clone());
                self.revision = Some(revision);
                self.editor.mark_saved(ticket.edit);
                self.saved_at = Some(now_ms());
                self.enqueue_config(crate::dialogs::ConfigWrite::AddRecent(path), cx);
                if self.editor.is_dirty() {
                    self.save_current(cx);
                }
            }
            Err(error) => self.set_persistence_error(error),
        }
        cx.notify();
    }

    pub(crate) fn new_project(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_replace(cx) {
            return;
        }
        let directory = match self.desktop.notes_directory() {
            Ok(path) => path,
            Err(error) => {
                self.notice = Some(error);
                self.open_modal(Modal::Setup, window, cx);
                return;
            }
        };
        let picker = cx.prompt_for_new_path(&directory, Some("Untitled.thoughttree"));
        let generation = self.generation;
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(Ok(Some(mut path))) => {
                        path.set_extension("thoughttree");
                        this.replace_after_io(
                            move |desktop| new_empty_project(desktop, path),
                            window,
                            cx,
                        );
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
                }
            });
        })
        .detach();
    }

    pub(crate) fn import_dialog(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.can_replace(cx) {
            return;
        }
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: false,
            prompt: Some("Import Kagi Assistant JSON".into()),
        });
        let generation = self.generation;
        cx.spawn_in(window, async move |this, cx| {
            let result = picker.await;
            let _ = this.update_in(cx, |this, window, cx| {
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(Ok(Some(paths))) => {
                        this.import_picked(paths, window, cx);
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
                }
            });
        })
        .detach();
    }

    pub(crate) fn import_picked(
        &mut self,
        paths: Vec<PathBuf>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(path) = paths.into_iter().next() else {
            return;
        };
        self.replace_after_io(move |_| import_kagi_project(path), window, cx);
    }

    fn transition(&mut self) -> Transition {
        self.transition_request = self.transition_request.wrapping_add(1);
        Transition {
            generation: self.generation,
            edit: self.editor.edit_revision,
            request: self.transition_request,
        }
    }

    fn close_blocker(&self) -> Option<&'static str> {
        if self.save_as_jobs > 0 {
            Some("Wait for Save As to finish before closing")
        } else if self.pending_edits > 0 {
            Some("Wait for attachments to finish before closing")
        } else if self.config_is_busy() {
            Some("Wait for settings to finish saving before closing")
        } else {
            None
        }
    }

    fn can_replace(&mut self, cx: &mut Context<Self>) -> bool {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return false;
        }
        // A replacement would discard the Save As result or the pending edit.
        let blocker = if !self.editor.active_turns.is_empty() {
            "Wait for the active Turn before changing Projects"
        } else if self.save_as_jobs > 0 {
            "Wait for Save As to finish before changing Projects"
        } else if self.pending_edits > 0 {
            "Wait for attachments to finish before changing Projects"
        } else {
            return true;
        };
        self.notice = Some(blocker.into());
        cx.notify();
        false
    }

    fn replace_after_io(
        &mut self,
        load: impl FnOnce(Desktop) -> Result<LoadedProject, String> + Send + 'static,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !self.can_replace(cx) {
            return;
        }
        let previous = if self.editor.is_dirty() {
            match self.editor.project.to_json() {
                Ok(data) => Some((self.project_path.clone(), data)),
                Err(error) => {
                    self.notice = Some(error.to_string());
                    cx.notify();
                    return;
                }
            }
        } else {
            None
        };
        let ticket = self.transition();
        let desktop = self.desktop.clone();
        let job = cx.background_executor().spawn(async move {
            if let Some((path, data)) = previous {
                desktop
                    .snapshot_project(path.as_deref(), &data)
                    .map_err(|e| e.to_string())?;
            }
            load(desktop)
        });
        cx.spawn_in(window, async move |this, cx| {
            let result = job.await;
            let _ = this.update_in(cx, |this, window, cx| {
                this.complete_replacement(ticket, result, window, cx)
            });
        })
        .detach();
    }

    fn complete_replacement(
        &mut self,
        ticket: Transition,
        result: Result<LoadedProject, String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        if !ticket.matches_project(self.generation, self.transition_request) {
            return;
        }
        if !ticket.may_replace(
            self.generation,
            self.editor.edit_revision,
            self.transition_request,
        ) {
            self.notice = Some("The graph changed while the Project was loading. Your current edits are still open; try again.".into());
            cx.notify();
            return;
        }
        match result {
            Ok(loaded) => {
                if !self.can_replace(cx) {
                    return;
                }
                let path = loaded.path.clone();
                self.install_project(
                    loaded.project,
                    loaded.path,
                    loaded.revision,
                    loaded.title,
                    window,
                    cx,
                );
                if let Some(path) = path {
                    self.enqueue_config(crate::dialogs::ConfigWrite::AddRecent(path), cx);
                }
                if loaded.unsaved {
                    self.editor.touch();
                    self.save_current(cx);
                }
            }
            Err(error) => {
                self.notice = Some(error);
                cx.notify();
            }
        }
    }

    /// A native close is deferred until the exact current edits have a durable
    /// snapshot. A new edit during I/O keeps the window open.
    pub(crate) fn request_close(&mut self, window: &mut Window, cx: &mut Context<Self>) -> bool {
        if let Some(reason) = self.close_blocker() {
            self.notice = Some(reason.into());
            cx.notify();
            return false;
        }
        if !self.can_replace(cx) {
            return false;
        }
        if !self.editor.is_dirty() {
            return true;
        }
        if self.closing {
            return false;
        }
        let data = match self.editor.project.to_json() {
            Ok(data) => data,
            Err(error) => {
                self.notice = Some(error.to_string());
                cx.notify();
                return false;
            }
        };
        self.closing = true;
        let desktop = self.desktop.clone();
        let path = self.project_path.clone();
        let edit = self.editor.edit_revision;
        let generation = self.generation;
        let job = cx
            .background_executor()
            .spawn(async move { desktop.snapshot_project(path.as_deref(), &data) });
        cx.spawn_in(window, async move |this, cx| {
            let result = job.await;
            let _ = this.update_in(cx, |this, window, cx| {
                // Release the latch even for a replaced Project, or no later
                // close request could proceed.
                this.closing = false;
                if this.generation != generation {
                    return;
                }
                match result {
                    Ok(_)
                        if this.editor.edit_revision == edit
                            && this.editor.active_turns.is_empty() =>
                    {
                        window.remove_window()
                    }
                    Ok(_) => {
                        this.notice = Some(
                            "New edits arrived while closing. Close again to preserve them.".into(),
                        );
                        cx.notify();
                    }
                    Err(error) => {
                        this.notice =
                            Some(format!("Cannot preserve edits before closing: {error}"));
                        cx.notify();
                    }
                }
            });
        })
        .detach();
        false
    }

    pub(crate) fn export(&mut self, all: bool, window: &mut Window, cx: &mut Context<Self>) {
        if matches!(self.modal, Some(Modal::ChangingVault)) {
            return;
        }
        let graph = &self.editor.project.graph;
        let ids: Vec<_> = if all {
            graph.nodes.keys().cloned().collect()
        } else {
            self.editor
                .selection
                .first()
                .map(|id| graph.conversation_path_ids(id))
                .unwrap_or_default()
        };
        let content = graph.export_markdown(&ids);
        let picker = cx.prompt_for_new_path(
            &self.desktop.notes_directory().unwrap_or_default(),
            Some(if all {
                "full-export.md"
            } else {
                "conversation-export.md"
            }),
        );
        let desktop = self.desktop.clone();
        cx.spawn_in(window, async move |this, cx| {
            let result = match picker.await {
                Ok(Ok(Some(path))) => {
                    cx.background_executor()
                        .spawn(async move { desktop.export_markdown(&path, &content) })
                        .await
                }
                Ok(Ok(None)) => return,
                Ok(Err(error)) => Err(error.to_string()),
                Err(error) => Err(error.to_string()),
            };
            let _ = this.update_in(cx, |this, _, cx| {
                this.notice = Some(
                    result
                        .map(|_| "Markdown exported".into())
                        .unwrap_or_else(|e| e),
                );
                cx.notify();
            });
        })
        .detach();
    }
}

fn new_empty_project(desktop: Desktop, path: PathBuf) -> Result<LoadedProject, String> {
    let path = desktop.project_path(&path).map_err(|e| e.to_string())?;
    let project = Project::default();
    let data = project.to_json().map_err(|e| e.to_string())?;
    let revision = desktop
        .save_project(&path, &data, None)
        .map_err(|e| e.to_string())?;
    Ok(LoadedProject {
        project,
        title: project_title(&path),
        path: Some(path),
        revision: Some(revision),
        unsaved: false,
    })
}

fn import_kagi_project(path: PathBuf) -> Result<LoadedProject, String> {
    let source = thoughttree_desktop::read_kagi_export(&path).map_err(|e| e.to_string())?;
    let imported = thoughttree_gpui_model::parse_kagi_export(&source).map_err(|e| e.to_string())?;
    let graph = thoughttree_gpui_model::conversation_to_graph(&imported, TurnRange::default());
    Ok(LoadedProject {
        project: Project::from(graph),
        title: imported.import_key,
        path: None,
        revision: None,
        unsaved: true,
    })
}

fn project_title(path: &std::path::Path) -> String {
    path.file_stem()
        .unwrap_or_default()
        .to_string_lossy()
        .into_owned()
}

#[cfg(test)]
mod tests {
    use super::Transition;

    #[test]
    fn an_earlier_open_cannot_replace_a_later_request_or_edited_graph() {
        let ticket = Transition {
            generation: 3,
            edit: 8,
            request: 4,
        };
        assert!(ticket.may_replace(3, 8, 4));
        assert!(
            !ticket.may_replace(3, 9, 4),
            "edits during recovery snapshot stay open"
        );
        assert!(
            !ticket.may_replace(4, 8, 4),
            "another Project has its own lifetime"
        );
        assert!(!ticket.may_replace(3, 8, 5), "the newest user choice wins");
    }

    #[test]
    fn save_as_may_adopt_its_saved_path_but_must_leave_later_edits_dirty() {
        let ticket = Transition {
            generation: 3,
            edit: 8,
            request: 4,
        };
        assert!(ticket.matches_project(3, 4));
        let mut editor = thoughttree_gpui_model::Editor::default();
        editor.edit_revision = 9;
        editor.mark_saved(ticket.edit);
        assert!(editor.is_dirty());
    }
}
