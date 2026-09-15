//! Turn provenance capture for live ACP turns.
//!
//! [`TurnRecorder`] observes every `session/update` notification of one Turn
//! and, when the Turn closes, produces the [`TurnProvenance`] that the
//! frontend attaches to the assistant GraphNode. The recorder is pure until
//! [`TurnRecorder::close`], which touches the filesystem only to decide
//! whether a referenced path lies inside the Vault.
//!
//! Rules (see CONTEXT.md, "Tool activity"):
//! - Tool activity is ordered by first appearance; lifecycle updates refine
//!   the same item and terminal state never regresses.
//! - Activity still pending or in progress when the Turn closes becomes
//!   `incomplete`.
//! - File references are derived only from completed tool calls, deduplicated
//!   by path with merged relations.
//! - Raw tool input and tool output are never retained.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use agent_client_protocol::schema::v1::{
    SessionUpdate, ToolCall, ToolCallId, ToolCallStatus, ToolCallUpdate, ToolKind,
};
use serde::Serialize;

use crate::vault::files::relativize_vault_path;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvenanceCompleteness {
    Complete,
    Partial,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnReferenceRelation {
    Read,
    Updated,
    Deleted,
    Moved,
    Searched,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TurnReference {
    File(FileTurnReference),
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "scope", rename_all = "camelCase")]
pub enum FileTurnReference {
    #[serde(rename_all = "camelCase")]
    Vault {
        path: String,
        display_name: String,
        relations: Vec<TurnReferenceRelation>,
        timestamp: u64,
    },
    #[serde(rename_all = "camelCase")]
    External {
        display_name: String,
        relations: Vec<TurnReferenceRelation>,
        timestamp: u64,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolActivityKind {
    Read,
    Edit,
    Delete,
    Move,
    Search,
    Execute,
    Fetch,
    Delegate,
    Other,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolActivityStatus {
    Pending,
    Completed,
    Failed,
    Incomplete,
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TurnActivity {
    #[serde(rename_all = "camelCase")]
    Tool {
        kind: ToolActivityKind,
        title: String,
        status: ToolActivityStatus,
        timestamp: u64,
        #[serde(skip_serializing_if = "Option::is_none")]
        completed_at: Option<u64>,
    },
    #[serde(rename_all = "camelCase")]
    Unknown {
        provider_type: String,
        label: String,
        timestamp: u64,
    },
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize)]
pub struct TurnProvenance {
    pub completeness: ProvenanceCompleteness,
    pub references: Vec<TurnReference>,
    pub activity: Vec<TurnActivity>,
}

/// Wall-clock source, injectable so tests get deterministic timestamps.
pub trait Clock: Send + Sync {
    fn now_ms(&self) -> u64;
}

#[derive(Clone, Copy, Debug, Default)]
pub struct SystemClock;

impl Clock for SystemClock {
    fn now_ms(&self) -> u64 {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }
}

/// One logical tool invocation, refined across its lifecycle updates.
#[derive(Debug)]
struct ToolRecord {
    kind: ToolKind,
    title: String,
    status: ToolCallStatus,
    locations: Vec<PathBuf>,
    started_at: u64,
    completed_at: Option<u64>,
}

impl ToolRecord {
    fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            ToolCallStatus::Completed | ToolCallStatus::Failed
        )
    }
}

#[derive(Debug)]
enum Entry {
    Tool(ToolCallId),
    Unknown {
        provider_type: String,
        timestamp: u64,
    },
}

pub struct TurnRecorder {
    clock: Box<dyn Clock>,
    order: Vec<Entry>,
    tools: HashMap<ToolCallId, ToolRecord>,
}

impl Default for TurnRecorder {
    fn default() -> Self {
        Self::new()
    }
}

impl TurnRecorder {
    pub fn new() -> Self {
        Self::with_clock(Box::new(SystemClock))
    }

    pub fn with_clock(clock: Box<dyn Clock>) -> Self {
        Self {
            clock,
            order: Vec::new(),
            tools: HashMap::new(),
        }
    }

    /// Record one session update. Message text, thoughts, plans, and
    /// bookkeeping updates are not Turn activity and are ignored.
    pub fn observe(&mut self, update: &SessionUpdate) {
        match update {
            SessionUpdate::ToolCall(call) => self.observe_tool_call(call),
            SessionUpdate::ToolCallUpdate(update) => self.observe_tool_update(update),
            SessionUpdate::AgentMessageChunk(_)
            | SessionUpdate::AgentThoughtChunk(_)
            | SessionUpdate::UserMessageChunk(_)
            | SessionUpdate::Plan(_)
            | SessionUpdate::PlanUpdate(_)
            | SessionUpdate::PlanRemoved(_)
            | SessionUpdate::AvailableCommandsUpdate(_)
            | SessionUpdate::CurrentModeUpdate(_)
            | SessionUpdate::ConfigOptionUpdate(_)
            | SessionUpdate::SessionInfoUpdate(_)
            | SessionUpdate::UsageUpdate(_) => {}
            other => self.observe_unknown(other),
        }
    }

    fn observe_tool_call(&mut self, call: &ToolCall) {
        let now = self.clock.now_ms();
        let locations = paths_of(Some(call.locations.as_slice()));
        match self.tools.get_mut(&call.tool_call_id) {
            Some(record) => {
                record.title = call.title.clone();
                record.kind = call.kind;
                if !locations.is_empty() {
                    record.locations = locations;
                }
                apply_status(record, Some(call.status), now);
            }
            None => {
                let mut record = ToolRecord {
                    kind: call.kind,
                    title: call.title.clone(),
                    status: ToolCallStatus::Pending,
                    locations,
                    started_at: now,
                    completed_at: None,
                };
                apply_status(&mut record, Some(call.status), now);
                self.tools.insert(call.tool_call_id.clone(), record);
                self.order.push(Entry::Tool(call.tool_call_id.clone()));
            }
        }
    }

    fn observe_tool_update(&mut self, update: &ToolCallUpdate) {
        let now = self.clock.now_ms();
        let fields = &update.fields;
        let record = match self.tools.get_mut(&update.tool_call_id) {
            Some(record) => record,
            None => {
                // Update before its ToolCall: still a real invocation, so it
                // is recorded at this point in the order.
                self.tools.insert(
                    update.tool_call_id.clone(),
                    ToolRecord {
                        kind: ToolKind::Other,
                        title: String::new(),
                        status: ToolCallStatus::Pending,
                        locations: Vec::new(),
                        started_at: now,
                        completed_at: None,
                    },
                );
                self.order.push(Entry::Tool(update.tool_call_id.clone()));
                self.tools
                    .get_mut(&update.tool_call_id)
                    .expect("record was just inserted")
            }
        };
        if let Some(title) = &fields.title {
            record.title = title.clone();
        }
        if let Some(kind) = fields.kind {
            record.kind = kind;
        }
        let locations = paths_of(fields.locations.as_deref());
        if !locations.is_empty() {
            record.locations = locations;
        }
        apply_status(record, fields.status, now);
    }

    fn observe_unknown(&mut self, update: &SessionUpdate) {
        let provider_type = variant_name(update);
        self.order.push(Entry::Unknown {
            provider_type,
            timestamp: self.clock.now_ms(),
        });
    }

    /// Close the Turn and build its provenance. `vault_root` decides which
    /// referenced paths are Vault files; everything else keeps a basename.
    pub fn close(self, vault_root: &Path) -> TurnProvenance {
        let mut references: Vec<(String, FileTurnReference)> = Vec::new();
        let mut activity = Vec::with_capacity(self.order.len());
        let mut has_unknown = false;

        for entry in &self.order {
            match entry {
                Entry::Tool(id) => {
                    let record = &self.tools[id];
                    activity.push(tool_activity(record));
                    if record.status == ToolCallStatus::Completed {
                        collect_references(&mut references, record, vault_root);
                    }
                }
                Entry::Unknown {
                    provider_type,
                    timestamp,
                } => {
                    has_unknown = true;
                    activity.push(TurnActivity::Unknown {
                        provider_type: provider_type.clone(),
                        label: "Unrecognized session update".to_string(),
                        timestamp: *timestamp,
                    });
                }
            }
        }

        TurnProvenance {
            completeness: if has_unknown {
                ProvenanceCompleteness::Partial
            } else {
                ProvenanceCompleteness::Complete
            },
            references: references
                .into_iter()
                .map(|(_, reference)| TurnReference::File(reference))
                .collect(),
            activity,
        }
    }
}

fn paths_of(
    locations: Option<&[agent_client_protocol::schema::v1::ToolCallLocation]>,
) -> Vec<PathBuf> {
    locations
        .map(|locations| locations.iter().map(|loc| loc.path.clone()).collect())
        .unwrap_or_default()
}

/// Terminal state never regresses: once completed or failed, later updates
/// cannot move the record back to pending or in progress.
fn apply_status(record: &mut ToolRecord, status: Option<ToolCallStatus>, now: u64) {
    let Some(status) = status else { return };
    if record.is_terminal() {
        return;
    }
    record.status = status;
    if record.is_terminal() {
        record.completed_at = Some(now);
    }
}

fn tool_activity(record: &ToolRecord) -> TurnActivity {
    TurnActivity::Tool {
        kind: activity_kind(record.kind),
        title: record.title.clone(),
        status: match record.status {
            ToolCallStatus::Completed => ToolActivityStatus::Completed,
            ToolCallStatus::Failed => ToolActivityStatus::Failed,
            _ => ToolActivityStatus::Incomplete,
        },
        timestamp: record.started_at,
        completed_at: record.completed_at,
    }
}

fn activity_kind(kind: ToolKind) -> ToolActivityKind {
    match kind {
        ToolKind::Read => ToolActivityKind::Read,
        ToolKind::Edit => ToolActivityKind::Edit,
        ToolKind::Delete => ToolActivityKind::Delete,
        ToolKind::Move => ToolActivityKind::Move,
        ToolKind::Search => ToolActivityKind::Search,
        ToolKind::Execute => ToolActivityKind::Execute,
        ToolKind::Fetch => ToolActivityKind::Fetch,
        ToolKind::Think | ToolKind::SwitchMode | ToolKind::Other => ToolActivityKind::Other,
        _ => ToolActivityKind::Other,
    }
}

/// Which relation a completed tool of this kind establishes with the files it
/// touched. Tools whose kind says nothing about files establish none.
fn relation_for(kind: ToolKind) -> Option<TurnReferenceRelation> {
    match kind {
        ToolKind::Read => Some(TurnReferenceRelation::Read),
        ToolKind::Edit => Some(TurnReferenceRelation::Updated),
        ToolKind::Delete => Some(TurnReferenceRelation::Deleted),
        ToolKind::Move => Some(TurnReferenceRelation::Moved),
        ToolKind::Search => Some(TurnReferenceRelation::Searched),
        ToolKind::Execute
        | ToolKind::Fetch
        | ToolKind::Think
        | ToolKind::SwitchMode
        | ToolKind::Other => None,
        _ => None,
    }
}

fn collect_references(
    references: &mut Vec<(String, FileTurnReference)>,
    record: &ToolRecord,
    vault_root: &Path,
) {
    let Some(relation) = relation_for(record.kind) else {
        return;
    };
    for path in &record.locations {
        let (key, reference) = classify(path, vault_root, relation, record.started_at);
        match references.iter_mut().find(|(existing, _)| *existing == key) {
            Some((_, existing)) => existing.add_relation(relation),
            None => references.push((key, reference)),
        }
    }
}

impl FileTurnReference {
    fn add_relation(&mut self, relation: TurnReferenceRelation) {
        let relations = match self {
            FileTurnReference::Vault { relations, .. }
            | FileTurnReference::External { relations, .. } => relations,
        };
        if !relations.contains(&relation) {
            relations.push(relation);
        }
    }
}

/// Dedup key plus reference. Vault files key on their Vault-relative path;
/// external files key on the display name they will be shown as, since
/// the absolute path is never retained.
fn classify(
    path: &Path,
    vault_root: &Path,
    relation: TurnReferenceRelation,
    timestamp: u64,
) -> (String, FileTurnReference) {
    let display_name = path
        .file_name()
        .map(|name| name.to_string_lossy().into_owned())
        .unwrap_or_else(|| "file".to_string());
    match relativize_vault_path(vault_root, path) {
        Ok(relative) => (
            format!("vault:{relative}"),
            FileTurnReference::Vault {
                path: relative,
                display_name,
                relations: vec![relation],
                timestamp,
            },
        ),
        Err(_) => (
            format!("external:{display_name}"),
            FileTurnReference::External {
                display_name,
                relations: vec![relation],
                timestamp,
            },
        ),
    }
}

/// Variant name of an unrecognized update, taken from its Debug form so no
/// payload is retained.
fn variant_name(update: &SessionUpdate) -> String {
    let debug = format!("{update:?}");
    debug
        .split(|c: char| !c.is_alphanumeric())
        .next()
        .filter(|name| !name.is_empty())
        .unwrap_or("Unknown")
        .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::schema::v1::{
        ContentBlock, ContentChunk, TextContent, ToolCallLocation, ToolCallUpdateFields,
    };
    use std::sync::atomic::{AtomicU64, Ordering};
    use std::sync::Arc;

    struct TickingClock(Arc<AtomicU64>);

    impl Clock for TickingClock {
        fn now_ms(&self) -> u64 {
            self.0.fetch_add(1, Ordering::Relaxed) + 1
        }
    }

    fn recorder() -> TurnRecorder {
        TurnRecorder::with_clock(Box::new(TickingClock(Arc::new(AtomicU64::new(0)))))
    }

    fn vault() -> tempfile::TempDir {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("notes.md"), "hi").unwrap();
        std::fs::create_dir(dir.path().join("sub")).unwrap();
        std::fs::write(dir.path().join("sub").join("deep.md"), "hi").unwrap();
        dir
    }

    fn call(id: &str, title: &str, kind: ToolKind, paths: &[PathBuf]) -> SessionUpdate {
        SessionUpdate::ToolCall(
            ToolCall::new(ToolCallId::new(id), title)
                .kind(kind)
                .locations(paths.iter().map(ToolCallLocation::new).collect()),
        )
    }

    fn status(id: &str, status: ToolCallStatus) -> SessionUpdate {
        SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            ToolCallId::new(id),
            ToolCallUpdateFields::new().status(status),
        ))
    }

    fn tool_activities(
        provenance: &TurnProvenance,
    ) -> Vec<(ToolActivityKind, ToolActivityStatus, String)> {
        provenance
            .activity
            .iter()
            .filter_map(|activity| match activity {
                TurnActivity::Tool {
                    kind,
                    status,
                    title,
                    ..
                } => Some((*kind, *status, title.clone())),
                TurnActivity::Unknown { .. } => None,
            })
            .collect()
    }

    #[test]
    fn empty_turn_is_complete_with_nothing_recorded() {
        let dir = vault();
        let provenance = recorder().close(dir.path());
        assert_eq!(provenance.completeness, ProvenanceCompleteness::Complete);
        assert!(provenance.references.is_empty());
        assert!(provenance.activity.is_empty());
    }

    #[test]
    fn completed_read_of_vault_file_becomes_vault_reference() {
        let dir = vault();
        let mut rec = recorder();
        rec.observe(&call(
            "tc-1",
            "Read notes.md",
            ToolKind::Read,
            &[dir.path().join("notes.md")],
        ));
        rec.observe(&status("tc-1", ToolCallStatus::Completed));

        let provenance = rec.close(dir.path());

        assert_eq!(
            provenance.references,
            vec![TurnReference::File(FileTurnReference::Vault {
                path: "notes.md".into(),
                display_name: "notes.md".into(),
                relations: vec![TurnReferenceRelation::Read],
                timestamp: 1,
            })]
        );
        assert_eq!(
            tool_activities(&provenance),
            vec![(
                ToolActivityKind::Read,
                ToolActivityStatus::Completed,
                "Read notes.md".to_string()
            )]
        );
        assert_eq!(provenance.completeness, ProvenanceCompleteness::Complete);
    }

    #[test]
    fn file_outside_vault_keeps_only_basename() {
        let dir = vault();
        let outside = tempfile::tempdir().unwrap();
        let secret = outside.path().join("secret.txt");
        std::fs::write(&secret, "x").unwrap();
        let mut rec = recorder();
        rec.observe(&call(
            "tc-1",
            "Read",
            ToolKind::Read,
            std::slice::from_ref(&secret),
        ));
        rec.observe(&status("tc-1", ToolCallStatus::Completed));

        let provenance = rec.close(dir.path());

        assert_eq!(
            provenance.references,
            vec![TurnReference::File(FileTurnReference::External {
                display_name: "secret.txt".into(),
                relations: vec![TurnReferenceRelation::Read],
                timestamp: 1,
            })]
        );
        let json = serde_json::to_string(&provenance).unwrap();
        assert!(!json.contains(outside.path().to_str().unwrap()));
    }

    #[test]
    fn same_file_touched_twice_merges_relations() {
        let dir = vault();
        let path = dir.path().join("sub").join("deep.md");
        let mut rec = recorder();
        rec.observe(&call(
            "tc-1",
            "Read",
            ToolKind::Read,
            std::slice::from_ref(&path),
        ));
        rec.observe(&status("tc-1", ToolCallStatus::Completed));
        rec.observe(&call(
            "tc-2",
            "Edit",
            ToolKind::Edit,
            std::slice::from_ref(&path),
        ));
        rec.observe(&status("tc-2", ToolCallStatus::Completed));
        rec.observe(&call("tc-3", "Read again", ToolKind::Read, &[path]));
        rec.observe(&status("tc-3", ToolCallStatus::Completed));

        let provenance = rec.close(dir.path());

        assert_eq!(provenance.references.len(), 1);
        let TurnReference::File(FileTurnReference::Vault {
            path, relations, ..
        }) = &provenance.references[0]
        else {
            panic!("expected vault reference");
        };
        assert_eq!(path, "sub/deep.md");
        assert_eq!(
            relations,
            &vec![TurnReferenceRelation::Read, TurnReferenceRelation::Updated]
        );
        assert_eq!(provenance.activity.len(), 3);
    }

    #[test]
    fn unfinished_tool_is_incomplete_and_yields_no_reference() {
        let dir = vault();
        let mut rec = recorder();
        rec.observe(&call(
            "tc-1",
            "Read",
            ToolKind::Read,
            &[dir.path().join("notes.md")],
        ));
        rec.observe(&status("tc-1", ToolCallStatus::InProgress));

        let provenance = rec.close(dir.path());

        assert!(provenance.references.is_empty());
        assert_eq!(
            tool_activities(&provenance),
            vec![(
                ToolActivityKind::Read,
                ToolActivityStatus::Incomplete,
                "Read".to_string()
            )]
        );
    }

    #[test]
    fn failed_tool_is_failed_and_yields_no_reference() {
        let dir = vault();
        let mut rec = recorder();
        rec.observe(&call(
            "tc-1",
            "Read",
            ToolKind::Read,
            &[dir.path().join("notes.md")],
        ));
        rec.observe(&status("tc-1", ToolCallStatus::Failed));

        let provenance = rec.close(dir.path());

        assert!(provenance.references.is_empty());
        assert_eq!(
            tool_activities(&provenance)[0].1,
            ToolActivityStatus::Failed
        );
    }

    #[test]
    fn terminal_status_never_regresses() {
        let dir = vault();
        let mut rec = recorder();
        rec.observe(&call("tc-1", "Read", ToolKind::Read, &[]));
        rec.observe(&status("tc-1", ToolCallStatus::Completed));
        rec.observe(&status("tc-1", ToolCallStatus::InProgress));

        let provenance = rec.close(dir.path());

        assert_eq!(
            tool_activities(&provenance)[0].1,
            ToolActivityStatus::Completed
        );
    }

    #[test]
    fn update_refines_title_kind_and_locations_of_same_item() {
        let dir = vault();
        let mut rec = recorder();
        rec.observe(&call("tc-1", "Tool", ToolKind::Other, &[]));
        rec.observe(&SessionUpdate::ToolCallUpdate(ToolCallUpdate::new(
            ToolCallId::new("tc-1"),
            ToolCallUpdateFields::new()
                .title("Read notes.md".to_string())
                .kind(ToolKind::Read)
                .locations(vec![ToolCallLocation::new(dir.path().join("notes.md"))])
                .status(ToolCallStatus::Completed),
        )));

        let provenance = rec.close(dir.path());

        assert_eq!(provenance.activity.len(), 1);
        assert_eq!(
            tool_activities(&provenance),
            vec![(
                ToolActivityKind::Read,
                ToolActivityStatus::Completed,
                "Read notes.md".to_string()
            )]
        );
        assert_eq!(provenance.references.len(), 1);
    }

    #[test]
    fn execute_and_fetch_tools_produce_activity_but_no_file_reference() {
        let dir = vault();
        let mut rec = recorder();
        rec.observe(&call(
            "tc-1",
            "cat notes.md",
            ToolKind::Execute,
            &[dir.path().join("notes.md")],
        ));
        rec.observe(&status("tc-1", ToolCallStatus::Completed));
        rec.observe(&call("tc-2", "Fetch", ToolKind::Fetch, &[]));
        rec.observe(&status("tc-2", ToolCallStatus::Completed));

        let provenance = rec.close(dir.path());

        assert!(provenance.references.is_empty());
        assert_eq!(
            tool_activities(&provenance)
                .iter()
                .map(|(kind, _, _)| *kind)
                .collect::<Vec<_>>(),
            vec![ToolActivityKind::Execute, ToolActivityKind::Fetch]
        );
    }

    #[test]
    fn message_text_and_thoughts_are_not_activity() {
        let dir = vault();
        let mut rec = recorder();
        let chunk = ContentChunk::new(ContentBlock::Text(TextContent::new("hi")));
        rec.observe(&SessionUpdate::AgentMessageChunk(chunk.clone()));
        rec.observe(&SessionUpdate::AgentThoughtChunk(chunk));

        let provenance = rec.close(dir.path());

        assert!(provenance.activity.is_empty());
        assert_eq!(provenance.completeness, ProvenanceCompleteness::Complete);
    }

    #[test]
    fn serializes_to_frontend_wire_shape() {
        let dir = vault();
        let mut rec = recorder();
        rec.observe(&call(
            "tc-1",
            "Read notes.md",
            ToolKind::Read,
            &[dir.path().join("notes.md")],
        ));
        rec.observe(&status("tc-1", ToolCallStatus::Completed));

        let json = serde_json::to_value(rec.close(dir.path())).unwrap();

        assert_eq!(
            json,
            serde_json::json!({
                "completeness": "complete",
                "references": [{
                    "type": "file",
                    "scope": "vault",
                    "path": "notes.md",
                    "displayName": "notes.md",
                    "relations": ["read"],
                    "timestamp": 1
                }],
                "activity": [{
                    "type": "tool",
                    "kind": "read",
                    "title": "Read notes.md",
                    "status": "completed",
                    "timestamp": 1,
                    "completedAt": 2
                }]
            })
        );
    }
}
