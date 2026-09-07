use std::fs::File;
use std::io::Read;
use std::path::Path;

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

const KAGI_EXPORT_MAX_BYTES: u64 = 16 * 1024 * 1024;

/// Typed, serializable failure for the Kagi import file seam. `kind` is a
/// stable discriminator for the frontend; `message` is user-readable.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum KagiImportError {
    Io {
        message: String,
    },
    InputTooLarge {
        message: String,
        input_bytes: u64,
        limit_bytes: u64,
    },
    InvalidUtf8 {
        message: String,
    },
}

impl KagiImportError {
    fn io(context: &str, error: std::io::Error) -> Self {
        Self::Io {
            message: format!("{context}: {error}"),
        }
    }

    fn input_too_large(input_bytes: u64) -> Self {
        Self::InputTooLarge {
            message: format!(
                "Kagi export exceeds the {KAGI_EXPORT_MAX_BYTES}-byte input limit ({input_bytes} bytes)"
            ),
            input_bytes,
            limit_bytes: KAGI_EXPORT_MAX_BYTES,
        }
    }

    fn invalid_utf8() -> Self {
        Self::InvalidUtf8 {
            message: "Kagi export is not valid UTF-8 text".to_string(),
        }
    }
}

pub(crate) fn read_kagi_export(path: &Path) -> Result<String, KagiImportError> {
    let file = File::open(path)
        .map_err(|error| KagiImportError::io("Unable to open Kagi export", error))?;
    let size = file
        .metadata()
        .map_err(|error| KagiImportError::io("Unable to inspect Kagi export", error))?
        .len();
    read_kagi_text(file, size)
}

fn read_kagi_text(reader: impl Read, size: u64) -> Result<String, KagiImportError> {
    if size > KAGI_EXPORT_MAX_BYTES {
        return Err(KagiImportError::input_too_large(size));
    }

    let mut bytes = Vec::with_capacity(size.min(KAGI_EXPORT_MAX_BYTES) as usize);
    reader
        .take(KAGI_EXPORT_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| KagiImportError::io("Unable to read Kagi export", error))?;
    if bytes.len() as u64 > KAGI_EXPORT_MAX_BYTES {
        return Err(KagiImportError::input_too_large(bytes.len() as u64));
    }
    String::from_utf8(bytes).map_err(|_| KagiImportError::invalid_utf8())
}

#[tauri::command]
pub(crate) async fn import_kagi_export(path: String) -> Result<String, KagiImportError> {
    read_kagi_export(Path::new(&path))
}

#[tauri::command]
pub(crate) async fn pick_kagi_export(app: AppHandle) -> Result<Option<String>, String> {
    Ok(app
        .dialog()
        .file()
        .set_title("Import Kagi Export")
        .add_filter("JSON", &["json"])
        .blocking_pick_file()
        .map(|path| path.to_string()))
}

#[cfg(test)]
mod tests {
    use super::{read_kagi_export, read_kagi_text, KagiImportError, KAGI_EXPORT_MAX_BYTES};
    use std::io::{Cursor, Read};
    use std::path::Path;

    fn fixture_path() -> &'static Path {
        Path::new(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../test/fixtures/kagi-export-v1.json"
        ))
    }

    #[test]
    fn reads_sanitized_fixture_as_exact_text() {
        let expected = std::fs::read_to_string(fixture_path()).unwrap();
        assert_eq!(read_kagi_export(fixture_path()).unwrap(), expected);
    }

    #[test]
    fn caps_bytes_consumed_even_when_the_reported_size_is_stale() {
        // An endless source represents a file that keeps growing after metadata.
        let mut source = std::io::repeat(b'a');
        let mut counted = (&mut source).take(KAGI_EXPORT_MAX_BYTES * 2);
        let error = read_kagi_text(&mut counted, 0).unwrap_err();

        assert!(
            matches!(error, KagiImportError::InputTooLarge { input_bytes, .. }
            if input_bytes == KAGI_EXPORT_MAX_BYTES + 1)
        );
        assert_eq!(counted.limit(), KAGI_EXPORT_MAX_BYTES - 1);
    }

    #[test]
    fn accepts_exactly_the_byte_limit() {
        let bytes = vec![b'a'; KAGI_EXPORT_MAX_BYTES as usize];
        assert_eq!(
            read_kagi_text(Cursor::new(&bytes), KAGI_EXPORT_MAX_BYTES).unwrap(),
            String::from_utf8(bytes).unwrap()
        );
    }

    #[test]
    fn rejects_oversize_input_with_typed_error() {
        let path = tempfile::NamedTempFile::new().unwrap();
        let size = 16 * 1024 * 1024 + 1;
        path.as_file().set_len(size as u64).unwrap();

        let error = read_kagi_export(path.path()).unwrap_err();

        assert_eq!(
            error,
            KagiImportError::InputTooLarge {
                message: format!(
                    "Kagi export exceeds the {}-byte input limit ({} bytes)",
                    16 * 1024 * 1024,
                    size
                ),
                input_bytes: size as u64,
                limit_bytes: 16 * 1024 * 1024,
            }
        );
        assert_eq!(
            serde_json::to_value(&error).unwrap(),
            serde_json::json!({
                "kind": "input_too_large",
                "message": format!(
                    "Kagi export exceeds the {}-byte input limit ({} bytes)",
                    16 * 1024 * 1024,
                    size
                ),
                "input_bytes": size,
                "limit_bytes": 16 * 1024 * 1024,
            })
        );
    }

    #[test]
    fn rejects_invalid_utf8_input_with_typed_error() {
        let path = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(path.path(), [0xff, 0xfe, 0xfd]).unwrap();

        let error = read_kagi_export(path.path()).unwrap_err();

        assert_eq!(
            error,
            KagiImportError::InvalidUtf8 {
                message: "Kagi export is not valid UTF-8 text".to_string()
            }
        );
        assert_eq!(
            serde_json::to_value(&error).unwrap()["kind"],
            serde_json::json!("invalid_utf8")
        );
    }

    #[test]
    fn reports_missing_file_as_io_error() {
        let dir = tempfile::tempdir().unwrap();
        let error = read_kagi_export(&dir.path().join("missing.json")).unwrap_err();

        match error {
            KagiImportError::Io { message } => {
                assert!(
                    message.starts_with("Unable to open Kagi export: "),
                    "{message}"
                );
            }
            other => panic!("expected Io error, got {other:?}"),
        }
    }
}
