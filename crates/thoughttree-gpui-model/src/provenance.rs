//! The same allowlist as `packages/graph-model/src/normalize.ts`.
//! Unknown provider payloads and raw tool input/output never enter saved files.

use std::sync::LazyLock;

use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ProvenanceCompleteness {
    Complete,
    Partial,
    Unknown,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum TurnReferenceRelation {
    Consulted,
    Cited,
    Read,
    Created,
    Updated,
    Deleted,
    Moved,
    Searched,
    Fetched,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TurnReference {
    Url {
        url: String,
        relations: Vec<TurnReferenceRelation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        domain: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        index: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        percentage: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        is_search_result: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<f64>,
    },
    File(FileTurnReference),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "scope", rename_all = "lowercase")]
pub enum FileTurnReference {
    #[serde(rename_all = "camelCase")]
    Vault {
        path: String,
        display_name: String,
        relations: Vec<TurnReferenceRelation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    External {
        display_name: String,
        relations: Vec<TurnReferenceRelation>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<f64>,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
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

impl ToolActivityKind {
    fn summary(self) -> &'static str {
        match self {
            Self::Read => "Read a file",
            Self::Edit => "Edited a file",
            Self::Delete => "Deleted a file",
            Self::Move => "Moved a file",
            Self::Search => "Ran a search",
            Self::Execute => "Ran a command",
            Self::Fetch => "Fetched a resource",
            Self::Delegate => "Delegated a task",
            Self::Other => "Used a tool",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ToolActivityStatus {
    Pending,
    Completed,
    Failed,
    Incomplete,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "lowercase")]
pub enum TurnActivity {
    Commentary {
        content: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    Tool {
        kind: ToolActivityKind,
        title: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        title_truncated: Option<bool>,
        #[serde(skip_serializing_if = "Option::is_none")]
        title_redacted: Option<bool>,
        status: ToolActivityStatus,
        #[serde(skip_serializing_if = "Option::is_none")]
        completed_at: Option<f64>,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<f64>,
    },
    #[serde(rename_all = "camelCase")]
    Unknown {
        provider_type: String,
        label: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        timestamp: Option<f64>,
    },
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct TurnProvenance {
    pub completeness: ProvenanceCompleteness,
    pub references: Vec<TurnReference>,
    pub activity: Vec<TurnActivity>,
}

static HOST_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r#"(?:^|[\s"'`(=:,<\[])(?:/[^\s/]|~[\\/]|[A-Za-z]:[\\/]|\\\\)"#)
        .expect("host path expression")
});
static BARE_PATH: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?i)^(?:file:|/|~[\\/]|[A-Za-z]:[\\/]|\\\\)").expect("bare path expression")
});
static SCHEME: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z][a-zA-Z0-9+.-]*:").expect("scheme expression"));

pub fn is_vault_relative_path(path: &str) -> bool {
    !path.is_empty()
        && !path.starts_with(['/', '\\'])
        && !SCHEME.is_match(path)
        && !path.split(['/', '\\']).any(|segment| segment == "..")
}

pub fn is_file_url_or_bare_path(text: &str) -> bool {
    let normalized = text.replace(['\t', '\n', '\r'], "");
    BARE_PATH.is_match(normalized.trim_start_matches(|c: char| c.is_whitespace() || c <= ' '))
}

pub fn contains_host_path(text: &str) -> bool {
    is_file_url_or_bare_path(text) || HOST_PATH.is_match(text)
}

pub fn is_web_url(text: &str) -> bool {
    url::Url::parse(text).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

pub fn safe_tool_title(raw: &str, kind: ToolActivityKind) -> (String, bool) {
    let collapsed = raw.split_whitespace().collect::<Vec<_>>().join(" ");
    if kind == ToolActivityKind::Execute {
        return (kind.summary().to_owned(), raw != kind.summary());
    }
    let shell = collapsed.contains(['`', '$', '|', ';', '<', '>'])
        || collapsed.contains("&&")
        || collapsed.chars().any(|c| c.is_ascii_control());
    if collapsed.is_empty() || shell || contains_host_path(&collapsed) {
        return (kind.summary().to_owned(), true);
    }
    (collapsed.chars().take(200).collect(), false)
}

fn text(value: &Value, key: &str) -> Option<String> {
    value.get(key)?.as_str().map(str::to_owned)
}
fn number(value: &Value, key: &str) -> Option<f64> {
    value.get(key)?.as_f64().filter(|n| n.is_finite())
}

fn safe_text(value: Option<String>, loss: &mut bool) -> Option<String> {
    let text = value?;
    if contains_host_path(&text) {
        *loss = true;
        None
    } else {
        Some(text)
    }
}

fn display_name(value: &str, loss: &mut bool) -> Option<String> {
    let name = value.rsplit(['/', '\\']).next()?.trim();
    *loss |= name != value;
    (!name.is_empty() && name != "." && name != "..").then(|| name.to_owned())
}

fn reference(value: &Value, loss: &mut bool) -> Option<TurnReference> {
    let mut relations = Vec::new();
    for relation in value
        .get("relations")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        match serde_json::from_value(relation.clone()) {
            Ok(relation) => relations.push(relation),
            Err(_) => *loss = true,
        }
    }
    let timestamp = number(value, "timestamp");
    match value.get("type")?.as_str()? {
        "url" => {
            let url = text(value, "url")?;
            if is_file_url_or_bare_path(&url) {
                return None;
            }
            Some(TurnReference::Url {
                url,
                relations,
                title: safe_text(text(value, "title"), loss),
                domain: safe_text(text(value, "domain"), loss),
                index: number(value, "index"),
                percentage: number(value, "percentage"),
                is_search_result: value.get("is_search_result").and_then(Value::as_bool),
                timestamp,
            })
        }
        "file" => {
            let path = text(value, "path");
            let name = text(value, "displayName").or_else(|| path.clone())?;
            let display_name = display_name(&name, loss)?;
            if value.get("scope").and_then(Value::as_str) == Some("vault") {
                if let Some(path) = path.filter(|path| is_vault_relative_path(path)) {
                    return Some(TurnReference::File(FileTurnReference::Vault {
                        path,
                        display_name,
                        relations,
                        timestamp,
                    }));
                }
                *loss = true;
            }
            Some(TurnReference::File(FileTurnReference::External {
                display_name,
                relations,
                timestamp,
            }))
        }
        _ => None,
    }
}

fn activity(value: &Value, loss: &mut bool) -> Option<TurnActivity> {
    let timestamp = number(value, "timestamp");
    match value.get("type")?.as_str()? {
        "commentary" => Some(TurnActivity::Commentary {
            content: text(value, "content")?,
            timestamp,
        }),
        "tool" => {
            let raw = text(value, "title")?;
            let kind = value
                .get("kind")
                .and_then(|kind| serde_json::from_value(kind.clone()).ok())
                .unwrap_or(ToolActivityKind::Other);
            let (title, redacted) = safe_tool_title(&raw, kind);
            let title_truncated = !redacted
                && (value.get("titleTruncated") == Some(&Value::Bool(true))
                    || raw.trim().chars().count() > 200);
            let title_redacted = redacted || value.get("titleRedacted") == Some(&Value::Bool(true));
            *loss |= title_truncated || title_redacted;
            let status = value
                .get("status")
                .and_then(|status| serde_json::from_value(status.clone()).ok())
                .unwrap_or(ToolActivityStatus::Incomplete);
            Some(TurnActivity::Tool {
                kind,
                title,
                title_truncated: title_truncated.then_some(true),
                title_redacted: title_redacted.then_some(true),
                status,
                completed_at: number(value, "completedAt"),
                timestamp,
            })
        }
        "unknown" => {
            let provider_type = text(value, "providerType")?;
            let label = text(value, "label")?;
            *loss = true;
            Some(TurnActivity::Unknown {
                provider_type: safe_text(Some(provider_type), loss)
                    .unwrap_or_else(|| "unknown".into()),
                label: safe_text(Some(label), loss).unwrap_or_else(|| "Unknown activity".into()),
                timestamp,
            })
        }
        _ => None,
    }
}

fn normalized_list<T>(
    value: Option<&Value>,
    parse: fn(&Value, &mut bool) -> Option<T>,
    loss: &mut bool,
) -> Vec<T> {
    let mut result = Vec::new();
    for value in value.and_then(Value::as_array).into_iter().flatten() {
        match parse(value, loss) {
            Some(value) => result.push(value),
            None => *loss = true,
        }
    }
    result
}

pub fn normalize_provenance(value: &Value) -> Option<TurnProvenance> {
    if !value.is_object() {
        return None;
    }
    let mut loss = false;
    let references = normalized_list(value.get("references"), reference, &mut loss);
    let activity = normalized_list(value.get("activity"), activity, &mut loss);
    let mut completeness = value
        .get("completeness")
        .and_then(|v| serde_json::from_value(v.clone()).ok())
        .unwrap_or(ProvenanceCompleteness::Unknown);
    if completeness == ProvenanceCompleteness::Complete && loss {
        completeness = ProvenanceCompleteness::Partial;
    }
    Some(TurnProvenance {
        completeness,
        references,
        activity,
    })
}

fn enum_text(value: &impl Serialize) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_default()
}

fn code_span(text: &str) -> String {
    let longest = text.split(|c| c != '`').map(str::len).max().unwrap_or(0);
    let fence = "`".repeat(longest + 1);
    format!("{fence} {text} {fence}")
}

fn format_url(url: &str) -> String {
    if is_file_url_or_bare_path(url) {
        return "_(file URL redacted)_".into();
    }
    if !is_web_url(url) {
        return code_span(url);
    }
    let mut escaped = String::new();
    for ch in url.chars() {
        if ch == '<' || ch == '>' || ch.is_whitespace() {
            let mut buffer = [0; 4];
            for byte in ch.encode_utf8(&mut buffer).bytes() {
                escaped.push_str(&format!("%{byte:02X}"));
            }
        } else {
            escaped.push(ch);
        }
    }
    format!("<{escaped}>")
}

impl TurnProvenance {
    pub fn markdown(&self) -> String {
        let completeness = match self.completeness {
            ProvenanceCompleteness::Complete => "Complete",
            ProvenanceCompleteness::Partial => "Partial",
            ProvenanceCompleteness::Unknown => "Unknown",
        };
        let mut lines = vec![
            "### Provenance".into(),
            String::new(),
            format!("**Completeness:** {completeness}"),
        ];
        if !self.references.is_empty() {
            lines.extend([String::new(), "#### References".into(), String::new()]);
            lines.extend(
                self.references
                    .iter()
                    .enumerate()
                    .map(|(index, reference)| reference.markdown(index + 1)),
            );
        }
        if !self.activity.is_empty() {
            lines.extend([String::new(), "#### Turn Activity".into(), String::new()]);
            lines.extend(
                self.activity
                    .iter()
                    .enumerate()
                    .map(|(index, activity)| format!("{}. {}", index + 1, activity.markdown())),
            );
        }
        lines.join("\n")
    }
}

impl TurnReference {
    pub fn markdown(&self, index: usize) -> String {
        let (text, relations) = match self {
            Self::Url {
                url,
                title,
                relations,
                ..
            } => {
                let title = title
                    .as_ref()
                    .filter(|title| !title.is_empty())
                    .map(|title| format!("{title} — "))
                    .unwrap_or_default();
                (format!("**URL:** {title}{}", format_url(url)), relations)
            }
            Self::File(FileTurnReference::Vault {
                path, relations, ..
            }) => (format!("**Vault file:** {}", code_span(path)), relations),
            Self::File(FileTurnReference::External {
                display_name,
                relations,
                ..
            }) => (
                format!("**External file:** {}", code_span(display_name)),
                relations,
            ),
        };
        format!(
            "{index}. {text} — Relations: {}",
            relations
                .iter()
                .map(enum_text)
                .collect::<Vec<_>>()
                .join(", ")
        )
    }
}

impl TurnActivity {
    pub fn markdown(&self) -> String {
        match self {
            Self::Commentary { content, .. } => format!("**Commentary:** {content}"),
            Self::Tool {
                kind,
                status,
                title,
                ..
            } => format!(
                "**Tool ({}, {}):** {title}",
                enum_text(kind),
                enum_text(status)
            ),
            Self::Unknown {
                provider_type,
                label,
                ..
            } => format!("**Unknown ({provider_type}):** {label}"),
        }
    }
}
