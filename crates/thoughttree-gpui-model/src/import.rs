use std::sync::LazyLock;

use regex::Regex;
use serde_json::{json, Value};
use thiserror::Error;

use crate::{
    is_web_url, normalize_provenance, Graph, GraphNode, NodeKind, Position, TurnProvenance,
};

pub const KAGI_EXPORT_MAX_BYTES: usize = 16 * 1024 * 1024;

#[derive(Clone, Debug, PartialEq)]
pub struct ImportedConversation {
    pub import_key: String,
    pub turns: Vec<ImportedConversationTurn>,
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct ImportedConversationTurn {
    pub user_message: String,
    pub assistant_answer: String,
    pub incomplete: bool,
    pub model: Option<String>,
    pub user_timestamp: Option<f64>,
    pub assistant_timestamp: Option<f64>,
    pub provenance: Option<TurnProvenance>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct TurnRange {
    pub start_index: Option<usize>,
    pub end_index: Option<usize>,
}

#[derive(Debug, Error)]
pub enum ImportError {
    #[error("Kagi export exceeds the {limit}-byte input limit ({size} bytes)")]
    InputTooLarge { size: usize, limit: usize },
    #[error("Invalid Kagi export JSON")]
    InvalidJson,
    #[error("Unsupported Kagi export version: {0}")]
    UnsupportedVersion(Value),
    #[error("Kagi export contains no messages")]
    NoMessages,
}

pub fn parse_kagi_export(input: impl AsRef<[u8]>) -> Result<ImportedConversation, ImportError> {
    parse_kagi_export_with_limit(input.as_ref(), KAGI_EXPORT_MAX_BYTES)
}

pub fn parse_kagi_export_with_limit(
    input: &[u8],
    limit: usize,
) -> Result<ImportedConversation, ImportError> {
    if input.len() > limit {
        return Err(ImportError::InputTooLarge {
            size: input.len(),
            limit,
        });
    }
    let source = std::str::from_utf8(input).map_err(|_| ImportError::InvalidJson)?;
    let value: Value = serde_json::from_str(source).map_err(|_| ImportError::InvalidJson)?;
    if value.get("version").and_then(Value::as_u64) != Some(1) {
        return Err(ImportError::UnsupportedVersion(
            value.get("version").cloned().unwrap_or(Value::Null),
        ));
    }
    let conversation = value.get("conversation").unwrap_or(&Value::Null);
    let messages = value
        .get("messages")
        .and_then(Value::as_array)
        .or_else(|| conversation.get("messages").and_then(Value::as_array));
    let mut turns = Vec::new();
    let mut pending = None;
    for message in messages.into_iter().flatten() {
        match message.get("role").and_then(Value::as_str) {
            Some("user") => {
                flush_pending(&mut pending, &mut turns);
                pending = Some(message);
            }
            Some("assistant") => {
                if let Some(user) = pending.take() {
                    turns.push(turn(user, Some(message)));
                }
            }
            _ => flush_pending(&mut pending, &mut turns),
        }
    }
    flush_pending(&mut pending, &mut turns);
    if turns.is_empty() {
        return Err(ImportError::NoMessages);
    }
    let import_key = conversation
        .get("title")
        .and_then(Value::as_str)
        .filter(|text| !text.is_empty())
        .unwrap_or("Kagi conversation")
        .to_owned();
    Ok(ImportedConversation { import_key, turns })
}

fn flush_pending(pending: &mut Option<&Value>, turns: &mut Vec<ImportedConversationTurn>) {
    if let Some(user) = pending.take() {
        turns.push(turn(user, None));
    }
}

fn turn(user: &Value, assistant: Option<&Value>) -> ImportedConversationTurn {
    let user_message = user
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into();
    let Some(assistant) = assistant else {
        return ImportedConversationTurn {
            user_message,
            incomplete: true,
            ..Default::default()
        };
    };
    let assistant_answer = assistant
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .into();
    let model = assistant
        .get("model_name")
        .and_then(Value::as_str)
        .map(str::to_owned);
    ImportedConversationTurn {
        user_message,
        assistant_answer,
        model,
        provenance: kagi_provenance(assistant),
        ..Default::default()
    }
}

static CITATION: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"【(\d+)】").expect("citation expression"));

fn kagi_provenance(message: &Value) -> Option<TurnProvenance> {
    let content = message
        .get("content")
        .and_then(Value::as_str)
        .unwrap_or_default();
    let cited: Vec<f64> = CITATION
        .captures_iter(content)
        .filter_map(|capture| capture[1].parse().ok())
        .collect();
    let references: Vec<_> = message
        .get("references")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|value| {
            let url = value.get("url")?.as_str().filter(|url| is_web_url(url))?;
            let relation = if value
                .get("index")
                .and_then(Value::as_f64)
                .is_some_and(|index| cited.contains(&index))
            {
                "cited"
            } else {
                "consulted"
            };
            let mut reference = json!({"type": "url", "url": url, "relations": [relation]});
            for field in ["title", "domain", "index", "percentage", "is_search_result"] {
                if let Some(value) = value.get(field) {
                    reference[field] = value.clone();
                }
            }
            Some(reference)
        })
        .collect();
    normalize_provenance(
        &json!({"completeness": "complete", "references": references, "activity": []}),
    )
}

fn encode_component(text: &str) -> String {
    let mut encoded = String::new();
    for byte in text.bytes() {
        if byte.is_ascii_alphanumeric() || b"-_.!~*'()".contains(&byte) {
            encoded.push(byte as char);
        } else {
            encoded.push_str(&format!("%{byte:02X}"));
        }
    }
    encoded
}

pub fn conversation_to_graph(conversation: &ImportedConversation, range: TurnRange) -> Graph {
    let mut graph = Graph::default();
    let start = range.start_index.unwrap_or(0);
    let end = range.end_index.unwrap_or(usize::MAX);
    let key = encode_component(&conversation.import_key);
    let mut previous: Option<String> = None;
    for (index, turn) in conversation
        .turns
        .iter()
        .enumerate()
        .filter(|(index, _)| *index >= start && *index <= end)
    {
        let user_id = format!("import:{key}:turn:{index}:user");
        let assistant_id = format!("import:{key}:turn:{index}:assistant");
        let mut user = GraphNode::user(
            &user_id,
            &turn.user_message,
            turn.user_timestamp.unwrap_or((index * 2) as f64),
        );
        let mut assistant = GraphNode::assistant(
            &assistant_id,
            &turn.assistant_answer,
            turn.assistant_timestamp.unwrap_or((index * 2 + 1) as f64),
        );
        user.content_updated_at = None;
        assistant.content_updated_at = None;
        if let NodeKind::Assistant(data) = &mut assistant.kind {
            data.model = turn.model.clone();
            data.incomplete = turn.incomplete;
            data.provenance = turn.provenance.clone();
        }
        graph.add_node(
            user,
            Position {
                x: 0.0,
                y: (index * 240) as f64,
            },
        );
        graph.add_node(
            assistant,
            Position {
                x: 0.0,
                y: (index * 240 + 120) as f64,
            },
        );
        if let Some(previous) = previous {
            graph
                .add_edge(&previous, &user_id)
                .expect("new linear graph");
        }
        graph
            .add_edge(&user_id, &assistant_id)
            .expect("new linear graph");
        previous = Some(assistant_id);
    }
    graph
}

impl ImportedConversation {
    pub fn to_graph(&self, range: TurnRange) -> Graph {
        conversation_to_graph(self, range)
    }
}
