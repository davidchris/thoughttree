//! In-process native desktop services. ACP, Vault resolution, guarded writes,
//! recovery, attachment validation and provenance use the same core as Tauri.
//!
//! `Desktop::open` takes an explicit config directory for isolated fixtures.
//! All ACP work runs on an owned Tokio runtime and reaches the UI through an
//! awaitable channel. Filesystem methods are synchronous and can be dispatched
//! to a GPUI background executor when the caller needs to keep rendering.

mod config;
mod events;
mod files;
mod imports;
mod projects;
mod providers;

use std::{path::PathBuf, sync::Arc};

use config::ConfigStore;
use events::NativeEventSink;
use providers::ProviderPathRequests;
use thoughttree_core::{
    acp::sessions::{run_prompt_session, run_summary_session, PromptSessionParams},
    permissions::PermissionBroker,
    runtime::run_localset_blocking,
    turns::ActiveTurns,
    vault::files::PreviewCache,
};

pub use config::Config;
pub use events::DesktopEvent;
pub use files::VaultFileStatus;
pub use imports::{read_kagi_export, ImportError, KAGI_EXPORT_MAX_BYTES};
pub use thoughttree_core::{
    events::{PermissionRequestEvent, StreamChunkEvent, TurnProvenanceEvent},
    types::{
        AgentProvider, EffortPreferences, Message, MessageFile, MessageImage, ModelInfo,
        ModelPreferences, ProviderPaths, ProviderStatus, ReasoningEffort,
    },
    vault::{ProjectDoc, ProjectEntry, RecoveryEntry, Revision, VaultError},
};

#[derive(Clone, serde::Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PromptRequest {
    pub node_id: String,
    pub turn_id: String,
    pub messages: Vec<Message>,
    pub provider: Option<AgentProvider>,
    pub model_id: Option<String>,
    pub effort: Option<ReasoningEffort>,
}

struct Inner {
    config: ConfigStore,
    runtime: Option<tokio::runtime::Runtime>,
    sink: NativeEventSink,
    active_turns: ActiveTurns,
    broker: PermissionBroker,
    preview_cache: PreviewCache,
    provider_path_requests: ProviderPathRequests,
}

impl Drop for Inner {
    fn drop(&mut self) {
        // A window can close on any executor; dropping Tokio's runtime from
        // an async context must not block or panic.
        if let Some(runtime) = self.runtime.take() {
            runtime.shutdown_background();
        }
    }
}

#[derive(Clone)]
pub struct Desktop(Arc<Inner>);

impl Desktop {
    pub fn open(
        config_directory: PathBuf,
    ) -> Result<(Self, async_channel::Receiver<DesktopEvent>), String> {
        let config = ConfigStore::open(config_directory)?;
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .enable_all()
            .thread_name("thoughttree-desktop")
            .worker_threads(2)
            .build()
            .map_err(|e| format!("Cannot start desktop runtime: {e}"))?;
        let (sender, receiver) = async_channel::unbounded();
        Ok((
            Self(Arc::new(Inner {
                config,
                runtime: Some(runtime),
                sink: NativeEventSink(sender),
                active_turns: ActiveTurns::default(),
                broker: PermissionBroker::new(),
                preview_cache: PreviewCache::default(),
                provider_path_requests: ProviderPathRequests::default(),
            })),
            receiver,
        ))
    }

    /// Shares the existing app's portable config format and installation
    /// location. `THOUGHTTREE_CONFIG_DIR` isolates development and screenshots.
    pub fn open_default() -> Result<(Self, async_channel::Receiver<DesktopEvent>), String> {
        let directory = std::env::var_os("THOUGHTTREE_CONFIG_DIR")
            .map(PathBuf::from)
            .or_else(|| dirs::data_dir().map(|p| p.join("com.david.thoughttree")))
            .ok_or_else(|| "Application config directory is unavailable".to_string())?;
        Self::open(directory)
    }

    pub fn config(&self) -> Config {
        self.0.config.get()
    }

    pub fn set_notes_directory(&self, path: PathBuf) -> Result<(), String> {
        if !path.is_absolute() || !path.is_dir() {
            return Err("Select an existing absolute path for the notes directory".into());
        }
        self.0
            .config
            .update(|config| config.notes_directory = Some(path))
    }

    pub fn notes_directory(&self) -> Result<PathBuf, String> {
        self.config()
            .notes_directory
            .ok_or_else(|| "Notes directory not configured. Please set it in settings.".to_string())
    }

    pub fn set_default_provider(&self, provider: AgentProvider) -> Result<(), String> {
        self.0
            .config
            .update(|config| config.default_provider = provider)
    }

    pub fn set_model_preference(
        &self,
        provider: &AgentProvider,
        model_id: Option<String>,
    ) -> Result<(), String> {
        self.0
            .config
            .update(|config| config.model_preferences.set(provider, model_id))
    }

    pub fn set_effort_preference(
        &self,
        provider: &AgentProvider,
        effort: Option<ReasoningEffort>,
    ) -> Result<(), String> {
        self.0
            .config
            .update(|config| config.effort_preferences.set(provider, effort))
    }

    /// Reserve the GraphNode synchronously so duplicate submissions cannot
    /// race the background executor. Turn ownership remains in core.
    pub fn start_prompt(&self, request: PromptRequest) -> Result<(), String> {
        let notes_directory = self.notes_directory()?;
        let config = self.config();
        let turn = self
            .0
            .active_turns
            .start(request.node_id.clone(), request.turn_id.clone())?;
        let sink = self.0.sink.clone();
        let broker = self.0.broker.clone();
        self.runtime().spawn(async move {
            let node_id = request.node_id.clone();
            let turn_id = request.turn_id.clone();
            let completion_sink = sink.clone();
            let result = run_localset_blocking(move || async move {
                let _turn = turn;
                let provider = request.provider.unwrap_or(config.default_provider);
                run_prompt_session(PromptSessionParams {
                    sink,
                    node_id: request.node_id,
                    turn_id: request.turn_id,
                    messages: request.messages,
                    broker,
                    notes_directory,
                    model_id: request
                        .model_id
                        .or_else(|| config.model_preferences.get(&provider).cloned()),
                    effort: request
                        .effort
                        .or_else(|| config.effort_preferences.get(&provider).copied()),
                    provider,
                    provider_paths: config.provider_paths,
                })
                .await
                .map_err(|e| e.to_string())
            })
            .await;
            completion_sink.emit(DesktopEvent::PromptFinished {
                node_id,
                turn_id,
                result,
            });
        });
        Ok(())
    }

    pub fn respond_to_permission(&self, request_id: String, option_id: String) {
        let broker = self.0.broker.clone();
        let sink = self.0.sink.clone();
        self.runtime().spawn(async move {
            if let Err(error) = broker.respond(&request_id, option_id).await {
                sink.emit(DesktopEvent::PermissionResponseFailed {
                    request_id,
                    error: error.to_string(),
                });
            }
        });
    }

    pub fn generate_summary(&self, node_id: String, content: String) -> Result<(), String> {
        let notes_directory = self.notes_directory()?;
        let config = self.config();
        let sink = self.0.sink.clone();
        self.runtime().spawn(async move {
            let result = run_localset_blocking(move || async move {
                run_summary_session(
                    content,
                    notes_directory,
                    config.default_provider,
                    config.provider_paths,
                )
                .await
                .map_err(|e| e.to_string())
            })
            .await;
            sink.emit(DesktopEvent::SummaryFinished { node_id, result });
        });
        Ok(())
    }

    fn runtime(&self) -> &tokio::runtime::Runtime {
        self.0
            .runtime
            .as_ref()
            .expect("Runtime exists until Desktop drops")
    }
}

#[cfg(test)]
mod tests;
