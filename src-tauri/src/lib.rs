mod backend;

use backend::commands::{
    add_recent_project, check_acp_available, export_markdown, generate_summary,
    get_available_models, get_available_providers, get_default_provider, get_model_preferences,
    get_notes_directory, get_provider_paths, get_recent_projects, load_project, new_project_dialog,
    open_project_dialog, pick_notes_directory, pick_provider_executable, remove_recent_project,
    respond_to_permission, save_project, search_files, send_prompt, set_default_provider,
    set_model_preference, set_notes_directory, set_provider_path, validate_provider_path,
};
use backend::state::AppState;

pub fn run() {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_default_env()
                .add_directive("info".parse().unwrap()),
        )
        .init();

    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_store::Builder::default().build())
        .plugin(tauri_plugin_shell::init())
        .manage(AppState::default())
        .invoke_handler(tauri::generate_handler![
            send_prompt,
            respond_to_permission,
            check_acp_available,
            get_available_providers,
            get_default_provider,
            set_default_provider,
            get_model_preferences,
            set_model_preference,
            get_available_models,
            get_provider_paths,
            set_provider_path,
            validate_provider_path,
            pick_provider_executable,
            get_notes_directory,
            set_notes_directory,
            pick_notes_directory,
            save_project,
            load_project,
            new_project_dialog,
            open_project_dialog,
            export_markdown,
            get_recent_projects,
            add_recent_project,
            remove_recent_project,
            search_files,
            generate_summary,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
