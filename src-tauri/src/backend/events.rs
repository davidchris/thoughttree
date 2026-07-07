use tauri::{AppHandle, Emitter};
use thoughttree_core::events::{PermissionRequestEvent, SessionEventSink, StreamChunkEvent};
use tracing::error;

#[derive(Clone)]
pub(crate) struct TauriEventSink {
    app_handle: AppHandle,
}

impl TauriEventSink {
    pub(crate) fn new(app_handle: AppHandle) -> Self {
        Self { app_handle }
    }
}

impl SessionEventSink for TauriEventSink {
    fn stream_chunk(&self, event: StreamChunkEvent) {
        if let Err(err) = self.app_handle.emit("stream-chunk", event) {
            error!("Failed to emit stream chunk: {:?}", err);
        }
    }

    fn permission_request(&self, event: PermissionRequestEvent) {
        if let Err(err) = self.app_handle.emit("permission-request", event.clone()) {
            event.mark_delivery_failed();
            error!("Failed to emit permission request: {:?}", err);
        }
    }
}
