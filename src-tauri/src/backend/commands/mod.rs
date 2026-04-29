pub(crate) mod chat;
pub(crate) mod projects;
pub(crate) mod providers;
pub(crate) mod summary;

pub(crate) use chat::{check_acp_available, respond_to_permission, send_prompt};
pub(crate) use projects::{
    add_recent_project, export_markdown, get_notes_directory, get_recent_projects, load_project,
    new_project_dialog, open_project_dialog, pick_notes_directory, remove_recent_project,
    save_project, search_files, set_notes_directory,
};
pub(crate) use providers::{
    get_available_models, get_available_providers, get_default_provider, get_model_preferences,
    get_provider_paths, pick_provider_executable, set_default_provider, set_model_preference,
    set_provider_path, validate_provider_path,
};
pub(crate) use summary::generate_summary;
