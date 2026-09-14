//! Vault file references for File nodes (epic thoughttree-zzr).
//!
//! Every filesystem access for a File node goes through [`resolve_vault_file`]:
//! the path is Vault-relative, canonicalized, and rejected when it escapes the
//! Vault (`..`, absolute input, symlinks that resolve outside). Bytes never
//! cross this module unbounded: [`read_bounded`] and the preview functions cap
//! what they read.

use std::collections::HashMap;
use std::fs;
use std::io::{Cursor, Read};
use std::path::{Path, PathBuf};
use std::sync::{Mutex, MutexGuard};
use std::time::UNIX_EPOCH;

use super::{validate_relative_path, VaultError};

/// Attachment limits live here only; the frontend reads them via
/// [`limits::attachment_limits`] instead of duplicating the numbers.
pub mod limits {
    /// Image bytes (Anthropic API limit).
    pub const IMAGE_MAX_BYTES: u64 = 5 * 1024 * 1024;
    /// Longest image side in pixels (Anthropic API limit).
    pub const IMAGE_MAX_SIDE: u32 = 8000;
    pub const MAX_IMAGES_PER_PROMPT: usize = 20;
    /// Bytes of a text-like file shown in a File node preview.
    pub const PREVIEW_TEXT_BYTES: usize = 16 * 1024;
    /// Longest side of an image thumbnail in a File node preview.
    pub const PREVIEW_IMAGE_MAX_SIDE: u32 = 512;

    #[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
    pub struct AttachmentLimits {
        pub image_max_bytes: u64,
        pub image_max_side: u32,
        pub max_images_per_prompt: usize,
        pub preview_text_bytes: usize,
    }

    pub fn attachment_limits() -> AttachmentLimits {
        AttachmentLimits {
            image_max_bytes: IMAGE_MAX_BYTES,
            image_max_side: IMAGE_MAX_SIDE,
            max_images_per_prompt: MAX_IMAGES_PER_PROMPT,
            preview_text_bytes: PREVIEW_TEXT_BYTES,
        }
    }
}

pub use limits::AttachmentLimits;

#[derive(Debug, thiserror::Error)]
pub enum VaultFileError {
    #[error("path escapes vault root")]
    InvalidPath,
    #[error("file not found")]
    NotFound,
    #[error("path is not a regular file")]
    NotAFile,
    #[error("file exceeds the attachment limit ({limit})")]
    TooLarge { limit: u64 },
    #[error(transparent)]
    Io(#[from] std::io::Error),
}

impl From<VaultError> for VaultFileError {
    fn from(err: VaultError) -> Self {
        match err {
            VaultError::NotFound => Self::NotFound,
            VaultError::InvalidPath => Self::InvalidPath,
            VaultError::Io(io) => Self::Io(io),
            VaultError::Stale { .. } => Self::Io(std::io::Error::other(err.to_string())),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FileStat {
    pub size: u64,
    pub modified_epoch_ms: u64,
}

/// Canonical absolute path of an existing regular file inside the Vault.
/// Rejects `..`, absolute input, symlinks that resolve outside the Vault, and
/// directories.
pub fn resolve_vault_file(root: &Path, relative: &str) -> Result<PathBuf, VaultFileError> {
    let path = validate_relative_path(root, relative)?;
    let metadata = fs::metadata(&path).map_err(map_not_found)?;
    if !metadata.is_file() {
        return Err(VaultFileError::NotAFile);
    }
    Ok(path)
}

/// Vault-relative path (forward slashes) for an absolute path inside the
/// Vault. Both sides are canonicalized, so symlinks cannot fake membership.
pub fn relativize_vault_path(root: &Path, absolute: &Path) -> Result<String, VaultFileError> {
    if !absolute.is_absolute() {
        return Err(VaultFileError::InvalidPath);
    }
    let canonical_root = fs::canonicalize(root).map_err(map_not_found)?;
    let canonical = fs::canonicalize(absolute).map_err(map_not_found)?;
    let relative = canonical
        .strip_prefix(&canonical_root)
        .map_err(|_| VaultFileError::InvalidPath)?;
    if relative.as_os_str().is_empty() {
        return Err(VaultFileError::InvalidPath);
    }
    let relative = relative.to_str().ok_or(VaultFileError::InvalidPath)?;
    Ok(relative.replace('\\', "/"))
}

pub fn stat_vault_file(root: &Path, relative: &str) -> Result<FileStat, VaultFileError> {
    stat_path(&resolve_vault_file(root, relative)?)
}

fn stat_path(path: &Path) -> Result<FileStat, VaultFileError> {
    let metadata = fs::metadata(path).map_err(map_not_found)?;
    let modified_epoch_ms = metadata
        .modified()
        .ok()
        .and_then(|time| time.duration_since(UNIX_EPOCH).ok())
        .map(|duration| duration.as_millis() as u64)
        .unwrap_or(0);
    Ok(FileStat {
        size: metadata.len(),
        modified_epoch_ms,
    })
}

/// Reads at most `limit` bytes; `TooLarge` if the file is bigger. The size is
/// checked before and after reading so a file growing underneath still stops
/// at the limit.
pub fn read_bounded(path: &Path, limit: u64) -> Result<Vec<u8>, VaultFileError> {
    let file = fs::File::open(path).map_err(map_not_found)?;
    let size = file.metadata()?.len();
    if size > limit {
        return Err(VaultFileError::TooLarge { limit });
    }
    let mut bytes = Vec::with_capacity(size as usize);
    file.take(limit + 1).read_to_end(&mut bytes)?;
    if bytes.len() as u64 > limit {
        return Err(VaultFileError::TooLarge { limit });
    }
    Ok(bytes)
}

const OCTET_STREAM: &str = "application/octet-stream";

fn mime_for_extension(extension: &str) -> &'static str {
    match extension {
        "md" | "markdown" => "text/markdown",
        "txt" | "text" | "log" => "text/plain",
        "rs" => "text/x-rust",
        "ts" | "tsx" | "mts" => "text/typescript",
        "js" | "jsx" | "mjs" | "cjs" => "text/javascript",
        "json" | "jsonc" => "application/json",
        "yaml" | "yml" => "application/yaml",
        "toml" => "application/toml",
        "csv" => "text/csv",
        "tsv" => "text/tab-separated-values",
        "py" => "text/x-python",
        "rb" => "text/x-ruby",
        "go" => "text/x-go",
        "java" | "kt" | "kts" => "text/x-java",
        "c" | "h" => "text/x-c",
        "cpp" | "cc" | "cxx" | "hpp" | "hh" => "text/x-c++",
        "sh" | "bash" | "zsh" | "fish" => "text/x-shellscript",
        "sql" => "application/sql",
        "html" | "htm" => "text/html",
        "css" | "scss" | "less" => "text/css",
        "xml" => "application/xml",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "pdf" => "application/pdf",
        _ => OCTET_STREAM,
    }
}

/// Mime from the extension table, `application/octet-stream` when unknown.
/// Raster image extensions are magic-sniffed; a mismatch is octet-stream so a
/// renamed non-image is never sent as an image block.
pub fn mime_for_file(path: &Path) -> String {
    let extension = path
        .extension()
        .and_then(|ext| ext.to_str())
        .map(|ext| ext.to_ascii_lowercase())
        .unwrap_or_default();
    let mime = mime_for_extension(&extension);
    if is_raster_image(mime) && sniff_image_mime(path) != Some(mime) {
        return OCTET_STREAM.to_string();
    }
    mime.to_string()
}

fn sniff_image_mime(path: &Path) -> Option<&'static str> {
    let mut head = [0u8; 12];
    let mut file = fs::File::open(path).ok()?;
    let mut read = 0;
    while read < head.len() {
        match file.read(&mut head[read..]) {
            Ok(0) => break,
            Ok(n) => read += n,
            Err(_) => return None,
        }
    }
    let head = &head[..read];
    if head.starts_with(b"\x89PNG\r\n\x1a\n") {
        Some("image/png")
    } else if head.starts_with(&[0xFF, 0xD8, 0xFF]) {
        Some("image/jpeg")
    } else if head.starts_with(b"GIF87a") || head.starts_with(b"GIF89a") {
        Some("image/gif")
    } else if head.len() >= 12 && &head[0..4] == b"RIFF" && &head[8..12] == b"WEBP" {
        Some("image/webp")
    } else {
        None
    }
}

/// Only these become `ContentBlock::Image`; everything else is a pointer.
pub fn is_raster_image(mime: &str) -> bool {
    matches!(
        mime,
        "image/png" | "image/jpeg" | "image/gif" | "image/webp"
    )
}

/// Image bytes for a prompt, after checking the byte limit (stat, no read)
/// and the longest-side limit (header parse, no decode).
pub fn read_image_for_prompt(path: &Path) -> Result<Vec<u8>, VaultFileError> {
    let size = fs::metadata(path).map_err(map_not_found)?.len();
    if size > limits::IMAGE_MAX_BYTES {
        return Err(VaultFileError::TooLarge {
            limit: limits::IMAGE_MAX_BYTES,
        });
    }
    let dimensions = imagesize::size(path).map_err(image_header_error)?;
    if dimensions.width > limits::IMAGE_MAX_SIDE as usize
        || dimensions.height > limits::IMAGE_MAX_SIDE as usize
    {
        return Err(VaultFileError::TooLarge {
            limit: u64::from(limits::IMAGE_MAX_SIDE),
        });
    }
    read_bounded(path, limits::IMAGE_MAX_BYTES)
}

fn image_header_error(err: imagesize::ImageError) -> VaultFileError {
    match err {
        imagesize::ImageError::IoError(io) => map_not_found(io),
        other => VaultFileError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unreadable image header: {other}"),
        )),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FileInfo {
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub modified_epoch_ms: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum FilePreview {
    Image {
        /// Base64 of the thumbnail, never of the original bytes.
        data: String,
        mime_type: String,
        width: u32,
        height: u32,
    },
    Text {
        excerpt: String,
        truncated: bool,
    },
    None,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
pub struct FilePreviewResponse {
    pub info: FileInfo,
    pub preview: FilePreview,
}

/// Bounded in-memory preview cache keyed by (relative path, mtime, size).
/// Cleared wholesale when it grows past [`PREVIEW_CACHE_MAX_ENTRIES`].
#[derive(Default)]
pub struct PreviewCache {
    entries: Mutex<HashMap<PreviewKey, FilePreviewResponse>>,
}

const PREVIEW_CACHE_MAX_ENTRIES: usize = 64;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
struct PreviewKey {
    relative: String,
    modified_epoch_ms: u64,
    size: u64,
}

impl PreviewCache {
    pub fn preview(
        &self,
        root: &Path,
        relative: &str,
    ) -> Result<FilePreviewResponse, VaultFileError> {
        let path = resolve_vault_file(root, relative)?;
        let stat = stat_path(&path)?;
        let key = PreviewKey {
            relative: relative.to_string(),
            modified_epoch_ms: stat.modified_epoch_ms,
            size: stat.size,
        };
        if let Some(hit) = self.lock().get(&key) {
            return Ok(hit.clone());
        }

        let response = build_preview(&path, &stat)?;
        let mut entries = self.lock();
        if entries.len() >= PREVIEW_CACHE_MAX_ENTRIES {
            entries.clear();
        }
        entries.insert(key, response.clone());
        Ok(response)
    }

    fn lock(&self) -> MutexGuard<'_, HashMap<PreviewKey, FilePreviewResponse>> {
        self.entries
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
    }
}

fn build_preview(path: &Path, stat: &FileStat) -> Result<FilePreviewResponse, VaultFileError> {
    let mime_type = mime_for_file(path);
    let preview = if is_raster_image(&mime_type) {
        image_preview(path, &mime_type)?
    } else if is_text_like(&mime_type) {
        text_preview(path, stat.size)?
    } else {
        FilePreview::None
    };
    Ok(FilePreviewResponse {
        info: FileInfo {
            name: path
                .file_name()
                .map(|name| name.to_string_lossy().into_owned())
                .unwrap_or_default(),
            mime_type,
            size: stat.size,
            modified_epoch_ms: stat.modified_epoch_ms,
        },
        preview,
    })
}

fn is_text_like(mime: &str) -> bool {
    mime.starts_with("text/")
        || matches!(
            mime,
            "application/json"
                | "application/yaml"
                | "application/toml"
                | "application/xml"
                | "application/sql"
                | "application/javascript"
                | "image/svg+xml"
        )
}

fn text_preview(path: &Path, size: u64) -> Result<FilePreview, VaultFileError> {
    let limit = limits::PREVIEW_TEXT_BYTES;
    let mut bytes = Vec::with_capacity(limit.min(size as usize));
    fs::File::open(path)
        .map_err(map_not_found)?
        .take(limit as u64)
        .read_to_end(&mut bytes)?;
    let truncated = size > limit as u64;
    Ok(FilePreview::Text {
        excerpt: excerpt_on_char_boundary(&bytes, truncated),
        truncated,
    })
}

/// Lossy UTF-8 of `bytes`; a multibyte char split by the byte cut is dropped
/// rather than rendered as U+FFFD.
fn excerpt_on_char_boundary(bytes: &[u8], truncated: bool) -> String {
    let cut = match std::str::from_utf8(bytes) {
        Err(err) if truncated && err.error_len().is_none() => err.valid_up_to(),
        _ => bytes.len(),
    };
    String::from_utf8_lossy(&bytes[..cut]).into_owned()
}

fn image_preview(path: &Path, source_mime: &str) -> Result<FilePreview, VaultFileError> {
    use base64::Engine;

    let bytes = read_image_for_prompt(path)?;
    let decoded = image::load_from_memory(&bytes).map_err(image_decode_error)?;
    let side = limits::PREVIEW_IMAGE_MAX_SIDE;
    let thumbnail = if decoded.width() > side || decoded.height() > side {
        decoded.thumbnail(side, side)
    } else {
        decoded
    };
    let (format, mime_type, encodable) = if source_mime == "image/jpeg" {
        (
            image::ImageFormat::Jpeg,
            "image/jpeg",
            image::DynamicImage::ImageRgb8(thumbnail.to_rgb8()),
        )
    } else {
        (image::ImageFormat::Png, "image/png", thumbnail)
    };
    let mut encoded = Vec::new();
    encodable
        .write_to(&mut Cursor::new(&mut encoded), format)
        .map_err(image_decode_error)?;
    Ok(FilePreview::Image {
        data: base64::engine::general_purpose::STANDARD.encode(encoded),
        mime_type: mime_type.to_string(),
        width: encodable.width(),
        height: encodable.height(),
    })
}

fn image_decode_error(err: image::ImageError) -> VaultFileError {
    match err {
        image::ImageError::IoError(io) => map_not_found(io),
        other => VaultFileError::Io(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            format!("unreadable image: {other}"),
        )),
    }
}

fn map_not_found(err: std::io::Error) -> VaultFileError {
    if err.kind() == std::io::ErrorKind::NotFound {
        VaultFileError::NotFound
    } else {
        VaultFileError::Io(err)
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use tempfile::tempdir;

    use super::limits::{attachment_limits, IMAGE_MAX_BYTES, IMAGE_MAX_SIDE, PREVIEW_TEXT_BYTES};
    use super::{
        is_raster_image, mime_for_file, read_bounded, read_image_for_prompt, relativize_vault_path,
        resolve_vault_file, stat_vault_file, FilePreview, PreviewCache, VaultFileError,
    };

    fn text_excerpt(preview: &FilePreview) -> (&str, bool) {
        match preview {
            FilePreview::Text { excerpt, truncated } => (excerpt, *truncated),
            other => panic!("expected text preview, got {other:?}"),
        }
    }

    fn write_png(path: &std::path::Path, width: u32, height: u32) {
        image::RgbaImage::from_pixel(width, height, image::Rgba([10, 20, 30, 255]))
            .save_with_format(path, image::ImageFormat::Png)
            .unwrap();
    }

    #[test]
    fn resolves_a_regular_file_inside_the_vault() {
        let vault = tempdir().unwrap();
        fs::create_dir_all(vault.path().join("notes")).unwrap();
        fs::write(vault.path().join("notes/a.md"), "hello").unwrap();

        let resolved = resolve_vault_file(vault.path(), "notes/a.md").unwrap();

        assert_eq!(
            resolved,
            fs::canonicalize(vault.path().join("notes/a.md")).unwrap()
        );
    }

    #[test]
    fn rejects_parent_directory_traversal() {
        let vault = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "x").unwrap();
        let traversal = format!(
            "../{}/secret.txt",
            outside.path().file_name().unwrap().to_str().unwrap()
        );

        let err = resolve_vault_file(vault.path(), &traversal).unwrap_err();

        assert!(matches!(err, VaultFileError::InvalidPath), "{err:?}");
    }

    #[test]
    fn rejects_absolute_input_even_when_inside_the_vault() {
        let vault = tempdir().unwrap();
        let file = vault.path().join("a.md");
        fs::write(&file, "x").unwrap();

        let err = resolve_vault_file(vault.path(), file.to_str().unwrap()).unwrap_err();

        assert!(matches!(err, VaultFileError::InvalidPath), "{err:?}");
    }

    #[cfg(unix)]
    #[test]
    fn rejects_symlink_that_resolves_outside_the_vault() {
        let vault = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("secret.txt"), "x").unwrap();
        std::os::unix::fs::symlink(
            outside.path().join("secret.txt"),
            vault.path().join("link.txt"),
        )
        .unwrap();

        let err = resolve_vault_file(vault.path(), "link.txt").unwrap_err();

        assert!(matches!(err, VaultFileError::InvalidPath), "{err:?}");
    }

    #[cfg(unix)]
    #[test]
    fn accepts_symlink_that_stays_inside_the_vault() {
        let vault = tempdir().unwrap();
        fs::write(vault.path().join("real.md"), "x").unwrap();
        std::os::unix::fs::symlink(vault.path().join("real.md"), vault.path().join("link.md"))
            .unwrap();

        let resolved = resolve_vault_file(vault.path(), "link.md").unwrap();

        assert_eq!(
            resolved,
            fs::canonicalize(vault.path().join("real.md")).unwrap()
        );
    }

    #[test]
    fn rejects_directories() {
        let vault = tempdir().unwrap();
        fs::create_dir_all(vault.path().join("dir")).unwrap();

        let err = resolve_vault_file(vault.path(), "dir").unwrap_err();

        assert!(matches!(err, VaultFileError::NotAFile), "{err:?}");
    }

    #[test]
    fn reports_missing_files_as_not_found() {
        let vault = tempdir().unwrap();

        let err = resolve_vault_file(vault.path(), "nope.md").unwrap_err();

        assert!(matches!(err, VaultFileError::NotFound), "{err:?}");
    }

    #[test]
    fn relativizes_absolute_paths_inside_the_vault_with_forward_slashes() {
        let vault = tempdir().unwrap();
        fs::create_dir_all(vault.path().join("sub/dir")).unwrap();
        fs::write(vault.path().join("sub/dir/a.md"), "x").unwrap();

        let relative =
            relativize_vault_path(vault.path(), &vault.path().join("sub/dir/a.md")).unwrap();

        assert_eq!(relative, "sub/dir/a.md");
    }

    #[test]
    fn relativize_rejects_paths_outside_the_vault() {
        let vault = tempdir().unwrap();
        let outside = tempdir().unwrap();
        fs::write(outside.path().join("a.md"), "x").unwrap();

        let err = relativize_vault_path(vault.path(), &outside.path().join("a.md")).unwrap_err();

        assert!(matches!(err, VaultFileError::InvalidPath), "{err:?}");
    }

    #[test]
    fn relativize_rejects_relative_input() {
        let vault = tempdir().unwrap();

        let err = relativize_vault_path(vault.path(), std::path::Path::new("a.md")).unwrap_err();

        assert!(matches!(err, VaultFileError::InvalidPath), "{err:?}");
    }

    #[test]
    fn stat_reports_size_and_modified_time() {
        let vault = tempdir().unwrap();
        let file = vault.path().join("a.md");
        fs::write(&file, "hello").unwrap();
        let expected_mtime = fs::metadata(&file)
            .unwrap()
            .modified()
            .unwrap()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_millis() as u64;

        let stat = stat_vault_file(vault.path(), "a.md").unwrap();

        assert_eq!(stat.size, 5);
        assert_eq!(stat.modified_epoch_ms, expected_mtime);
    }

    #[test]
    fn read_bounded_returns_bytes_within_the_limit() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("a.txt");
        fs::write(&file, "hello").unwrap();

        assert_eq!(read_bounded(&file, 5).unwrap(), b"hello");
    }

    #[test]
    fn read_bounded_rejects_files_over_the_limit() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("a.txt");
        fs::write(&file, "hello!").unwrap();

        let err = read_bounded(&file, 5).unwrap_err();

        assert!(
            matches!(err, VaultFileError::TooLarge { limit: 5 }),
            "{err:?}"
        );
    }

    #[test]
    fn mime_uses_the_extension_table_with_octet_stream_fallback() {
        let dir = tempdir().unwrap();
        let cases = [
            ("a.md", "text/markdown"),
            ("a.txt", "text/plain"),
            ("a.rs", "text/x-rust"),
            ("a.json", "application/json"),
            ("a.YAML", "application/yaml"),
            ("a.csv", "text/csv"),
            ("a.bin", "application/octet-stream"),
            ("noext", "application/octet-stream"),
        ];
        for (name, expected) in cases {
            let file = dir.path().join(name);
            fs::write(&file, "x").unwrap();
            assert_eq!(mime_for_file(&file), expected, "{name}");
        }
    }

    #[test]
    fn mime_sniffs_image_magic_and_rejects_mismatches() {
        let dir = tempdir().unwrap();
        let real = dir.path().join("real.png");
        write_png(&real, 2, 2);
        let fake = dir.path().join("fake.png");
        fs::write(&fake, "not a png").unwrap();
        let jpeg_named_png = dir.path().join("wrong.png");
        fs::write(&jpeg_named_png, [0xFF, 0xD8, 0xFF, 0xE0, 0, 0]).unwrap();

        assert_eq!(mime_for_file(&real), "image/png");
        assert_eq!(mime_for_file(&fake), "application/octet-stream");
        assert_eq!(mime_for_file(&jpeg_named_png), "application/octet-stream");
    }

    #[test]
    fn raster_image_mimes_are_png_jpeg_gif_webp_only() {
        for mime in ["image/png", "image/jpeg", "image/gif", "image/webp"] {
            assert!(is_raster_image(mime), "{mime}");
        }
        for mime in ["image/svg+xml", "text/plain", "application/pdf"] {
            assert!(!is_raster_image(mime), "{mime}");
        }
    }

    #[test]
    fn image_for_prompt_returns_bytes_of_a_small_image() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("a.png");
        write_png(&file, 4, 3);

        let bytes = read_image_for_prompt(&file).unwrap();

        assert_eq!(bytes, fs::read(&file).unwrap());
    }

    #[test]
    fn image_for_prompt_rejects_files_over_the_byte_limit_before_reading() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("big.png");
        let handle = fs::File::create(&file).unwrap();
        handle.set_len(IMAGE_MAX_BYTES + 1).unwrap();

        let err = read_image_for_prompt(&file).unwrap_err();

        assert!(
            matches!(err, VaultFileError::TooLarge { limit } if limit == IMAGE_MAX_BYTES),
            "{err:?}"
        );
    }

    #[test]
    fn image_for_prompt_rejects_images_over_the_side_limit() {
        let dir = tempdir().unwrap();
        let file = dir.path().join("tall.png");
        write_png(&file, 1, IMAGE_MAX_SIDE + 1);

        let err = read_image_for_prompt(&file).unwrap_err();

        assert!(
            matches!(err, VaultFileError::TooLarge { limit } if limit == u64::from(IMAGE_MAX_SIDE)),
            "{err:?}"
        );
    }

    #[test]
    fn text_preview_returns_the_whole_small_file_with_info() {
        let vault = tempdir().unwrap();
        fs::write(vault.path().join("note.md"), "# Title\n\nbody").unwrap();

        let response = PreviewCache::default()
            .preview(vault.path(), "note.md")
            .unwrap();

        assert_eq!(response.info.name, "note.md");
        assert_eq!(response.info.mime_type, "text/markdown");
        assert_eq!(response.info.size, 13);
        assert_eq!(text_excerpt(&response.preview), ("# Title\n\nbody", false));
    }

    #[test]
    fn text_preview_truncates_at_the_byte_limit_on_a_char_boundary() {
        let vault = tempdir().unwrap();
        let mut content = "a".repeat(PREVIEW_TEXT_BYTES - 1);
        content.push('€'); // 3 bytes, straddles the cut
        content.push_str(" tail");
        fs::write(vault.path().join("big.txt"), &content).unwrap();

        let response = PreviewCache::default()
            .preview(vault.path(), "big.txt")
            .unwrap();

        let (excerpt, truncated) = text_excerpt(&response.preview);
        assert!(truncated);
        assert_eq!(excerpt, "a".repeat(PREVIEW_TEXT_BYTES - 1));
    }

    #[test]
    fn image_preview_is_a_downscaled_base64_png_thumbnail() {
        let vault = tempdir().unwrap();
        write_png(&vault.path().join("wide.png"), 1024, 256);

        let response = PreviewCache::default()
            .preview(vault.path(), "wide.png")
            .unwrap();

        assert_eq!(response.info.mime_type, "image/png");
        match response.preview {
            FilePreview::Image {
                data,
                mime_type,
                width,
                height,
            } => {
                assert_eq!((width, height), (512, 128));
                assert_eq!(mime_type, "image/png");
                use base64::Engine;
                let bytes = base64::engine::general_purpose::STANDARD
                    .decode(data)
                    .unwrap();
                assert!(bytes.starts_with(b"\x89PNG"));
            }
            other => panic!("expected image preview, got {other:?}"),
        }
    }

    #[test]
    fn image_preview_keeps_small_images_at_their_size_and_jpeg_stays_jpeg() {
        let vault = tempdir().unwrap();
        image::RgbImage::from_pixel(4, 3, image::Rgb([1, 2, 3]))
            .save_with_format(vault.path().join("tiny.jpg"), image::ImageFormat::Jpeg)
            .unwrap();

        let response = PreviewCache::default()
            .preview(vault.path(), "tiny.jpg")
            .unwrap();

        match response.preview {
            FilePreview::Image {
                mime_type,
                width,
                height,
                ..
            } => {
                assert_eq!((width, height), (4, 3));
                assert_eq!(mime_type, "image/jpeg");
            }
            other => panic!("expected image preview, got {other:?}"),
        }
    }

    #[test]
    fn image_preview_rejects_over_limit_images() {
        let vault = tempdir().unwrap();
        write_png(&vault.path().join("tall.png"), 1, IMAGE_MAX_SIDE + 1);

        let err = PreviewCache::default()
            .preview(vault.path(), "tall.png")
            .unwrap_err();

        assert!(matches!(err, VaultFileError::TooLarge { .. }), "{err:?}");
    }

    #[test]
    fn unknown_types_get_no_preview_but_still_info() {
        let vault = tempdir().unwrap();
        fs::write(vault.path().join("blob.bin"), [0, 1, 2]).unwrap();

        let response = PreviewCache::default()
            .preview(vault.path(), "blob.bin")
            .unwrap();

        assert!(matches!(response.preview, FilePreview::None));
        assert_eq!(response.info.mime_type, "application/octet-stream");
        assert_eq!(response.info.size, 3);
    }

    #[test]
    fn preview_goes_through_the_vault_boundary() {
        let vault = tempdir().unwrap();
        let cache = PreviewCache::default();

        assert!(matches!(
            cache.preview(vault.path(), "nope.md").unwrap_err(),
            VaultFileError::NotFound
        ));
        assert!(matches!(
            cache.preview(vault.path(), "../etc/passwd").unwrap_err(),
            VaultFileError::InvalidPath
        ));
    }

    #[test]
    fn preview_cache_hits_while_mtime_and_size_are_unchanged_and_misses_after_change() {
        let vault = tempdir().unwrap();
        let file = vault.path().join("note.txt");
        fs::write(&file, "aaaa").unwrap();
        let mtime = filetime::FileTime::from_last_modification_time(&fs::metadata(&file).unwrap());
        let cache = PreviewCache::default();

        let first = cache.preview(vault.path(), "note.txt").unwrap();
        assert_eq!(text_excerpt(&first.preview).0, "aaaa");

        // Same size and restored mtime: stat is unchanged, so the cache answers.
        fs::write(&file, "bbbb").unwrap();
        filetime::set_file_mtime(&file, mtime).unwrap();
        let cached = cache.preview(vault.path(), "note.txt").unwrap();
        assert_eq!(text_excerpt(&cached.preview).0, "aaaa");

        // Different size: the cache misses and reads the new content.
        fs::write(&file, "cccccc").unwrap();
        let fresh = cache.preview(vault.path(), "note.txt").unwrap();
        assert_eq!(text_excerpt(&fresh.preview).0, "cccccc");
        assert_eq!(fresh.info.size, 6);
    }

    #[test]
    fn attachment_limits_serialize_the_four_constants() {
        let json = serde_json::to_value(attachment_limits()).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "image_max_bytes": 5 * 1024 * 1024,
                "image_max_side": 8000,
                "max_images_per_prompt": 20,
                "preview_text_bytes": 16 * 1024,
            })
        );
    }
}
