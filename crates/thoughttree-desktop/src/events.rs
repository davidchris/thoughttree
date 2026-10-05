use thoughttree_core::{
    events::{PermissionRequestEvent, SessionEventSink, StreamChunkEvent, TurnProvenanceEvent},
    types::{AgentProvider, ModelInfo},
};

#[derive(Clone, Debug)]
pub enum DesktopEvent {
    StreamChunk(StreamChunkEvent),
    PermissionRequest(PermissionRequestEvent),
    TurnProvenance(TurnProvenanceEvent),
    PromptFinished {
        node_id: String,
        turn_id: String,
        result: Result<String, String>,
    },
    SummaryFinished {
        node_id: String,
        result: Result<String, String>,
    },
    ModelsDiscovered {
        provider: AgentProvider,
        /// The configured executable the discovery ran against.
        provider_path: Option<String>,
        result: Result<Vec<ModelInfo>, String>,
    },
    /// Every `set_provider_path` request reports exactly once. Only the
    /// latest request for the provider is `current`; an older one may still
    /// have been saved if it committed before the newer request began.
    ProviderPathValidated {
        provider: AgentProvider,
        result: Result<String, String>,
        current: bool,
    },
    PermissionResponseFailed {
        request_id: String,
        error: String,
    },
}

#[derive(Clone)]
pub(crate) struct NativeEventSink(pub async_channel::Sender<DesktopEvent>);

impl NativeEventSink {
    pub(crate) fn emit(&self, event: DesktopEvent) {
        // The channel is unbounded: native rendering never blocks the ACP
        // dispatch loop, and chunks retain their exact ordering.
        let _ = self.0.try_send(event);
    }
}

impl SessionEventSink for NativeEventSink {
    fn stream_chunk(&self, event: StreamChunkEvent) {
        self.emit(DesktopEvent::StreamChunk(event));
    }

    fn permission_request(&self, event: PermissionRequestEvent) {
        if self
            .0
            .try_send(DesktopEvent::PermissionRequest(event.clone()))
            .is_err()
        {
            event.mark_delivery_failed();
        }
    }

    fn turn_provenance(&self, event: TurnProvenanceEvent) {
        self.emit(DesktopEvent::TurnProvenance(event));
    }
}
