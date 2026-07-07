use std::collections::HashMap;
use std::sync::Arc;

use anyhow::Context;
use tokio::sync::{oneshot, Mutex};

use crate::backend::events::{PermissionRequestEvent, SessionEventSink};

#[derive(Clone)]
pub(crate) struct PermissionBroker {
    inner: Arc<Mutex<BrokerInner>>,
}

struct BrokerInner {
    pending: HashMap<String, PendingPermission>,
}

struct PendingPermission {
    // Read by `pending()`; kept for server-side reattach surfacing (thoughttree-73r).
    #[allow(dead_code)]
    request: PermissionRequestEvent,
    responder: oneshot::Sender<String>,
}

#[derive(Debug, thiserror::Error)]
pub(crate) enum RespondError {
    #[error("no pending permission request with id {0}")]
    UnknownRequest(String),
}

impl PermissionBroker {
    pub(crate) fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(BrokerInner {
                pending: HashMap::new(),
            })),
        }
    }

    pub(crate) async fn request(
        &self,
        request: PermissionRequestEvent,
        sink: &impl SessionEventSink,
    ) -> anyhow::Result<String> {
        let request_id = request.request_id.clone();
        let (tx, rx) = oneshot::channel();

        {
            let mut inner = self.inner.lock().await;
            inner.pending.insert(
                request_id.clone(),
                PendingPermission {
                    request: request.clone(),
                    responder: tx,
                },
            );
        }

        sink.permission_request(request.clone());

        if request.delivery_failed() {
            let mut inner = self.inner.lock().await;
            inner.pending.remove(&request_id);
            anyhow::bail!("permission request {request_id} could not be delivered");
        }

        rx.await.with_context(|| {
            format!("permission request {request_id} was dropped before receiving a response")
        })
    }

    pub(crate) async fn respond(
        &self,
        request_id: &str,
        option_id: String,
    ) -> Result<(), RespondError> {
        let pending = {
            let mut inner = self.inner.lock().await;
            inner.pending.remove(request_id)
        }
        .ok_or_else(|| RespondError::UnknownRequest(request_id.to_string()))?;

        let _ = pending.responder.send(option_id);
        Ok(())
    }

    /// Snapshot of parked requests — the server will surface these on client reattach
    /// (thoughttree-73r). Until then only tests call this.
    #[allow(dead_code)]
    pub(crate) async fn pending(&self) -> Vec<PermissionRequestEvent> {
        let inner = self.inner.lock().await;
        inner
            .pending
            .values()
            .map(|pending| pending.request.clone())
            .collect()
    }
}

impl Default for PermissionBroker {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex as StdMutex};

    use tokio::runtime::Builder;
    use tokio::task::LocalSet;

    use super::PermissionBroker;
    use crate::backend::events::{
        PermissionRequestEvent, PermissionRequestOption, SessionEventSink, StreamChunkEvent,
    };

    #[derive(Clone, Default)]
    struct RecordingSink {
        stream_chunks: Arc<StdMutex<Vec<StreamChunkEvent>>>,
        permission_requests: Arc<StdMutex<Vec<PermissionRequestEvent>>>,
    }

    #[derive(Clone, Default)]
    struct FailingSink;

    impl RecordingSink {
        fn permission_requests(&self) -> Vec<PermissionRequestEvent> {
            self.permission_requests.lock().unwrap().clone()
        }
    }

    impl SessionEventSink for RecordingSink {
        fn stream_chunk(&self, event: StreamChunkEvent) {
            self.stream_chunks.lock().unwrap().push(event);
        }

        fn permission_request(&self, event: PermissionRequestEvent) {
            self.permission_requests.lock().unwrap().push(event);
        }
    }

    impl SessionEventSink for FailingSink {
        fn stream_chunk(&self, _event: StreamChunkEvent) {}

        fn permission_request(&self, event: PermissionRequestEvent) {
            event.mark_delivery_failed();
        }
    }

    fn sample_request() -> PermissionRequestEvent {
        PermissionRequestEvent::new(
            "request-1".to_string(),
            "node-1".to_string(),
            "tool-call-1".to_string(),
            "WebFetch".to_string(),
            "No additional details".to_string(),
            vec![
                PermissionRequestOption {
                    id: "allow".to_string(),
                    label: "Allow".to_string(),
                },
                PermissionRequestOption {
                    id: "deny".to_string(),
                    label: "Deny".to_string(),
                },
            ],
        )
    }

    #[test]
    fn permission_request_round_trip_resolves_selected_option() {
        let runtime = Builder::new_current_thread().enable_all().build().unwrap();
        let local = LocalSet::new();

        local.block_on(&runtime, async {
            let broker = PermissionBroker::new();
            let sink = RecordingSink::default();
            let request = sample_request();

            let pending_request = request.clone();
            let broker_for_task = broker.clone();
            let sink_for_task = sink.clone();
            let handle = tokio::task::spawn_local(async move {
                broker_for_task
                    .request(pending_request, &sink_for_task)
                    .await
            });

            tokio::task::yield_now().await;

            assert_eq!(sink.permission_requests(), vec![request.clone()]);
            assert_eq!(broker.pending().await, vec![request.clone()]);

            broker
                .respond(&request.request_id, "allow".to_string())
                .await
                .unwrap();

            let selected = handle.await.unwrap().unwrap();
            assert_eq!(selected, "allow");
            assert!(broker.pending().await.is_empty());
        });
    }

    #[test]
    fn responding_to_unknown_request_returns_error() {
        let runtime = Builder::new_current_thread().enable_all().build().unwrap();

        runtime.block_on(async {
            let broker = PermissionBroker::new();
            let err = broker
                .respond("missing-request", "allow".to_string())
                .await
                .unwrap_err();

            assert_eq!(
                err.to_string(),
                "no pending permission request with id missing-request"
            );
        });
    }

    #[test]
    fn undeliverable_request_returns_error_without_leaving_pending_state() {
        let runtime = Builder::new_current_thread().enable_all().build().unwrap();

        runtime.block_on(async {
            let broker = PermissionBroker::new();
            let err = broker
                .request(sample_request(), &FailingSink)
                .await
                .unwrap_err();

            assert!(err
                .to_string()
                .contains("permission request request-1 could not be delivered"));
            assert!(broker.pending().await.is_empty());
        });
    }
}
