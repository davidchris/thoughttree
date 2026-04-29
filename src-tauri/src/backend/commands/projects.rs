use std::path::{Path, PathBuf};

use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use walkdir::WalkDir;

use crate::backend::config;

fn validate_path_in_notes_dir(path: &Path, notes_dir: &Path) -> Result<PathBuf, String> {
    let canonical_notes = std::fs::canonicalize(notes_dir)
        .map_err(|e| format!("Failed to resolve notes directory: {}", e))?;

    let canonical_path = if path.exists() {
        std::fs::canonicalize(path).map_err(|e| format!("Failed to resolve path: {}", e))?
    } else {
        let parent = path
            .parent()
            .ok_or_else(|| "Invalid path: no parent directory".to_string())?;
        let filename = path
            .file_name()
            .ok_or_else(|| "Invalid path: no filename".to_string())?;
        let canonical_parent = std::fs::canonicalize(parent)
            .map_err(|e| format!("Failed to resolve parent directory: {}", e))?;
        canonical_parent.join(filename)
    };

    if !canonical_path.starts_with(&canonical_notes) {
        return Err("Security error: path is outside the notes directory".to_string());
    }

    Ok(canonical_path)
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
pub(crate) async fn save_project(app: AppHandle, path: String, data: String) -> Result<(), String> {
    let notes_directory = config::get_notes_directory_required(&app)?;
    let validated_path = validate_path_in_notes_dir(Path::new(&path), &notes_directory)?;

    std::fs::write(&validated_path, &data).map_err(|e| format!("Failed to save project: {}", e))?;
    tracing::info!("Project saved to: {:?}", validated_path);
    Ok(())
}

#[tauri::command]
pub(crate) async fn load_project(app: AppHandle, path: String) -> Result<String, String> {
    let notes_directory = config::get_notes_directory_required(&app)?;
    let validated_path = validate_path_in_notes_dir(Path::new(&path), &notes_directory)?;

    let data = std::fs::read_to_string(&validated_path)
        .map_err(|e| format!("Failed to load project: {}", e))?;
    tracing::info!("Project loaded from: {:?}", validated_path);
    Ok(data)
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
            .map_err(|e| format!("Failed to export markdown: {}", e))?;
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
