use std::fs::File;
use std::io::Read;
use std::path::Path;

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

const KAGI_EXPORT_MAX_BYTES: u64 = 16 * 1024 * 1024;

pub(crate) fn read_kagi_export(path: &Path) -> Result<String, String> {
    let mut file =
        File::open(path).map_err(|error| format!("Unable to open Kagi export: {error}"))?;
    let size = file
        .metadata()
        .map_err(|error| format!("Unable to inspect Kagi export: {error}"))?
        .len();
    if size > KAGI_EXPORT_MAX_BYTES {
        return Err(format!(
            "Kagi export exceeds the {KAGI_EXPORT_MAX_BYTES}-byte input limit ({size} bytes)"
        ));
    }

    let mut bytes = Vec::with_capacity(size.min(KAGI_EXPORT_MAX_BYTES) as usize);
    file.take(KAGI_EXPORT_MAX_BYTES + 1)
        .read_to_end(&mut bytes)
        .map_err(|error| format!("Unable to read Kagi export: {error}"))?;
    if bytes.len() as u64 > KAGI_EXPORT_MAX_BYTES {
        return Err(format!(
            "Kagi export exceeds the {KAGI_EXPORT_MAX_BYTES}-byte input limit ({} bytes)",
            bytes.len()
        ));
    }
    String::from_utf8(bytes).map_err(|_| "Invalid Kagi export JSON".to_string())
}

#[tauri::command]
pub(crate) async fn import_kagi_export(path: String) -> Result<String, String> {
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
    use super::read_kagi_export;
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
    fn rejects_oversize_input() {
        let path = tempfile::NamedTempFile::new().unwrap();
        let size = 16 * 1024 * 1024 + 1;
        path.as_file().set_len(size as u64).unwrap();

        let error = read_kagi_export(path.path()).unwrap_err();

        assert_eq!(
            error,
            format!(
                "Kagi export exceeds the {}-byte input limit ({} bytes)",
                16 * 1024 * 1024,
                size
            )
        );
    }

    #[test]
    fn rejects_invalid_utf8_input() {
        let path = tempfile::NamedTempFile::new().unwrap();
        std::fs::write(path.path(), [0xff, 0xfe, 0xfd]).unwrap();

        assert_eq!(
            read_kagi_export(path.path()).unwrap_err(),
            "Invalid Kagi export JSON"
        );
    }
}
