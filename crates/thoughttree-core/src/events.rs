use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};

use crate::acp::provenance::TurnProvenance;

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct StreamChunkEvent {
    #[serde(rename = "node_id")]
    pub node_id: String,
    pub turn_id: String,
    pub chunk: String,
}

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct PermissionRequestOption {
    pub id: String,
    pub label: String,
}

#[derive(Clone, Debug, serde::Serialize)]
pub struct PermissionRequestEvent {
    #[serde(rename = "id")]
    pub request_id: String,
    #[serde(skip_serializing)]
    pub node_id: String,
    #[serde(rename = "tool_type")]
    pub tool_type: String,
    #[serde(rename = "tool_name")]
    pub tool_name: String,
    pub description: String,
    pub options: Vec<PermissionRequestOption>,
    #[serde(skip)]
    delivery_failed: Arc<AtomicBool>,
}

impl PermissionRequestEvent {
    pub fn new(
        request_id: String,
        node_id: String,
        tool_type: String,
        tool_name: String,
        description: String,
        options: Vec<PermissionRequestOption>,
    ) -> Self {
        Self {
            request_id,
            node_id,
            tool_type,
            tool_name,
            description,
            options,
            delivery_failed: Arc::new(AtomicBool::new(false)),
        }
    }

    pub fn mark_delivery_failed(&self) {
        self.delivery_failed.store(true, Ordering::Relaxed);
    }

    pub fn delivery_failed(&self) -> bool {
        self.delivery_failed.load(Ordering::Relaxed)
    }
}

impl PartialEq for PermissionRequestEvent {
    fn eq(&self, other: &Self) -> bool {
        self.request_id == other.request_id
            && self.node_id == other.node_id
            && self.tool_type == other.tool_type
            && self.tool_name == other.tool_name
            && self.description == other.description
            && self.options == other.options
    }
}

impl Eq for PermissionRequestEvent {}

impl Default for PermissionRequestEvent {
    fn default() -> Self {
        Self::new(
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            String::new(),
            Vec::new(),
        )
    }
}

#[derive(Clone, Debug, serde::Serialize, PartialEq, Eq)]
pub struct TurnProvenanceEvent {
    #[serde(rename = "node_id")]
    pub node_id: String,
    pub turn_id: String,
    pub provenance: TurnProvenance,
}

pub trait SessionEventSink: Clone + Send + Sync + 'static {
    fn stream_chunk(&self, event: StreamChunkEvent);
    fn permission_request(&self, event: PermissionRequestEvent);
    /// Emitted once per Turn, after the prompt finishes (successfully or not).
    fn turn_provenance(&self, event: TurnProvenanceEvent);
}

#[cfg(test)]
mod tests {
    use super::{
        PermissionRequestEvent, PermissionRequestOption, StreamChunkEvent, TurnProvenanceEvent,
    };
    use crate::acp::provenance::{ProvenanceCompleteness, TurnProvenance};

    #[test]
    fn turn_provenance_event_serializes_with_snake_case_node_id() {
        let event = TurnProvenanceEvent {
            node_id: "node-1".to_string(),
            turn_id: "turn-1".to_string(),
            provenance: TurnProvenance {
                completeness: ProvenanceCompleteness::Complete,
                references: vec![],
                activity: vec![],
            },
        };

        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            json,
            r#"{"node_id":"node-1","turn_id":"turn-1","provenance":{"completeness":"complete","references":[],"activity":[]}}"#
        );
    }

    #[test]
    fn stream_chunk_event_serializes_turn_identity() {
        let event = StreamChunkEvent {
            node_id: "node-1".to_string(),
            turn_id: "turn-1".to_string(),
            chunk: "hello".to_string(),
        };

        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            json,
            r#"{"node_id":"node-1","turn_id":"turn-1","chunk":"hello"}"#
        );
    }

    #[test]
    fn permission_request_event_serializes_with_existing_wire_shape() {
        let event = PermissionRequestEvent::new(
            "request-1".to_string(),
            "node-1".to_string(),
            "tool-call-1".to_string(),
            "WebFetch".to_string(),
            "No additional details".to_string(),
            vec![PermissionRequestOption {
                id: "allow".to_string(),
                label: "Allow".to_string(),
            }],
        );

        let json = serde_json::to_string(&event).unwrap();
        assert_eq!(
            json,
            r#"{"id":"request-1","tool_type":"tool-call-1","tool_name":"WebFetch","description":"No additional details","options":[{"id":"allow","label":"Allow"}]}"#
        );
    }
}
