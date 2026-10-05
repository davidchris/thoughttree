use std::{
    io::{Cursor, Read},
    path::PathBuf,
    time::Duration,
};

use base64::Engine;
use gpui::{
    prelude::*, ClipboardEntry, Context, EntityInputHandler, PathPromptOptions, ScrollHandle, Task,
    Window,
};
use thoughttree_core::vault::files::{
    is_raster_image,
    limits::{IMAGE_MAX_BYTES, IMAGE_MAX_SIDE},
};
use thoughttree_desktop::{Desktop, VaultFileStatus};
use thoughttree_gpui_model::{now_ms, FileData, GraphNode, ImageAttachment, NodeKind, Position};

use crate::workspace::Workspace;

#[derive(Default)]
pub(crate) struct MentionState {
    pub(crate) open: bool,
    pub(crate) loading: bool,
    pub(crate) error: Option<String>,
    pub(crate) scroll: ScrollHandle,
    request: u64,
    completed_value: Option<String>,
    task: Option<Task<()>>,
}

impl Workspace {
    pub(crate) fn pick_file(
        &mut self,
        position: Position,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let request = FileRequest::new(self);
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Add files from notes directory".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picker.await else {
                return;
            };
            let _ = this.update_in(cx, |this, _, cx| {
                if request.matches(this) {
                    this.add_files(paths, position, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn add_files(
        &mut self,
        paths: Vec<PathBuf>,
        position: Position,
        cx: &mut Context<Self>,
    ) {
        let request = FileRequest::new(self);
        let desktop = self.desktop.clone();
        let job = cx.background_executor().spawn(async move {
            paths
                .into_iter()
                .map(|path| {
                    let relative = desktop.resolve_dropped_file(&path)?;
                    match desktop.stat_vault_file(&relative)? {
                        VaultFileStatus::Ok {
                            stat,
                            mime_type,
                            name,
                        } => Ok(FileData {
                            path: relative,
                            name,
                            mime_type,
                            size: stat.size,
                            seen_mtime: stat.modified_epoch_ms,
                            seen_size: stat.size,
                        }),
                        _ => Err("File must exist inside the notes directory".into()),
                    }
                })
                .collect::<Vec<Result<FileData, String>>>()
        });
        cx.spawn(async move |this, cx| {
            let files = job.await;
            let _ = this.update(cx, |this, cx| {
                if !request.matches(this) {
                    return;
                }
                this.insert_file_nodes(files, position, cx);
                this.refresh_files(cx);
            });
        })
        .detach();
    }

    fn insert_file_nodes(
        &mut self,
        files: Vec<Result<FileData, String>>,
        position: Position,
        cx: &mut Context<Self>,
    ) {
        let (files, errors) = collect_results(files);
        self.change(
            |project| {
                for (index, file) in files.into_iter().enumerate() {
                    let node = GraphNode::file(uuid::Uuid::new_v4().to_string(), file, now_ms());
                    project.graph.add_node(
                        node,
                        Position {
                            x: position.x + index as f64 * 24.,
                            y: position.y + index as f64 * 24.,
                        },
                    );
                }
            },
            cx,
        );
        if !errors.is_empty() {
            self.notice = Some(errors.join("\n"));
        }
    }

    /// Every file is inspected before it can become generation context. The core
    /// preview checks decoded dimensions and caches at most 64 file versions.
    pub(crate) fn refresh_files(&mut self, cx: &mut Context<Self>) {
        let request = FileRequest::new(self);
        let files: Vec<_> = self
            .editor
            .project
            .graph
            .nodes
            .values()
            .filter_map(|n| n.file_data().map(|f| (n.id.clone(), f.path.clone())))
            .collect();
        self.file_status.clear();
        self.canvas.update(cx, |canvas, cx| {
            canvas.set_file_previews(
                Default::default(),
                Default::default(),
                Default::default(),
                cx,
            )
        });
        let desktop = self.desktop.clone();
        let focused = self.preview_id.clone();
        let job = cx.background_executor().spawn(async move {
            let mut previews = 0;
            files
                .into_iter()
                .map(|(id, path)| {
                    // Decode at most the previews the canvas keeps, plus the
                    // open file. Every other file still gets the prompt checks.
                    let full = previews < FILE_PREVIEW_LIMIT || focused.as_ref() == Some(&id);
                    let inspection = inspect_file(&desktop, id, path, full);
                    previews += usize::from(matches!(inspection.preview, Some(Ok(_))));
                    inspection
                })
                .collect::<Vec<_>>()
        });
        self.file_refresh = Some(cx.spawn(async move |this, cx| {
            let inspections = job.await;
            let _ = this.update(cx, |this, cx| {
                if !request.matches(this) {
                    return;
                }
                this.apply_file_inspections(inspections, cx);
                this.refresh(cx);
            });
        }));
        cx.notify();
    }

    fn apply_file_inspections(&mut self, inspections: Vec<FileInspection>, cx: &mut Context<Self>) {
        self.file_preview_errors.clear();
        self.file_previews.clear();
        for FileInspection {
            id,
            path,
            status,
            preview,
        } in inspections
        {
            if !same_file(self, &id, &path) {
                continue;
            }
            self.file_status.insert(id.clone(), status.clone());
            if self.preview_id.as_ref() == Some(&id) {
                self.file_preview = Some(
                    preview
                        .clone()
                        .unwrap_or_else(|| Err(status_message(&status))),
                );
            }
            match preview {
                Some(Err(error)) => {
                    self.file_preview_errors.insert(id, error);
                }
                Some(Ok(preview)) if self.file_previews.len() < FILE_PREVIEW_LIMIT => {
                    self.file_previews.insert(id, preview);
                }
                _ => {}
            }
        }
        self.canvas.update(cx, |canvas, cx| {
            canvas.set_file_previews(
                self.file_previews.clone(),
                self.file_status.clone(),
                self.file_preview_errors.clone(),
                cx,
            )
        });
    }

    pub(crate) fn load_file_preview(&mut self, id: String, cx: &mut Context<Self>) {
        self.file_preview = self.file_previews.get(&id).cloned().map(Ok);
        self.refresh_files(cx);
    }

    pub(crate) fn refresh_file(&mut self, id: String, cx: &mut Context<Self>) {
        let Some(file) = self
            .editor
            .project
            .graph
            .nodes
            .get(&id)
            .and_then(|n| n.file_data())
            .cloned()
        else {
            return;
        };
        let request = FileRequest::new(self);
        let desktop = self.desktop.clone();
        let path = file.path;
        let requested_path = path.clone();
        let job = cx
            .background_executor()
            .spawn(async move { desktop.stat_vault_file(&requested_path) });
        cx.spawn(async move |this, cx| {
            let result = job.await;
            let _ = this.update(cx, |this, cx| {
                if !request.matches(this) || !same_file(this, &id, &path) {
                    return;
                }
                this.adopt_file_version(id, result, cx);
                this.refresh_files(cx);
            });
        })
        .detach();
    }

    fn adopt_file_version(
        &mut self,
        id: String,
        result: Result<VaultFileStatus, String>,
        cx: &mut Context<Self>,
    ) {
        match result {
            Ok(VaultFileStatus::Ok {
                stat,
                mime_type,
                name,
            }) => self.change(
                |project| {
                    let Some(NodeKind::File(file)) =
                        project.graph.nodes.get_mut(&id).map(|node| &mut node.kind)
                    else {
                        return;
                    };
                    file.size = stat.size;
                    file.seen_size = stat.size;
                    file.seen_mtime = stat.modified_epoch_ms;
                    file.mime_type = mime_type;
                    file.name = name;
                },
                cx,
            ),
            Ok(_) => self.notice = Some("File missing or outside notes directory".into()),
            Err(error) => self.notice = Some(error),
        }
    }

    pub(crate) fn send_blocker(&self, id: &str) -> Option<String> {
        if self.editor.is_node_blocked(id) {
            return Some("A response is still generating on this branch".into());
        }
        for ancestor in self.editor.project.graph.conversation_path_ids(id) {
            let Some(file) = self
                .editor
                .project
                .graph
                .nodes
                .get(&ancestor)
                .and_then(|n| n.file_data())
            else {
                continue;
            };
            if let Some(reason) = file_blocker(
                file,
                self.file_status.get(&ancestor),
                self.file_preview_errors.get(&ancestor).map(String::as_str),
                self.desktop.attachment_limits().image_max_bytes,
            ) {
                return Some(reason);
            }
        }
        None
    }

    pub(crate) fn pick_images(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let request = FileRequest::new(self);
        let node = self.preview_id.clone();
        let picker = cx.prompt_for_paths(PathPromptOptions {
            files: true,
            directories: false,
            multiple: true,
            prompt: Some("Attach images".into()),
        });
        cx.spawn_in(window, async move |this, cx| {
            let Ok(Ok(Some(paths))) = picker.await else {
                return;
            };
            let _ = this.update_in(cx, |this, _, cx| {
                if request.matches(this) && this.preview_id == node {
                    this.attach_images(paths, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn attach_images(&mut self, paths: Vec<PathBuf>, cx: &mut Context<Self>) {
        let Some(id) = self.preview_id.clone() else {
            return;
        };
        if !self.editing {
            return;
        }
        self.attach_images_to(id, paths, cx);
    }

    pub(crate) fn attach_images_to(
        &mut self,
        id: String,
        paths: Vec<PathBuf>,
        cx: &mut Context<Self>,
    ) {
        if self.editor.is_node_blocked(&id)
            || !self
                .editor
                .project
                .graph
                .nodes
                .get(&id)
                .is_some_and(|node| matches!(node.kind, NodeKind::User(_)))
        {
            return;
        }
        let request = FileRequest::new(self);
        let job = cx.background_executor().spawn(async move {
            paths
                .into_iter()
                .map(|path| {
                    let mut data = Vec::new();
                    std::fs::File::open(&path)
                        .map_err(|e| e.to_string())?
                        .take(32 * 1024 * 1024 + 1)
                        .read_to_end(&mut data)
                        .map_err(|e| e.to_string())?;
                    prepare_image(
                        data,
                        path.file_name().map(|s| s.to_string_lossy().into_owned()),
                    )
                })
                .collect::<Vec<_>>()
        });
        cx.spawn(async move |this, cx| {
            let images = job.await;
            let _ = this.update(cx, |this, cx| {
                if request.matches(this) {
                    this.add_images_to_node(id, images, cx);
                }
            });
        })
        .detach();
    }

    pub(crate) fn paste_images(&mut self, cx: &mut Context<Self>) -> bool {
        let Some(id) = self.preview_id.clone() else {
            return false;
        };
        if !self.editing || self.editor.is_node_blocked(&id) {
            return false;
        }
        let request = FileRequest::new(self);
        let Some(clipboard) = cx.read_from_clipboard() else {
            return false;
        };
        let images: Vec<_> = clipboard
            .entries()
            .iter()
            .filter_map(|entry| match entry {
                ClipboardEntry::Image(image) => Some(image.bytes.clone()),
                _ => None,
            })
            .collect();
        if images.is_empty() {
            return false;
        }
        let job = cx.background_executor().spawn(async move {
            images
                .into_iter()
                .map(|bytes| prepare_image(bytes, Some("Pasted image.png".into())))
                .collect()
        });
        cx.spawn(async move |this, cx| {
            let images = job.await;
            let _ = this.update(cx, |this, cx| {
                if request.matches(this) {
                    this.add_images_to_node(id, images, cx);
                }
            });
        })
        .detach();
        true
    }

    fn add_images_to_node(
        &mut self,
        id: String,
        images: Vec<Result<ImageAttachment, String>>,
        cx: &mut Context<Self>,
    ) {
        if self.editor.is_node_blocked(&id) {
            return;
        }
        let (images, errors) = collect_results(images);
        self.change(
            |project| {
                let Some(NodeKind::User(data)) =
                    project.graph.nodes.get_mut(&id).map(|node| &mut node.kind)
                else {
                    return;
                };
                data.images.extend(images);
            },
            cx,
        );
        if !errors.is_empty() {
            self.notice = Some(errors.join("\n"));
        }
    }

    pub(crate) fn mention_open(&self) -> bool {
        self.mention_state.open
    }

    pub(crate) fn dismiss_mentions(&mut self) {
        self.mention_state.open = false;
        self.mention_state.loading = false;
        self.mention_state.error = None;
        self.mention_state.request = self.mention_state.request.wrapping_add(1);
        self.mention_state.task = None;
        self.mentions.clear();
    }

    pub(crate) fn update_mentions(&mut self, cx: &mut Context<Self>) {
        let text = self.input.read(cx).value().to_string();
        if self.mention_state.completed_value.as_ref() == Some(&text) {
            return;
        }
        self.mention_state.completed_value = None;
        let cursor = self.input.read(cx).cursor();
        let Some(query) = mention_query(&text, cursor) else {
            self.dismiss_mentions();
            return;
        };
        self.dismiss_mentions();
        self.mention_state.open = true;
        self.mention_state.loading = true;
        self.mention_index = 0;
        let search = self.mention_state.request;
        let request = FileRequest::new(self);
        let node = self.preview_id.clone();
        let desktop = self.desktop.clone();
        self.mention_state.task = Some(cx.spawn(async move |this, cx| {
            cx.background_executor()
                .timer(Duration::from_millis(100))
                .await;
            let result = cx
                .background_spawn(async move { desktop.search_files(&query, 15) })
                .await;
            let _ = this.update(cx, |this, cx| {
                if !request.matches(this)
                    || this.mention_state.request != search
                    || this.preview_id != node
                    || !this.editing
                    || this.input.read(cx).value().as_ref() != text
                    || this.input.read(cx).cursor() != cursor
                {
                    return;
                }
                this.finish_mention_search(result, cx);
            });
        }));
        cx.notify();
    }

    fn finish_mention_search(
        &mut self,
        result: Result<Vec<String>, String>,
        cx: &mut Context<Self>,
    ) {
        self.mention_state.loading = false;
        match result {
            Ok(files) => self.mentions = files,
            Err(error) => self.mention_state.error = Some(error),
        }
        self.mention_index = 0;
        self.mention_state.scroll.scroll_to_item(0);
        cx.notify();
    }

    pub(crate) fn accept_mention(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(path) = self.mentions.get(self.mention_index).cloned() else {
            return;
        };
        let text = self.input.read(cx).value().to_string();
        let cursor = self.input.read(cx).cursor().min(text.len());
        let Some(at) = text[..cursor].rfind('@') else {
            return;
        };
        let range = text[..at].encode_utf16().count()..text[..cursor].encode_utf16().count();
        self.input.update(cx, |input, cx| {
            input.replace_text_in_range(Some(range), &format!("@/{path}"), window, cx);
            input.focus(window, cx);
        });
        self.dismiss_mentions();
        self.mention_state.completed_value = Some(self.input.read(cx).value().to_string());
        // Programmatic edits emit an Input event on the next GPUI effect pass.
        cx.notify();
    }
}

fn mention_query(text: &str, cursor: usize) -> Option<String> {
    let before = text.get(..cursor.min(text.len()))?;
    let (left, query) = before.rsplit_once('@')?;
    if (!left.is_empty() && !left.chars().last().is_some_and(char::is_whitespace))
        || query.chars().any(char::is_whitespace)
    {
        return None;
    }
    Some(query.trim_start_matches('/').to_owned())
}

/// Thumbnails kept for the canvas, matching the core preview cache.
const FILE_PREVIEW_LIMIT: usize = 64;

fn inspect_file(desktop: &Desktop, id: String, path: String, full: bool) -> FileInspection {
    let status = desktop.stat_vault_file(&path);
    let preview = match &status {
        Ok(VaultFileStatus::Ok { .. }) if full => Some(desktop.read_vault_file_preview(&path)),
        Ok(VaultFileStatus::Ok { .. }) => desktop.check_vault_file(&path).err().map(Err),
        _ => None,
    };
    FileInspection {
        id,
        path,
        status,
        preview,
    }
}

struct FileInspection {
    id: String,
    path: String,
    status: Result<VaultFileStatus, String>,
    preview: Option<Result<thoughttree_core::vault::files::FilePreviewResponse, String>>,
}

fn collect_results<T>(results: Vec<Result<T, String>>) -> (Vec<T>, Vec<String>) {
    let mut values = Vec::new();
    let mut errors = Vec::new();
    for result in results {
        match result {
            Ok(value) => values.push(value),
            Err(error) => errors.push(error),
        }
    }
    (values, errors)
}

/// A picker or file read belongs to both its project and its Vault.
struct FileRequest {
    generation: u64,
    vault: Option<PathBuf>,
}

impl FileRequest {
    fn new(workspace: &Workspace) -> Self {
        Self {
            generation: workspace.generation,
            vault: workspace.desktop.config().notes_directory,
        }
    }

    fn matches(&self, workspace: &Workspace) -> bool {
        self.generation == workspace.generation
            && self.vault == workspace.desktop.config().notes_directory
    }
}

fn same_file(workspace: &Workspace, id: &str, path: &str) -> bool {
    workspace
        .editor
        .project
        .graph
        .nodes
        .get(id)
        .and_then(|node| node.file_data())
        .is_some_and(|file| file.path == path)
}

fn status_message(status: &Result<VaultFileStatus, String>) -> String {
    match status {
        Ok(VaultFileStatus::Missing) => "File missing — restore it or remove this node".into(),
        Ok(VaultFileStatus::Invalid) => "File is outside the notes directory".into(),
        Err(error) => error.clone(),
        _ => "File preview unavailable".into(),
    }
}

fn file_blocker(
    file: &FileData,
    status: Option<&Result<VaultFileStatus, String>>,
    preview_error: Option<&str>,
    image_limit: u64,
) -> Option<String> {
    let reason = match status {
        None => Some("Checking file".to_owned()),
        Some(Ok(VaultFileStatus::Missing)) => Some("File missing".to_owned()),
        Some(Ok(VaultFileStatus::Invalid)) => Some("File outside notes directory".to_owned()),
        Some(Err(error)) => Some(format!("File unavailable: {error}")),
        Some(Ok(VaultFileStatus::Ok {
            stat, mime_type, ..
        })) if is_raster_image(mime_type) && stat.size > image_limit => {
            Some("Image exceeds the attachment byte limit".to_owned())
        }
        Some(Ok(_)) => preview_error.map(|error| format!("File preview unavailable: {error}")),
    };
    reason.map(|reason| format!("{reason}: {}", file.name))
}

fn prepare_image(bytes: Vec<u8>, name: Option<String>) -> Result<ImageAttachment, String> {
    if bytes.len() > 32 * 1024 * 1024 {
        return Err("Image exceeds the 32 MB import limit".into());
    }
    let mut reader = image::ImageReader::new(Cursor::new(&bytes))
        .with_guessed_format()
        .map_err(|e| e.to_string())?;
    let mut limits = image::Limits::default();
    limits.max_image_width = Some(32000);
    limits.max_image_height = Some(32000);
    limits.max_alloc = Some(256 * 1024 * 1024);
    reader.limits(limits);
    let image = reader.decode().map_err(|e| e.to_string())?;
    let image = if image.width() > IMAGE_MAX_SIDE || image.height() > IMAGE_MAX_SIDE {
        image.resize(
            IMAGE_MAX_SIDE,
            IMAGE_MAX_SIDE,
            image::imageops::FilterType::Lanczos3,
        )
    } else {
        image
    };
    let mut data = Cursor::new(Vec::new());
    image
        .write_to(&mut data, image::ImageFormat::Png)
        .map_err(|e| e.to_string())?;
    let (data, mime_type) = if data.get_ref().len() as u64 <= IMAGE_MAX_BYTES {
        (data.into_inner(), "image/png")
    } else {
        let mut data = Vec::new();
        image::codecs::jpeg::JpegEncoder::new_with_quality(&mut data, 80)
            .encode_image(&image)
            .map_err(|e| e.to_string())?;
        if data.len() as u64 > IMAGE_MAX_BYTES {
            return Err("Image remains larger than the agent's 5 MB limit after resizing".into());
        }
        (data, "image/jpeg")
    };
    Ok(ImageAttachment {
        data: base64::engine::general_purpose::STANDARD.encode(data),
        mime_type: mime_type.into(),
        name,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[::core::prelude::v1::test]
    fn file_mentions_require_a_word_boundary_and_preserve_unicode_caret_offsets() {
        let text = "é @/notes/研究 suffix";
        let cursor = text.find(" suffix").unwrap();
        assert_eq!(mention_query(text, cursor), Some("notes/研究".into()));
        assert_eq!(mention_query(text, text.len()), None);
        assert_eq!(mention_query("mail@example.org", 16), None);
        assert_eq!(mention_query("@", 1), Some(String::new()));
        assert_eq!(mention_query("é @", 1), None);
    }
    #[::core::prelude::v1::test]
    fn malformed_attachment_is_rejected_before_entering_the_graph() {
        assert!(prepare_image(b"not an image".to_vec(), None).is_err());
    }
    #[::core::prelude::v1::test]
    fn image_attachment_resizes_proportionally_and_reports_the_format_of_its_actual_bytes() {
        let image = image::RgbImage::from_pixel(IMAGE_MAX_SIDE * 2, 16, image::Rgb([20, 80, 100]));
        let mut bytes = Cursor::new(Vec::new());
        image
            .write_to(&mut bytes, image::ImageFormat::Jpeg)
            .unwrap();
        let attachment = prepare_image(bytes.into_inner(), Some("test.jpg".into())).unwrap();
        let decoded = base64::engine::general_purpose::STANDARD
            .decode(attachment.data)
            .unwrap();
        assert_eq!(attachment.mime_type, "image/png");
        assert_eq!(
            image::guess_format(&decoded).unwrap(),
            image::ImageFormat::Png
        );
        let image = image::load_from_memory(&decoded).unwrap();
        assert_eq!((image.width(), image.height()), (IMAGE_MAX_SIDE, 8));
    }

    #[::core::prelude::v1::test]
    fn file_delivery_blocks_unknown_missing_and_invalid_preview_states() {
        let file = FileData {
            path: "chart.png".into(),
            name: "chart.png".into(),
            mime_type: "image/png".into(),
            size: 100,
            seen_mtime: 1,
            seen_size: 100,
        };
        let healthy = Ok(VaultFileStatus::Ok {
            stat: thoughttree_core::vault::files::FileStat {
                size: 100,
                modified_epoch_ms: 2,
            },
            mime_type: "image/png".into(),
            name: file.name.clone(),
        });
        assert!(file_blocker(&file, None, None, IMAGE_MAX_BYTES)
            .unwrap()
            .contains("chart.png"));
        assert!(file_blocker(
            &file,
            Some(&Ok(VaultFileStatus::Missing)),
            None,
            IMAGE_MAX_BYTES
        )
        .unwrap()
        .contains("missing"));
        assert!(file_blocker(
            &file,
            Some(&Ok(VaultFileStatus::Invalid)),
            None,
            IMAGE_MAX_BYTES
        )
        .unwrap()
        .contains("outside notes directory"));
        assert!(file_blocker(
            &file,
            Some(&Err("permission denied".into())),
            None,
            IMAGE_MAX_BYTES
        )
        .unwrap()
        .contains("permission denied"));
        assert!(file_blocker(
            &file,
            Some(&healthy),
            Some("too_large: decoded dimensions exceed limit"),
            IMAGE_MAX_BYTES
        )
        .is_some());
        // A modification time difference alone is not a reason to refuse a turn.
        assert_eq!(
            file_blocker(&file, Some(&healthy), None, IMAGE_MAX_BYTES),
            None
        );
    }

    #[::core::prelude::v1::test]
    fn image_byte_limit_does_not_reject_svg_disk_references() {
        let file = FileData {
            path: "diagram.svg".into(),
            name: "diagram.svg".into(),
            mime_type: "image/svg+xml".into(),
            size: IMAGE_MAX_BYTES + 1,
            seen_mtime: 1,
            seen_size: IMAGE_MAX_BYTES + 1,
        };
        let mut status = Ok(VaultFileStatus::Ok {
            stat: thoughttree_core::vault::files::FileStat {
                size: IMAGE_MAX_BYTES + 1,
                modified_epoch_ms: 1,
            },
            mime_type: "image/svg+xml".into(),
            name: file.name.clone(),
        });
        assert_eq!(
            file_blocker(&file, Some(&status), None, IMAGE_MAX_BYTES),
            None
        );
        if let Ok(VaultFileStatus::Ok { mime_type, .. }) = &mut status {
            *mime_type = "image/png".into();
        }
        assert!(file_blocker(&file, Some(&status), None, IMAGE_MAX_BYTES).is_some());
    }

    #[gpui::test]
    fn native_file_refresh_recovers_missing_files_and_reload_adopts_the_new_version(
        cx: &mut gpui::TestAppContext,
    ) {
        let (directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        let path = directory.path().join("vault/reference.md");
        std::fs::write(&path, "Original note").unwrap();
        workspace.update(cx, |this, cx| {
            this.add_files(vec![path.clone()], Position::default(), cx)
        });
        cx.run_until_parked();
        let id = workspace.read_with(cx, |this, _| {
            let node = this
                .editor
                .project
                .graph
                .nodes
                .values()
                .find(|node| node.file_data().is_some())
                .unwrap();
            let file = node.file_data().unwrap();
            assert_eq!(file.path, "reference.md");
            assert_eq!(file.name, "reference.md");
            assert_eq!(file.size, 13);
            assert_eq!(file.seen_size, 13);
            node.id.clone()
        });
        workspace.update(cx, |this, _| {
            this.editor.project.graph.add_edge(&id, "question").unwrap();
            this.editor.project.graph.add_node(
                GraphNode::user("independent", "An unrelated branch", 4.0),
                Position::default(),
            );
            assert_eq!(this.send_blocker("question"), None);
        });
        std::fs::remove_file(&path).unwrap();
        workspace.update(cx, |this, cx| {
            this.refresh_files(cx);
            assert!(this
                .send_blocker("question")
                .unwrap()
                .contains("Checking file"));
        });
        cx.run_until_parked();
        assert!(workspace.read_with(cx, |this, _| this
            .send_blocker("question")
            .unwrap()
            .contains("File missing: reference.md")));
        assert_eq!(
            workspace.read_with(cx, |this, _| this.send_blocker("independent")),
            None
        );
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        cx.deactivate_window();
        std::fs::write(&path, "Restored and changed note").unwrap();
        cx.update(|window, _| window.activate_window());
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert_eq!(this.send_blocker("question"), None);
            assert_eq!(this.editor.project.graph.nodes[&id].file_data().unwrap().seen_size, 13);
            assert!(matches!(this.file_previews.get(&id).map(|response| &response.preview), Some(thoughttree_core::vault::files::FilePreview::Text { excerpt, .. }) if excerpt == "Restored and changed note"));
        });
        workspace.update(cx, |this, cx| {
            let NodeKind::File(file) = &mut this.editor.project.graph.nodes[&id].kind else {
                unreachable!();
            };
            file.name = "stale imported name".into();
            file.mime_type = "application/octet-stream".into();
            this.editor.mark_saved(this.editor.edit_revision);
            this.refresh_file(id.clone(), cx);
        });
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            let file = this.editor.project.graph.nodes[&id].file_data().unwrap();
            assert_eq!(file.seen_size, 25);
            assert_eq!(file.size, 25);
            let Some(Ok(VaultFileStatus::Ok {
                stat,
                mime_type,
                name,
            })) = this.file_status.get(&id)
            else {
                panic!("restored file must have a current stat");
            };
            assert_eq!(file.seen_mtime, stat.modified_epoch_ms);
            assert_eq!(&file.mime_type, mime_type);
            assert_eq!(&file.name, name);
            assert!(this.editor.is_dirty());
        });
    }

    #[gpui::test]
    fn native_file_drop_fans_out_bounds_the_cache_and_blocks_oversized_ancestors(
        cx: &mut gpui::TestAppContext,
    ) {
        let (directory, workspace, cx, _events) = crate::interaction_tests::workspace(cx);
        let outside = directory.path().join("outside.md");
        std::fs::write(&outside, "Outside Vault").unwrap();
        workspace.update(cx, |this, cx| {
            this.add_files(vec![outside], Position::default(), cx)
        });
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert_eq!(this.editor.project.graph.nodes.len(), 3);
            assert!(
                this.notice
                    .as_ref()
                    .is_some_and(|notice| notice.contains("notes directory")),
                "notice={:?}",
                this.notice
            );
        });
        let mut paths = Vec::new();
        for index in 0..66 {
            let path = directory.path().join(format!("vault/note-{index:02}.md"));
            std::fs::write(&path, format!("File {index}")).unwrap();
            paths.push(path);
        }
        let oversized = directory.path().join("vault/wide.png");
        image::RgbImage::from_pixel(IMAGE_MAX_SIDE + 1, 2, image::Rgb([10, 30, 50]))
            .save(&oversized)
            .unwrap();
        paths.push(oversized);
        workspace.update(cx, |this, cx| {
            this.canvas.update(cx, |_, cx| {
                cx.emit(crate::canvas::GraphEvent::DropFiles(
                    paths,
                    Position { x: 20., y: 30. },
                ));
            });
        });
        cx.run_until_parked();
        let uncached = workspace.update(cx, |this, _| {
            assert_eq!(this.file_status.len(), 67);
            assert_eq!(this.file_previews.len(), 64);
            let files: Vec<_> = this
                .editor
                .project
                .graph
                .nodes
                .values()
                .filter(|node| node.file_data().is_some())
                .collect();
            for (index, node) in files.iter().enumerate() {
                assert_eq!(
                    this.editor.project.graph.layout[&node.id],
                    Position {
                        x: 20. + index as f64 * 24.,
                        y: 30. + index as f64 * 24.
                    }
                );
            }
            let wide = files
                .iter()
                .find(|node| node.file_data().unwrap().path == "wide.png")
                .unwrap()
                .id
                .clone();
            let uncached = files
                .iter()
                .find(|node| node.file_data().unwrap().path == "note-65.md")
                .unwrap()
                .id
                .clone();
            assert!(!this.file_previews.contains_key(&uncached));
            assert!(this.file_preview_errors.contains_key(&wide));
            assert!(
                this.editor.project.graph.nodes[&wide]
                    .file_data()
                    .unwrap()
                    .size
                    < IMAGE_MAX_BYTES
            );
            this.editor
                .project
                .graph
                .add_edge(&wide, "question")
                .unwrap();
            assert!(this.send_blocker("question").unwrap().contains("wide.png"));
            uncached
        });
        cx.update(|window, cx| {
            workspace.update(cx, |this, cx| this.preview(uncached, false, window, cx))
        });
        cx.run_until_parked();
        workspace.read_with(cx, |this, _| {
            assert_eq!(this.file_previews.len(), 64);
            assert!(matches!(this.file_preview.as_ref().and_then(|preview| preview.as_ref().ok()).map(|response| &response.preview), Some(thoughttree_core::vault::files::FilePreview::Text { excerpt, .. }) if excerpt == "File 65"));
        });
    }
}
