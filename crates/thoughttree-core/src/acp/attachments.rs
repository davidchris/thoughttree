//! Prompt attachments: how inline images and Vault files reach the agent.
//!
//! [`build_prompt_blocks`] is the single mapping from conversation messages to
//! ACP content blocks. Inline images (`Message.images`) and File nodes
//! (`Message.files`) both go through it, so limits and validation live in one
//! place and every error surfaces before the first ACP call.
//!
//! Two delivery modes, chosen by mime (`vault::files::is_raster_image`):
//!
//! - **Raster images** (png, jpeg, gif, webp): the bytes are read fresh from
//!   disk at send time via the Vault resolver and shipped as
//!   `ContentBlock::Image`. Models cannot fetch images themselves, so they
//!   must be inlined; the image limits from `vault::files::limits` apply.
//! - **Everything else** (text, code, data, unknown types): a pointer, not
//!   content. The prompt carries `ContentBlock::ResourceLink` with a `file://`
//!   URI to the canonical path, plus a one-line mention of the Vault-relative
//!   path in the text block for adapters that ignore resource links. Pointers
//!   work because the ACP session's cwd is the Vault (see
//!   `sessions::run_prompt_session`): the agent reads the file with its own
//!   tools, through the normal permission flow, and always sees the live
//!   content. Pointers have no size limit.
//!
//! The frontend-supplied mime is only a hint. A file that claims to be an
//! image but does not sniff as one is demoted to a pointer, so a renamed
//! non-image is never sent as an image block.

use std::path::Path;

use agent_client_protocol::schema::v1::{ContentBlock, ImageContent, ResourceLink, TextContent};
use base64::Engine;

use crate::types::{Message, MessageFile};
use crate::vault::files::limits::MAX_IMAGES_PER_PROMPT;
use crate::vault::files::{
    is_raster_image, mime_for_file, read_image_for_prompt, resolve_vault_file, VaultFileError,
};

/// Text sent in place of user content when a turn carries only attachments,
/// so adapters never receive a block-only prompt.
pub const FILE_ONLY_PLACEHOLDER: &str = "User attached a file without additional text. Respond using the conversation context and the attached file.";

#[derive(Debug, thiserror::Error)]
pub enum AttachmentError {
    #[error("attached file not found: {path}")]
    MissingFile { path: String },
    #[error("attached file path is invalid: {path}")]
    InvalidPath { path: String },
    #[error("attached image {path} exceeds the limit ({limit})")]
    ImageTooLarge { path: String, limit: u64 },
    #[error("too many images in one prompt (max {max})")]
    TooManyImages { max: usize },
    #[error("Cannot send empty prompt")]
    EmptyPrompt,
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

pub fn build_prompt_blocks(
    vault_root: &Path,
    messages: &[Message],
    date_prefix: &str,
) -> Result<Vec<ContentBlock>, AttachmentError> {
    let mut inline_images: Vec<ContentBlock> = Vec::new();
    let mut file_images: Vec<ContentBlock> = Vec::new();
    let mut links: Vec<ContentBlock> = Vec::new();
    let mut segments: Vec<String> = Vec::with_capacity(messages.len());

    for msg in messages {
        let inline = msg.images.as_deref().unwrap_or_default();
        let files = msg.files.as_deref().unwrap_or_default();
        inline_images.extend(inline.iter().map(|img| {
            ContentBlock::Image(ImageContent::new(img.data.clone(), img.mime_type.clone()))
        }));

        let content = if msg.content.trim().is_empty() && !(inline.is_empty() && files.is_empty()) {
            FILE_ONLY_PLACEHOLDER
        } else {
            msg.content.as_str()
        };
        let mut segment = format!("{}: {content}", msg.role);
        for file in files {
            match resolve_attachment(vault_root, file)? {
                Delivery::Image(block) => {
                    file_images.push(block);
                    segment.push_str(&format!("\n[Attached image: {}]", file.path));
                }
                Delivery::Pointer { link, mime_type } => {
                    links.push(ContentBlock::ResourceLink(link));
                    segment.push_str(&format!(
                        "\n[Attached file: {} ({mime_type}, {}) — read it from disk]",
                        file.path,
                        human_size(file.size)
                    ));
                }
            }
        }
        segments.push(segment);
    }

    if inline_images.len() + file_images.len() > MAX_IMAGES_PER_PROMPT {
        return Err(AttachmentError::TooManyImages {
            max: MAX_IMAGES_PER_PROMPT,
        });
    }

    let body = segments.join("\n\n");
    let mut blocks = inline_images;
    blocks.append(&mut file_images);
    blocks.append(&mut links);
    if body.trim().is_empty() && blocks.is_empty() {
        return Err(AttachmentError::EmptyPrompt);
    }
    if !body.trim().is_empty() {
        blocks.push(ContentBlock::Text(TextContent::new(format!(
            "{date_prefix}{body}"
        ))));
    }
    Ok(blocks)
}

enum Delivery {
    Image(ContentBlock),
    Pointer {
        link: ResourceLink,
        mime_type: String,
    },
}

/// Resolves `file` through the Vault boundary and decides its delivery mode.
/// The frontend mime is only a hint: a file that claims to be an image but
/// does not sniff as one is delivered as a pointer under the sniffed mime.
fn resolve_attachment(vault_root: &Path, file: &MessageFile) -> Result<Delivery, AttachmentError> {
    let path = resolve_vault_file(vault_root, &file.path).map_err(|e| map_error(e, file))?;
    let mime_type = if is_raster_image(&file.mime_type) {
        let sniffed = mime_for_file(&path);
        if sniffed == file.mime_type {
            let bytes = read_image_for_prompt(&path).map_err(|e| map_error(e, file))?;
            let data = base64::engine::general_purpose::STANDARD.encode(bytes);
            return Ok(Delivery::Image(ContentBlock::Image(ImageContent::new(
                data, sniffed,
            ))));
        }
        sniffed
    } else {
        file.mime_type.clone()
    };
    let link = ResourceLink::new(file.name.clone(), format!("file://{}", path.display()))
        .mime_type(mime_type.clone())
        .size(i64::try_from(file.size).ok());
    Ok(Delivery::Pointer { link, mime_type })
}

/// Binary-prefixed size for the text mention, e.g. `8 B`, `1.5 KB`, `2.0 MB`.
fn human_size(bytes: u64) -> String {
    const UNITS: [&str; 4] = ["KB", "MB", "GB", "TB"];
    if bytes < 1024 {
        return format!("{bytes} B");
    }
    let mut value = bytes as f64 / 1024.0;
    let mut unit = UNITS[0];
    for next in &UNITS[1..] {
        if value < 1024.0 {
            break;
        }
        value /= 1024.0;
        unit = next;
    }
    format!("{value:.1} {unit}")
}

fn map_error(err: VaultFileError, file: &MessageFile) -> AttachmentError {
    let path = file.path.clone();
    match err {
        VaultFileError::NotFound => AttachmentError::MissingFile { path },
        VaultFileError::InvalidPath | VaultFileError::NotAFile => {
            AttachmentError::InvalidPath { path }
        }
        VaultFileError::TooLarge { limit } => AttachmentError::ImageTooLarge { path, limit },
        VaultFileError::Io(io) => AttachmentError::Io(io),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use agent_client_protocol::schema::v1::{
        ContentBlock, ImageContent, ResourceLink, TextContent,
    };
    use base64::Engine;
    use tempfile::tempdir;

    use super::{build_prompt_blocks, human_size, AttachmentError, FILE_ONLY_PLACEHOLDER};
    use crate::types::{Message, MessageFile, MessageImage};
    use crate::vault::files::limits::{IMAGE_MAX_SIDE, MAX_IMAGES_PER_PROMPT};

    fn text_message(role: &str, content: &str) -> Message {
        Message {
            role: role.to_string(),
            content: content.to_string(),
            images: None,
            files: None,
        }
    }

    fn inline_image(mime: &str, data: &str) -> MessageImage {
        MessageImage {
            data: data.to_string(),
            mime_type: mime.to_string(),
        }
    }

    #[test]
    fn inline_images_become_image_blocks_ahead_of_the_text() {
        let vault = tempdir().unwrap();
        let mut first = text_message("user", "look");
        first.images = Some(vec![inline_image("image/png", "AAAA")]);
        let mut second = text_message("user", "and this");
        second.images = Some(vec![inline_image("image/jpeg", "BBBB")]);

        let blocks = build_prompt_blocks(vault.path(), &[first, second], "").unwrap();

        assert_eq!(
            blocks,
            vec![
                ContentBlock::Image(ImageContent::new("AAAA", "image/png")),
                ContentBlock::Image(ImageContent::new("BBBB", "image/jpeg")),
                ContentBlock::Text(TextContent::new("user: look\n\nuser: and this")),
            ]
        );
    }

    #[test]
    fn a_message_with_only_an_inline_image_gets_the_placeholder_text() {
        let vault = tempdir().unwrap();
        let mut only_image = text_message("user", "   ");
        only_image.images = Some(vec![inline_image("image/png", "AAAA")]);

        let blocks = build_prompt_blocks(vault.path(), &[only_image], "").unwrap();

        assert_eq!(
            blocks[1],
            ContentBlock::Text(TextContent::new(format!("user: {FILE_ONLY_PLACEHOLDER}")))
        );
    }

    #[test]
    fn no_messages_is_an_empty_prompt() {
        let vault = tempdir().unwrap();

        let err = build_prompt_blocks(vault.path(), &[], "Current date: X\n\n").unwrap_err();

        assert!(matches!(err, AttachmentError::EmptyPrompt), "{err:?}");
        assert_eq!(err.to_string(), "Cannot send empty prompt");
    }

    fn write_png(path: &std::path::Path, width: u32, height: u32) {
        image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]))
            .save_with_format(path, image::ImageFormat::Png)
            .unwrap();
    }

    fn file_ref(path: &str, mime: &str, size: u64) -> MessageFile {
        MessageFile {
            path: path.to_string(),
            name: path.rsplit('/').next().unwrap().to_string(),
            mime_type: mime.to_string(),
            size,
        }
    }

    fn file_message(content: &str, files: Vec<MessageFile>) -> Message {
        let mut message = text_message("user", content);
        message.files = Some(files);
        message
    }

    #[test]
    fn an_image_file_is_read_fresh_into_an_image_block_with_a_mention_line() {
        let vault = tempdir().unwrap();
        fs::create_dir_all(vault.path().join("img")).unwrap();
        write_png(&vault.path().join("img/shot.png"), 3, 2);
        let bytes = fs::read(vault.path().join("img/shot.png")).unwrap();
        let expected_data = base64::engine::general_purpose::STANDARD.encode(&bytes);
        let messages = [file_message(
            "what is this?",
            vec![file_ref("img/shot.png", "image/png", bytes.len() as u64)],
        )];

        let blocks = build_prompt_blocks(vault.path(), &messages, "").unwrap();

        assert_eq!(
            blocks,
            vec![
                ContentBlock::Image(ImageContent::new(expected_data, "image/png")),
                ContentBlock::Text(TextContent::new(
                    "user: what is this?\n[Attached image: img/shot.png]"
                )),
            ]
        );
    }

    #[test]
    fn a_text_file_becomes_a_resource_link_pointer_with_a_mention_line() {
        let vault = tempdir().unwrap();
        fs::create_dir_all(vault.path().join("notes")).unwrap();
        let content = "x".repeat(1536);
        fs::write(vault.path().join("notes/plan.md"), &content).unwrap();
        let canonical = fs::canonicalize(vault.path().join("notes/plan.md")).unwrap();
        let messages = [file_message(
            "summarize",
            vec![file_ref("notes/plan.md", "text/markdown", 1536)],
        )];

        let blocks = build_prompt_blocks(vault.path(), &messages, "").unwrap();

        assert_eq!(
            blocks,
            vec![
                ContentBlock::ResourceLink(
                    ResourceLink::new("plan.md", format!("file://{}", canonical.display()))
                        .mime_type("text/markdown".to_string())
                        .size(1536)
                ),
                ContentBlock::Text(TextContent::new(
                    "user: summarize\n[Attached file: notes/plan.md (text/markdown, 1.5 KB) — read it from disk]"
                )),
            ]
        );
    }

    #[test]
    fn a_file_only_message_uses_the_placeholder_as_its_content() {
        let vault = tempdir().unwrap();
        fs::write(vault.path().join("data.csv"), "a,b\n1,2\n").unwrap();
        let messages = [file_message("", vec![file_ref("data.csv", "text/csv", 8)])];

        let blocks = build_prompt_blocks(vault.path(), &messages, "D\n\n").unwrap();

        assert_eq!(
            blocks[1],
            ContentBlock::Text(TextContent::new(format!(
                "D\n\nuser: {FILE_ONLY_PLACEHOLDER}\n[Attached file: data.csv (text/csv, 8 B) — read it from disk]"
            )))
        );
    }

    #[test]
    fn a_missing_file_is_refused_before_any_send() {
        let vault = tempdir().unwrap();
        let messages = [file_message(
            "x",
            vec![file_ref("gone.md", "text/markdown", 1)],
        )];

        let err = build_prompt_blocks(vault.path(), &messages, "").unwrap_err();

        assert!(
            matches!(err, AttachmentError::MissingFile { ref path } if path == "gone.md"),
            "{err:?}"
        );
    }

    #[test]
    fn a_traversal_path_is_refused_as_invalid() {
        let vault = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "x").unwrap();
        let traversal = format!(
            "../{}/secret.txt",
            outside.path().file_name().unwrap().to_str().unwrap()
        );
        let messages = [file_message(
            "x",
            vec![file_ref(&traversal, "text/plain", 1)],
        )];

        let err = build_prompt_blocks(vault.path(), &messages, "").unwrap_err();

        assert!(
            matches!(err, AttachmentError::InvalidPath { ref path } if *path == traversal),
            "{err:?}"
        );
    }

    #[test]
    fn an_oversized_image_file_is_refused_with_the_limit() {
        let vault = tempdir().unwrap();
        write_png(&vault.path().join("tall.png"), 1, IMAGE_MAX_SIDE + 1);
        let messages = [file_message(
            "x",
            vec![file_ref("tall.png", "image/png", 100)],
        )];

        let err = build_prompt_blocks(vault.path(), &messages, "").unwrap_err();

        assert!(
            matches!(
                err,
                AttachmentError::ImageTooLarge { ref path, limit }
                    if path == "tall.png" && limit == u64::from(IMAGE_MAX_SIDE)
            ),
            "{err:?}"
        );
    }

    #[test]
    fn inline_and_file_images_count_together_against_the_prompt_limit() {
        let vault = tempdir().unwrap();
        write_png(&vault.path().join("one.png"), 2, 2);
        let mut message = file_message("x", vec![file_ref("one.png", "image/png", 100)]);
        message.images = Some(
            (0..MAX_IMAGES_PER_PROMPT)
                .map(|_| inline_image("image/png", "AAAA"))
                .collect(),
        );

        let err = build_prompt_blocks(vault.path(), &[message], "").unwrap_err();

        assert!(
            matches!(err, AttachmentError::TooManyImages { max } if max == MAX_IMAGES_PER_PROMPT),
            "{err:?}"
        );
    }

    #[test]
    fn exactly_the_image_limit_is_accepted() {
        let vault = tempdir().unwrap();
        let mut message = text_message("user", "x");
        message.images = Some(
            (0..MAX_IMAGES_PER_PROMPT)
                .map(|_| inline_image("image/png", "AAAA"))
                .collect(),
        );

        let blocks = build_prompt_blocks(vault.path(), &[message], "").unwrap();

        assert_eq!(blocks.len(), MAX_IMAGES_PER_PROMPT + 1);
    }

    #[test]
    fn a_file_claiming_to_be_an_image_that_does_not_sniff_as_one_is_a_pointer() {
        let vault = tempdir().unwrap();
        fs::write(vault.path().join("fake.png"), "not a png").unwrap();
        let canonical = fs::canonicalize(vault.path().join("fake.png")).unwrap();
        let messages = [file_message(
            "x",
            vec![file_ref("fake.png", "image/png", 9)],
        )];

        let blocks = build_prompt_blocks(vault.path(), &messages, "").unwrap();

        assert_eq!(
            blocks,
            vec![
                ContentBlock::ResourceLink(
                    ResourceLink::new("fake.png", format!("file://{}", canonical.display()))
                        .mime_type("application/octet-stream".to_string())
                        .size(9)
                ),
                ContentBlock::Text(TextContent::new(
                    "user: x\n[Attached file: fake.png (application/octet-stream, 9 B) — read it from disk]"
                )),
            ]
        );
    }

    #[test]
    fn blocks_are_ordered_inline_images_then_image_files_then_links_then_text() {
        let vault = tempdir().unwrap();
        write_png(&vault.path().join("a.png"), 2, 2);
        write_png(&vault.path().join("b.png"), 3, 3);
        fs::write(vault.path().join("a.md"), "a").unwrap();
        fs::write(vault.path().join("b.txt"), "b").unwrap();
        let a_png = base64::engine::general_purpose::STANDARD
            .encode(fs::read(vault.path().join("a.png")).unwrap());
        let b_png = base64::engine::general_purpose::STANDARD
            .encode(fs::read(vault.path().join("b.png")).unwrap());

        let mut first = file_message(
            "first",
            vec![
                file_ref("a.md", "text/markdown", 1),
                file_ref("a.png", "image/png", 1),
            ],
        );
        first.images = Some(vec![inline_image("image/png", "INLINE1")]);
        let mut second = file_message(
            "second",
            vec![
                file_ref("b.png", "image/png", 1),
                file_ref("b.txt", "text/plain", 1),
            ],
        );
        second.images = Some(vec![inline_image("image/jpeg", "INLINE2")]);

        let blocks = build_prompt_blocks(vault.path(), &[first, second], "D\n\n").unwrap();

        let kinds: Vec<&str> = blocks
            .iter()
            .map(|block| match block {
                ContentBlock::Image(img) => img.data.as_str(),
                ContentBlock::ResourceLink(link) => link.name.as_str(),
                ContentBlock::Text(_) => "text",
                _ => "other",
            })
            .collect();
        assert_eq!(
            kinds,
            vec![
                "INLINE1",
                "INLINE2",
                a_png.as_str(),
                b_png.as_str(),
                "a.md",
                "b.txt",
                "text"
            ]
        );
        assert_eq!(
            blocks[6],
            ContentBlock::Text(TextContent::new(
                "D\n\nuser: first\n[Attached file: a.md (text/markdown, 1 B) — read it from disk]\n[Attached image: a.png]\n\nuser: second\n[Attached image: b.png]\n[Attached file: b.txt (text/plain, 1 B) — read it from disk]"
            ))
        );
    }

    #[test]
    fn human_sizes_use_binary_units_with_one_decimal() {
        assert_eq!(human_size(0), "0 B");
        assert_eq!(human_size(1023), "1023 B");
        assert_eq!(human_size(1536), "1.5 KB");
        assert_eq!(human_size(5 * 1024 * 1024), "5.0 MB");
        assert_eq!(human_size(3 * 1024 * 1024 * 1024), "3.0 GB");
    }

    #[test]
    fn without_attachments_the_prompt_is_the_legacy_single_text_block() {
        let vault = tempdir().unwrap();
        let messages = [
            text_message("user", "hello"),
            text_message("assistant", "hi"),
            text_message("user", "how are you?"),
        ];

        let blocks = build_prompt_blocks(vault.path(), &messages, "Current date: X\n\n").unwrap();

        assert_eq!(
            blocks,
            vec![ContentBlock::Text(TextContent::new(
                "Current date: X\n\nuser: hello\n\nassistant: hi\n\nuser: how are you?"
            ))]
        );
    }
}
