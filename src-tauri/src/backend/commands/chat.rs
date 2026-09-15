use tauri::{AppHandle, State};
use thoughttree_core::acp::process::find_sidecar_path;
use thoughttree_core::acp::sessions::{run_prompt_session, PromptSessionParams};
use thoughttree_core::runtime::run_localset_blocking;
use thoughttree_core::types::{AgentProvider, Message, ReasoningEffort};

use crate::backend::config;
use crate::backend::events::TauriEventSink;
use crate::backend::state::AppState;

#[derive(serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct PromptRequest {
    node_id: String,
    turn_id: String,
    messages: Vec<Message>,
    provider: Option<AgentProvider>,
    model_id: Option<String>,
    effort: Option<ReasoningEffort>,
}

#[tauri::command]
pub(crate) async fn send_prompt(
    app_handle: AppHandle,
    state: State<'_, AppState>,
    request: PromptRequest,
) -> Result<String, String> {
    let PromptRequest {
        node_id,
        turn_id,
        messages,
        provider,
        model_id,
        effort,
    } = request;
    let turn = state.active_turns.start(node_id.clone(), turn_id.clone())?;
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
        // The blocking task outlives its IPC caller after a frontend reload.
        let _turn = turn;
        let result = run_prompt_session(PromptSessionParams {
            sink,
            node_id: node_id.clone(),
            turn_id: turn_id.clone(),
            messages,
            broker,
            notes_directory,
            provider: active_provider,
            model_id,
            effort,
            provider_paths,
        })
        .await
        .map_err(|e| e.to_string());
        match &result {
            Ok(_) => tracing::info!(%node_id, %turn_id, "Turn completed"),
            Err(error) => tracing::error!(%node_id, %turn_id, %error, "Turn failed"),
        }
        result
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

#[cfg(test)]
mod tests {
    use super::PromptRequest;

    #[test]
    fn prompt_request_requires_turn_identity_in_the_frontend_wire_shape() {
        let mut payload = serde_json::json!({
            "nodeId": "node-1",
            "turnId": "turn-1",
            "messages": [{"role": "user", "content": "Hello", "images": null, "files": null}],
            "provider": "codex",
            "modelId": "example-model",
            "effort": "high"
        });
        let request: PromptRequest = serde_json::from_value(payload.clone()).unwrap();
        assert_eq!(request.node_id, "node-1");
        assert_eq!(request.turn_id, "turn-1");
        assert_eq!(request.model_id.as_deref(), Some("example-model"));
        payload.as_object_mut().unwrap().remove("turnId");
        assert!(serde_json::from_value::<PromptRequest>(payload).is_err());
    }
}
