use std::path::PathBuf;

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
    _app: AppHandle,
    path: String,
    data: String,
    base_revision: Option<String>,
) -> Result<String, ProjectCommandError> {
    let revision = base_revision
        .as_deref()
        .map(|value| Revision(value.to_string()));
    let next_revision =
        guarded_write_file(&path, &data, revision.as_ref()).map_err(map_save_error)?;

    tracing::info!("Project saved to: {}", path);
    Ok(next_revision.0)
}

#[tauri::command]
pub(crate) async fn load_project(
    _app: AppHandle,
    path: String,
) -> Result<LoadProjectResponse, ProjectCommandError> {
    let project = read_project_file(&path).map_err(map_load_error)?;
    tracing::info!("Project loaded from: {}", path);
    Ok(LoadProjectResponse {
        content: project.content,
        revision: project.revision.0,
    })
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
