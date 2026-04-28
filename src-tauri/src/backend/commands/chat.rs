use tauri::{AppHandle, State};

use crate::backend::acp::process::find_sidecar_path;
use crate::backend::acp::sessions::run_prompt_session;
use crate::backend::config;
use crate::backend::runtime::run_localset_blocking;
use crate::backend::state::AppState;
use crate::backend::types::{AgentProvider, Message};

#[tauri::command]
pub async fn send_prompt(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    node_id: String,
    messages: Vec<Message>,
    provider: Option<AgentProvider>,
    model_id: Option<String>,
) -> Result<String, String> {
    let pending_permissions = state.pending_permissions.clone();

    let notes_directory = config::get_notes_directory_required(&app_handle)?;
    let default_provider = config::get_default_provider(&app_handle)?;
    let provider_paths = config::get_provider_paths(&app_handle)?;

    let active_provider = provider.unwrap_or(default_provider);

    tracing::info!(
        "Using provider: {:?}, notes directory: {:?}",
        active_provider,
        notes_directory
    );

    run_localset_blocking(move || async move {
        run_prompt_session(
            app_handle,
            node_id,
            messages,
            pending_permissions,
            notes_directory,
            active_provider,
            model_id,
            provider_paths,
        )
        .await
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub async fn respond_to_permission(
    state: State<'_, AppState>,
    request_id: String,
    option_id: String,
) -> Result<(), String> {
    let mut pending = state.pending_permissions.lock().await;

    if let Some(sender) = pending.remove(&request_id) {
        sender
            .send(option_id)
            .map_err(|_| "Failed to send permission response")?;
        Ok(())
    } else {
        Err(format!(
            "No pending permission request with ID: {}",
            request_id
        ))
    }
}

#[tauri::command]
pub async fn check_acp_available() -> Result<bool, String> {
    Ok(find_sidecar_path().is_some())
}
