use tauri::AppHandle;

use crate::backend::acp::sessions::run_summary_session;
use crate::backend::config;
use crate::backend::runtime::run_localset_blocking;
use crate::backend::types::SummaryResult;

#[tauri::command]
pub async fn generate_summary(
    app: AppHandle,
    node_id: String,
    content: String,
) -> Result<SummaryResult, String> {
    let notes_directory = config::get_notes_directory_required(&app)?;
    let provider_paths = config::get_provider_paths(&app)?;
    let custom_path = provider_paths.claude_code;

    tracing::info!("Generating summary for node: {}", node_id);

    let result = run_localset_blocking(move || async move {
        run_summary_session(content, notes_directory, custom_path)
            .await
            .map_err(|e| e.to_string())
    })
    .await;

    match result {
        Ok(summary) => {
            tracing::info!("Generated summary for {}: {}", node_id, summary);
            Ok(SummaryResult { node_id, summary })
        }
        Err(error_message) => {
            tracing::warn!(
                "Summary generation failed for {}: {}",
                node_id,
                error_message
            );
            Err(error_message)
        }
    }
}
