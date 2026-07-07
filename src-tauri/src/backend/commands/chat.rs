use tauri::{AppHandle, State};

use crate::backend::acp::process::find_sidecar_path;
use crate::backend::acp::sessions::{run_prompt_session, PromptSessionParams};
use crate::backend::config;
use crate::backend::events::TauriEventSink;
use crate::backend::runtime::run_localset_blocking;
use crate::backend::state::AppState;
use crate::backend::types::{AgentProvider, Message, ReasoningEffort};

#[tauri::command]
pub(crate) async fn send_prompt(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    node_id: String,
    messages: Vec<Message>,
    provider: Option<AgentProvider>,
    model_id: Option<String>,
    effort: Option<ReasoningEffort>,
) -> Result<String, String> {
    let sink = TauriEventSink::new(app_handle.clone());
    let broker = state.broker.clone();

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
        run_prompt_session(PromptSessionParams {
            sink,
            node_id,
            messages,
            broker,
            notes_directory,
            provider: active_provider,
            model_id,
            effort,
            provider_paths,
        })
        .await
        .map_err(|e| e.to_string())
    })
    .await
}

#[tauri::command]
pub(crate) async fn respond_to_permission(
    state: State<'_, AppState>,
    request_id: String,
    option_id: String,
) -> Result<(), String> {
    state
        .broker
        .respond(&request_id, option_id)
        .await
        .map_err(|err| err.to_string())
}

#[tauri::command]
pub(crate) async fn check_acp_available() -> Result<bool, String> {
    Ok(find_sidecar_path().is_some())
}
