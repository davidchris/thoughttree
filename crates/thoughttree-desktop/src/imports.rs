use std::{fs::File, io::Read, path::Path};

pub const KAGI_EXPORT_MAX_BYTES: u64 = 16 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ImportError {
    Io(String),
    InputTooLarge { input_bytes: u64, limit_bytes: u64 },
    InvalidUtf8,
}

impl std::fmt::Display for ImportError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io(message) => write!(f, "{message}"),
            Self::InputTooLarge {
                input_bytes,
                limit_bytes,
            } => write!(
                f,
                "Kagi export exceeds the {limit_bytes}-byte input limit ({input_bytes} bytes)"
            ),
            Self::InvalidUtf8 => write!(f, "Kagi export is not valid UTF-8 text"),
        }
    }
}

impl std::error::Error for ImportError {}

/// Read only a file explicitly selected by the user. Parsing and fresh graph
/// IDs belong to the graph model, matching the existing transport boundary.
pub fn read_kagi_export(path: &Path) -> Result<String, ImportError> {
    let file = File::open(path)
        .map_err(|e| ImportError::Io(format!("Unable to open Kagi export: {e}")))?;
    let size = file
        .metadata()
        .map_err(|e| ImportError::Io(e.to_string()))?
        .len();
    read_kagi_text(file, size)
}

fn read_kagi_text(reader: impl Read, size: u64) -> Result<String, ImportError> {
    if size > KAGI_EXPORT_MAX_BYTES {
        return Err(too_large(size));
    }
    let mut bytes = Vec::with_capacity(size as usize);
    reader
        .take(KAGI_EXPORT_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|e| ImportError::Io(format!("Unable to read Kagi export: {e}")))?;
    if bytes.len() as u64 > KAGI_EXPORT_MAX_BYTES {
        return Err(too_large(bytes.len() as u64));
    }
    String::from_utf8(bytes).map_err(|_| ImportError::InvalidUtf8)
}

fn too_large(size: u64) -> ImportError {
    ImportError::InputTooLarge {
        input_bytes: size,
        limit_bytes: KAGI_EXPORT_MAX_BYTES,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rejects_a_growing_file_without_reading_unbounded_input() {
        let error = read_kagi_text(std::io::repeat(b'a'), 0).unwrap_err();
        assert_eq!(error, too_large(KAGI_EXPORT_MAX_BYTES + 1));
    }

    #[test]
    fn rejects_invalid_utf8_and_oversize_metadata() {
        assert_eq!(
            read_kagi_text(&[0xff][..], 1).unwrap_err(),
            ImportError::InvalidUtf8
        );
        assert_eq!(
            read_kagi_text(&[][..], KAGI_EXPORT_MAX_BYTES + 1).unwrap_err(),
            too_large(KAGI_EXPORT_MAX_BYTES + 1)
        );
    }

    #[test]
    fn reads_the_same_sanitized_kagi_fixture_as_tauri() {
        let fixture =
            Path::new(env!("CARGO_MANIFEST_DIR")).join("../../test/fixtures/kagi-export-v1.json");
        assert_eq!(
            read_kagi_export(&fixture).unwrap(),
            std::fs::read_to_string(fixture).unwrap()
        );
    }
}
