use std::fs;
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use thoughttree_core::vault::{guarded_write_file, read_project_file, Revision, VaultError};
use walkdir::WalkDir;

use crate::backend::config;

#[derive(Debug, serde::Serialize)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub(crate) enum ProjectCommandError {
    Message { message: String },
    StaleRevision { current_revision: String },
}

#[derive(Debug, serde::Serialize)]
pub(crate) struct LoadProjectResponse {
    content: String,
    revision: String,
}

#[derive(Clone, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct ProjectEntry {
    relative_path: String,
    modified_epoch_ms: u64,
}

/// Resolves `path` and confirms it stays inside the configured notes directory.
/// Symlinks are followed before the check, so a link pointing outside the
/// vault is rejected the same way a plain outside path is. Relative paths are
/// rejected outright: they would resolve against the process working directory,
/// not the vault.
fn validate_path_in_notes_dir(path: &Path, notes_directory: &Path) -> Result<PathBuf, String> {
    if !path.is_absolute() {
        return Err("Security error: project path must be absolute".to_string());
    }

    let canonical_notes = fs::canonicalize(notes_directory)
        .map_err(|err| format!("Failed to resolve notes directory: {err}"))?;

    let canonical_path = if path.exists() {
        fs::canonicalize(path).map_err(|err| format!("Failed to resolve project path: {err}"))?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| "Invalid project path: no parent directory".to_string())?;
        let file_name = path
            .file_name()
            .ok_or_else(|| "Invalid project path: no file name".to_string())?;
        let canonical_parent = fs::canonicalize(parent)
            .map_err(|err| format!("Failed to resolve project directory: {err}"))?;
        canonical_parent.join(file_name)
    };

    if !canonical_path.starts_with(&canonical_notes) {
        return Err("Security error: project path is outside the notes directory".to_string());
    }

    Ok(canonical_path)
}

fn collect_project_entries(notes_directory: &Path) -> Result<Vec<ProjectEntry>, String> {
    let mut projects = Vec::new();

    for entry in WalkDir::new(notes_directory)
        .follow_links(false)
        .max_depth(20)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let rel_path = match entry.path().strip_prefix(notes_directory) {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        if !rel_path.ends_with(".thoughttree") {
            continue;
        }

        let metadata = fs::metadata(entry.path())
            .map_err(|err| format!("Failed to read project metadata for {rel_path}: {err}"))?;
        let modified_epoch_ms = metadata
            .modified()
            .map_err(|err| format!("Failed to read project modified time for {rel_path}: {err}"))?
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis() as u64;

        projects.push(ProjectEntry {
            relative_path: rel_path,
            modified_epoch_ms,
        });
    }

    projects.sort_by(|left, right| {
        right
            .modified_epoch_ms
            .cmp(&left.modified_epoch_ms)
            .then_with(|| left.relative_path.cmp(&right.relative_path))
    });

    Ok(projects)
}

fn map_load_error(err: VaultError) -> ProjectCommandError {
    match err {
        VaultError::NotFound => ProjectCommandError::Message {
            message: "Project file not found".to_string(),
        },
        VaultError::InvalidPath => ProjectCommandError::Message {
            message: "Invalid project path".to_string(),
        },
        VaultError::Stale { .. } => ProjectCommandError::Message {
            message: "Unexpected stale revision while loading project".to_string(),
        },
        VaultError::Io(err) => ProjectCommandError::Message {
            message: format!("Failed to load project: {err}"),
        },
    }
}

fn map_save_error(err: VaultError) -> ProjectCommandError {
    match err {
        VaultError::Stale { current } => ProjectCommandError::StaleRevision {
            current_revision: current.0,
        },
        VaultError::NotFound => ProjectCommandError::Message {
            message: "Project file not found".to_string(),
        },
        VaultError::InvalidPath => ProjectCommandError::Message {
            message: "Invalid project path".to_string(),
        },
        VaultError::Io(err) => ProjectCommandError::Message {
            message: format!("Failed to save project: {err}"),
        },
    }
}

fn command_message(message: String) -> ProjectCommandError {
    ProjectCommandError::Message { message }
}

fn save_project_in_notes_dir(
    notes_directory: &Path,
    path: &Path,
    data: &str,
    base_revision: Option<&Revision>,
) -> Result<(PathBuf, Revision), ProjectCommandError> {
    let validated_path =
        validate_path_in_notes_dir(path, notes_directory).map_err(command_message)?;
    let revision =
        guarded_write_file(&validated_path, data, base_revision).map_err(map_save_error)?;
    Ok((validated_path, revision))
}

fn load_project_in_notes_dir(
    notes_directory: &Path,
    path: &Path,
) -> Result<(PathBuf, LoadProjectResponse), ProjectCommandError> {
    let validated_path =
        validate_path_in_notes_dir(path, notes_directory).map_err(command_message)?;
    let project = read_project_file(&validated_path).map_err(map_load_error)?;
    Ok((
        validated_path,
        LoadProjectResponse {
            content: project.content,
            revision: project.revision.0,
        },
    ))
}

#[cfg(test)]
mod tests {
    use std::fs;

    use filetime::{set_file_mtime, FileTime};
    use tempfile::tempdir;

    use super::*;

    #[test]
    fn collect_project_entries_filters_to_project_files_and_preserves_metadata() {
        let dir = tempdir().unwrap();
        fs::write(dir.path().join("alpha.thoughttree"), "{}").unwrap();
        fs::create_dir(dir.path().join("nested")).unwrap();
        fs::write(dir.path().join("nested").join("beta.thoughttree"), "{}").unwrap();
        fs::write(dir.path().join("notes.md"), "# note").unwrap();

        let entries = collect_project_entries(dir.path()).unwrap();
        let mut paths = entries
            .iter()
            .map(|entry| entry.relative_path.as_str())
            .collect::<Vec<_>>();
        paths.sort_unstable();

        assert_eq!(entries.len(), 2);
        assert_eq!(paths, vec!["alpha.thoughttree", "nested/beta.thoughttree"]);
        assert!(entries.iter().all(|entry| entry.modified_epoch_ms > 0));
    }

    #[test]
    fn collect_project_entries_sorts_newest_first_then_path() {
        let dir = tempdir().unwrap();
        let alpha = dir.path().join("alpha.thoughttree");
        let beta = dir.path().join("beta.thoughttree");
        let newer = dir.path().join("newer.thoughttree");

        fs::write(&alpha, "{}").unwrap();
        fs::write(&beta, "{}").unwrap();
        fs::write(&newer, "{}").unwrap();

        let shared_time = FileTime::from_unix_time(1_700_000_000, 0);
        let newer_time = FileTime::from_unix_time(1_700_000_100, 0);
        set_file_mtime(&alpha, shared_time).unwrap();
        set_file_mtime(&beta, shared_time).unwrap();
        set_file_mtime(&newer, newer_time).unwrap();

        let entries = collect_project_entries(dir.path()).unwrap();

        assert_eq!(
            entries
                .iter()
                .map(|entry| entry.relative_path.as_str())
                .collect::<Vec<_>>(),
            vec!["newer.thoughttree", "alpha.thoughttree", "beta.thoughttree"]
        );
        assert!(entries[0].modified_epoch_ms > entries[1].modified_epoch_ms);
        assert_eq!(entries[1].modified_epoch_ms, entries[2].modified_epoch_ms);
    }

    #[test]
    fn save_project_rejects_paths_outside_notes_directory() {
        let dir = tempdir().unwrap();
        let notes_directory = dir.path().join("notes");
        let outside_path = dir.path().join("outside.thoughttree");
        fs::create_dir(&notes_directory).unwrap();
        fs::write(&outside_path, "outside content").unwrap();

        let error =
            save_project_in_notes_dir(&notes_directory, &outside_path, "changed content", None)
                .unwrap_err();

        assert!(matches!(
            error,
            ProjectCommandError::Message { message }
                if message == "Security error: project path is outside the notes directory"
        ));
        assert_eq!(fs::read_to_string(outside_path).unwrap(), "outside content");
    }

    #[test]
    fn load_project_rejects_paths_outside_notes_directory() {
        let dir = tempdir().unwrap();
        let notes_directory = dir.path().join("notes");
        let outside_path = dir.path().join("outside.thoughttree");
        fs::create_dir(&notes_directory).unwrap();
        fs::write(&outside_path, "outside content").unwrap();

        let error = load_project_in_notes_dir(&notes_directory, &outside_path).unwrap_err();

        assert!(matches!(
            error,
            ProjectCommandError::Message { message }
                if message == "Security error: project path is outside the notes directory"
        ));
    }

    fn assert_outside_notes_directory(error: ProjectCommandError) {
        assert!(matches!(
            error,
            ProjectCommandError::Message { message }
                if message == "Security error: project path is outside the notes directory"
        ));
    }

    #[test]
    fn load_and_save_reject_relative_traversal_paths() {
        let dir = tempdir().unwrap();
        let notes_directory = dir.path().join("notes");
        let outside_path = dir.path().join("outside.thoughttree");
        fs::create_dir(&notes_directory).unwrap();
        fs::write(&outside_path, "outside content").unwrap();

        let traversal = notes_directory.join("..").join("outside.thoughttree");
        let missing_traversal = notes_directory.join("..").join("new.thoughttree");

        assert_outside_notes_directory(
            load_project_in_notes_dir(&notes_directory, &traversal).unwrap_err(),
        );
        assert_outside_notes_directory(
            save_project_in_notes_dir(&notes_directory, &traversal, "changed", None).unwrap_err(),
        );
        assert_outside_notes_directory(
            save_project_in_notes_dir(&notes_directory, &missing_traversal, "new", None)
                .unwrap_err(),
        );
        assert_eq!(
            fs::read_to_string(&outside_path).unwrap(),
            "outside content"
        );
        assert!(!dir.path().join("new.thoughttree").exists());
    }

    #[test]
    fn load_and_save_reject_non_absolute_paths() {
        let dir = tempdir().unwrap();
        let notes_directory = dir.path().join("notes");
        fs::create_dir(&notes_directory).unwrap();
        fs::write(notes_directory.join("inside.thoughttree"), "{}").unwrap();

        let relative = Path::new("inside.thoughttree");

        for error in [
            load_project_in_notes_dir(&notes_directory, relative).unwrap_err(),
            save_project_in_notes_dir(&notes_directory, relative, "changed", None).unwrap_err(),
        ] {
            assert!(matches!(
                error,
                ProjectCommandError::Message { message }
                    if message == "Security error: project path must be absolute"
            ));
        }
    }

    #[cfg(unix)]
    #[test]
    fn load_and_save_reject_symlink_escapes() {
        use std::os::unix::fs::symlink;

        let dir = tempdir().unwrap();
        let notes_directory = dir.path().join("notes");
        let outside_dir = dir.path().join("outside");
        let outside_file = outside_dir.join("secret.thoughttree");
        fs::create_dir(&notes_directory).unwrap();
        fs::create_dir(&outside_dir).unwrap();
        fs::write(&outside_file, "secret").unwrap();

        // File symlink inside the vault pointing at a file outside it.
        let file_link = notes_directory.join("link.thoughttree");
        symlink(&outside_file, &file_link).unwrap();
        // Directory symlink inside the vault pointing at a directory outside it.
        let dir_link = notes_directory.join("linked-dir");
        symlink(&outside_dir, &dir_link).unwrap();
        let through_dir_link = dir_link.join("secret.thoughttree");
        let new_through_dir_link = dir_link.join("planted.thoughttree");

        assert_outside_notes_directory(
            load_project_in_notes_dir(&notes_directory, &file_link).unwrap_err(),
        );
        assert_outside_notes_directory(
            save_project_in_notes_dir(&notes_directory, &file_link, "changed", None).unwrap_err(),
        );
        assert_outside_notes_directory(
            load_project_in_notes_dir(&notes_directory, &through_dir_link).unwrap_err(),
        );
        assert_outside_notes_directory(
            save_project_in_notes_dir(&notes_directory, &through_dir_link, "changed", None)
                .unwrap_err(),
        );
        assert_outside_notes_directory(
            save_project_in_notes_dir(&notes_directory, &new_through_dir_link, "planted", None)
                .unwrap_err(),
        );
        assert_eq!(fs::read_to_string(&outside_file).unwrap(), "secret");
        assert!(!outside_dir.join("planted.thoughttree").exists());
    }

    #[test]
    fn valid_project_inside_notes_directory_saves_and_loads() {
        let dir = tempdir().unwrap();
        let notes_directory = dir.path().join("notes");
        fs::create_dir_all(notes_directory.join("nested")).unwrap();
        let project_path = notes_directory.join("nested").join("project.thoughttree");

        let (saved_path, first_revision) =
            save_project_in_notes_dir(&notes_directory, &project_path, "first", None).unwrap();
        let (loaded_path, loaded) =
            load_project_in_notes_dir(&notes_directory, &project_path).unwrap();

        assert_eq!(saved_path, loaded_path);
        assert_eq!(loaded.content, "first");
        assert_eq!(loaded.revision, first_revision.0);

        let (_, second_revision) = save_project_in_notes_dir(
            &notes_directory,
            &project_path,
            "second",
            Some(&first_revision),
        )
        .unwrap();
        let (_, reloaded) = load_project_in_notes_dir(&notes_directory, &project_path).unwrap();

        assert_ne!(second_revision, first_revision);
        assert_eq!(reloaded.content, "second");
        assert_eq!(reloaded.revision, second_revision.0);
    }

    #[test]
    fn stale_save_reports_current_revision_and_keeps_newer_content() {
        let dir = tempdir().unwrap();
        let notes_directory = dir.path().join("notes");
        fs::create_dir(&notes_directory).unwrap();
        let project_path = notes_directory.join("project.thoughttree");

        let (_, base_revision) =
            save_project_in_notes_dir(&notes_directory, &project_path, "base", None).unwrap();
        let (_, newer_revision) = save_project_in_notes_dir(
            &notes_directory,
            &project_path,
            "newer",
            Some(&base_revision),
        )
        .unwrap();

        let error = save_project_in_notes_dir(
            &notes_directory,
            &project_path,
            "stale",
            Some(&base_revision),
        )
        .unwrap_err();

        assert!(matches!(
            error,
            ProjectCommandError::StaleRevision { current_revision }
                if current_revision == newer_revision.0
        ));
        assert_eq!(fs::read_to_string(&project_path).unwrap(), "newer");
    }
}

#[tauri::command]
pub(crate) async fn get_notes_directory(app: AppHandle) -> Result<Option<String>, String> {
    config::get_notes_directory_optional(&app)
}

#[tauri::command]
pub(crate) async fn set_notes_directory(app: AppHandle, path: String) -> Result<(), String> {
    config::set_notes_directory(&app, &path)?;
    tracing::info!("Notes directory set to: {}", path);
    Ok(())
}

#[tauri::command]
pub(crate) async fn pick_notes_directory(app: AppHandle) -> Result<Option<String>, String> {
    let path = app
        .dialog()
        .file()
        .set_title("Select Notes Directory")
        .blocking_pick_folder();

    Ok(path.map(|p| p.to_string()))
}

#[tauri::command]
pub(crate) async fn save_project(
    app: AppHandle,
    path: String,
    data: String,
    base_revision: Option<String>,
) -> Result<String, ProjectCommandError> {
    let notes_directory = config::get_notes_directory_required(&app).map_err(command_message)?;
    let revision = base_revision
        .as_deref()
        .map(|value| Revision(value.to_string()));
    let (validated_path, next_revision) =
        save_project_in_notes_dir(&notes_directory, Path::new(&path), &data, revision.as_ref())?;

    tracing::info!("Project saved to: {:?}", validated_path);
    Ok(next_revision.0)
}

#[tauri::command]
pub(crate) async fn load_project(
    app: AppHandle,
    path: String,
) -> Result<LoadProjectResponse, ProjectCommandError> {
    let notes_directory = config::get_notes_directory_required(&app).map_err(command_message)?;
    let (validated_path, project) = load_project_in_notes_dir(&notes_directory, Path::new(&path))?;
    tracing::info!("Project loaded from: {:?}", validated_path);
    Ok(project)
}

#[tauri::command]
pub(crate) async fn list_projects(app: AppHandle) -> Result<Vec<ProjectEntry>, String> {
    let notes_directory = config::get_notes_directory_required(&app)?;
    collect_project_entries(&notes_directory)
}

#[tauri::command]
pub(crate) async fn new_project_dialog(app: AppHandle) -> Result<Option<String>, String> {
    let default_dir = config::get_notes_directory_optional(&app)?.map(PathBuf::from);

    let mut dialog = app
        .dialog()
        .file()
        .set_title("Save New Project")
        .add_filter("ThoughtTree Project", &["thoughttree"])
        .set_file_name("untitled.thoughttree");

    if let Some(dir) = default_dir {
        dialog = dialog.set_directory(dir);
    }

    Ok(dialog.blocking_save_file().map(|p| p.to_string()))
}

#[tauri::command]
pub(crate) async fn open_project_dialog(app: AppHandle) -> Result<Option<String>, String> {
    let default_dir = config::get_notes_directory_optional(&app)?.map(PathBuf::from);

    let mut dialog = app
        .dialog()
        .file()
        .set_title("Open Project")
        .add_filter("ThoughtTree Project", &["thoughttree"]);

    if let Some(dir) = default_dir {
        dialog = dialog.set_directory(dir);
    }

    Ok(dialog.blocking_pick_file().map(|p| p.to_string()))
}

#[tauri::command]
pub(crate) async fn get_recent_projects(app: AppHandle) -> Result<Vec<String>, String> {
    config::get_recent_projects(&app)
}

#[tauri::command]
pub(crate) async fn add_recent_project(app: AppHandle, path: String) -> Result<(), String> {
    let mut recent_projects = config::get_recent_projects(&app)?;

    recent_projects.retain(|project_path| project_path != &path);
    recent_projects.insert(0, path);
    recent_projects.truncate(10);

    config::set_recent_projects(&app, &recent_projects)
}

#[tauri::command]
pub(crate) async fn remove_recent_project(app: AppHandle, path: String) -> Result<(), String> {
    let mut recent_projects = config::get_recent_projects(&app)?;
    recent_projects.retain(|project_path| project_path != &path);

    config::set_recent_projects(&app, &recent_projects)
}

#[tauri::command]
pub(crate) async fn export_markdown(
    app: AppHandle,
    content: String,
    default_name: String,
) -> Result<Option<String>, String> {
    let mut dialog = app
        .dialog()
        .file()
        .set_title("Export as Markdown")
        .add_filter("Markdown", &["md"])
        .set_file_name(&default_name);

    if let Some(dir) = config::get_notes_directory_optional(&app)?.map(PathBuf::from) {
        dialog = dialog.set_directory(dir);
    }

    if let Some(path) = dialog.blocking_save_file() {
        let path_str = path.to_string();
        std::fs::write(&path_str, &content)
            .map_err(|e| format!("Failed to export markdown: {e}"))?;
        tracing::info!("Exported markdown to: {}", path_str);
        Ok(Some(path_str))
    } else {
        Ok(None)
    }
}

#[tauri::command]
pub(crate) async fn search_files(
    app: AppHandle,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<String>, String> {
    let notes_directory = config::get_notes_directory_required(&app)?;
    let max_results = limit.unwrap_or(20);

    let query = query.chars().take(100).collect::<String>();
    let query_lower = query.to_lowercase();

    let mut files = Vec::new();

    for entry in WalkDir::new(&notes_directory)
        .follow_links(false)
        .max_depth(20)
        .into_iter()
        .filter_map(|entry| entry.ok())
    {
        if !entry.file_type().is_file() {
            continue;
        }

        let rel_path = match entry.path().strip_prefix(&notes_directory) {
            Ok(path) => path.to_string_lossy().to_string(),
            Err(_) => continue,
        };

        if query.is_empty() || rel_path.to_lowercase().contains(&query_lower) {
            files.push(rel_path);
            if files.len() >= max_results {
                break;
            }
        }
    }

    Ok(files)
}
