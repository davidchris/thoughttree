use std::collections::BTreeMap;

use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use thiserror::Error;

use crate::{
    is_vault_relative_path, normalize_provenance, Graph, GraphEdge, GraphNode, NodeKind, Position,
    Role,
};

pub const GRAPH_JSON_VERSION: u32 = 5;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
    XHigh,
}

impl ReasoningEffort {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Project {
    pub graph: Graph,
    /// Unknown provider keys survive round trips. Null entries mean unset.
    pub project_model_preferences: Option<BTreeMap<String, String>>,
    pub project_effort_preferences: Option<BTreeMap<String, ReasoningEffort>>,
}

#[derive(Debug, Error)]
pub enum ProjectError {
    #[error("Invalid Project file JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("Unsupported Project file version: {0}")]
    UnsupportedVersion(Value),
    #[error("Invalid Project file: {0}")]
    Invalid(&'static str),
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct ProjectFile<'a> {
    version: u32,
    graph: GraphJson,
    project_model_preferences: &'a Option<BTreeMap<String, String>>,
    project_effort_preferences: &'a Option<BTreeMap<String, ReasoningEffort>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GraphJson {
    pub version: u32,
    pub nodes: Vec<GraphNode>,
    pub edges: Vec<GraphEdge>,
    pub layout: Vec<LayoutEntry>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct LayoutEntry {
    pub id: String,
    pub position: Position,
}

impl Project {
    pub fn from_json(source: &str) -> Result<Self, ProjectError> {
        let value: Value = serde_json::from_str(source)?;
        let version = value.get("version").and_then(Value::as_u64);
        let graph = match version {
            Some(1 | 2) => legacy_graph(&value)?,
            Some(3..=5) => current_graph(
                value
                    .get("graph")
                    .ok_or(ProjectError::Invalid("missing graph"))?,
            )?,
            _ => {
                return Err(ProjectError::UnsupportedVersion(
                    value.get("version").cloned().unwrap_or(Value::Null),
                ))
            }
        };
        let project_model_preferences = preferences(value.get("projectModelPreferences"));
        let project_effort_preferences = if version.is_some_and(|version| version >= 3) {
            preferences(value.get("projectEffortPreferences"))
        } else {
            None
        };
        Ok(Self {
            graph,
            project_model_preferences,
            project_effort_preferences,
        })
    }

    pub fn to_json(&self) -> Result<String, ProjectError> {
        let graph = GraphJson::from_graph(&self.graph)?;
        Ok(serde_json::to_string_pretty(&ProjectFile {
            version: GRAPH_JSON_VERSION,
            graph,
            project_model_preferences: &self.project_model_preferences,
            project_effort_preferences: &self.project_effort_preferences,
        })?)
    }
}

impl GraphJson {
    pub fn from_graph(graph: &Graph) -> Result<Self, ProjectError> {
        let nodes = graph
            .nodes
            .values()
            .map(serde_json::to_value)
            .collect::<Result<Vec<_>, _>>()?
            .iter()
            .filter_map(normalize_node)
            .collect();
        let edges = graph.edges.clone();
        let layout = graph
            .layout
            .iter()
            .map(|(id, position)| LayoutEntry {
                id: id.clone(),
                position: *position,
            })
            .collect();
        Ok(Self {
            version: GRAPH_JSON_VERSION,
            nodes,
            edges,
            layout,
        })
    }
}

fn preferences<T: for<'de> Deserialize<'de>>(value: Option<&Value>) -> Option<BTreeMap<String, T>> {
    let object = value?.as_object()?;
    Some(
        object
            .iter()
            .filter_map(|(key, value)| {
                serde_json::from_value(value.clone())
                    .ok()
                    .map(|value| (key.clone(), value))
            })
            .collect(),
    )
}

fn current_graph(value: &Value) -> Result<Graph, ProjectError> {
    let raw_nodes = value
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or(ProjectError::Invalid("missing graph nodes"))?;
    let nodes = raw_nodes
        .iter()
        .filter_map(normalize_node)
        .map(|node| (node.id.clone(), node))
        .collect();
    let mut graph = Graph {
        nodes,
        ..Graph::default()
    };
    graph.edges = normalized_edges(value.get("edges"), &graph)?;
    let layout: Vec<LayoutEntry> = serde_json::from_value(
        value
            .get("layout")
            .cloned()
            .ok_or(ProjectError::Invalid("missing graph layout"))?,
    )?;
    graph.layout = layout
        .into_iter()
        .filter(|entry| entry.position.x.is_finite() && entry.position.y.is_finite())
        .map(|entry| (entry.id, entry.position))
        .collect();
    Ok(graph)
}

fn normalized_edges(value: Option<&Value>, graph: &Graph) -> Result<Vec<GraphEdge>, ProjectError> {
    let edges: Vec<GraphEdge> = serde_json::from_value(
        value
            .cloned()
            .ok_or(ProjectError::Invalid("missing graph edges"))?,
    )?;
    Ok(edges
        .into_iter()
        .filter(|edge| {
            !graph
                .nodes
                .get(&edge.target)
                .is_some_and(|node| node.role() == Role::File)
        })
        .collect())
}

fn legacy_graph(value: &Value) -> Result<Graph, ProjectError> {
    let nodes = value
        .get("nodes")
        .and_then(Value::as_array)
        .ok_or(ProjectError::Invalid("missing legacy nodes"))?;
    let data = value
        .get("nodeData")
        .and_then(Value::as_object)
        .ok_or(ProjectError::Invalid("missing legacy nodeData"))?;
    let mut graph = Graph::default();
    for flow in nodes {
        let Some(id) = flow.get("id").and_then(Value::as_str) else {
            continue;
        };
        let Some(mut raw) = data.get(id).cloned() else {
            continue;
        };
        if raw.get("contentUpdatedAt").is_none() {
            raw["contentUpdatedAt"] = raw["timestamp"].clone();
        }
        if raw.get("role").and_then(Value::as_str) == Some("assistant")
            && raw.get("provider").is_none()
        {
            raw["provider"] = Value::String("claude-code".into());
        }
        let Some(node) = normalize_node(&raw) else {
            continue;
        };
        let position: Position = serde_json::from_value(
            flow.get("position")
                .cloned()
                .ok_or(ProjectError::Invalid("missing legacy position"))?,
        )?;
        // Legacy ReactFlow ids are the keys, just as in the TypeScript migration.
        graph.nodes.insert(id.into(), node);
        graph.layout.insert(id.into(), position);
    }
    graph.edges = normalized_edges(value.get("edges"), &graph)?;
    Ok(graph)
}

/// Invalid nodes are omitted, matching the browser persistence boundary.
pub fn normalize_node(value: &Value) -> Option<GraphNode> {
    let id = value.get("id")?.as_str()?;
    let content = value.get("content")?.as_str()?;
    let timestamp = value
        .get("timestamp")?
        .as_f64()
        .filter(|number| number.is_finite())?;
    let role = value.get("role")?.as_str()?;
    let mut normalized =
        serde_json::json!({"id": id, "role": role, "content": content, "timestamp": timestamp});
    for name in ["contentUpdatedAt", "summaryTimestamp"] {
        if let Some(number) = value
            .get(name)
            .and_then(Value::as_f64)
            .filter(|number| number.is_finite())
        {
            normalized[name] = Value::from(number);
        }
    }
    if let Some(summary) = value.get("summary").and_then(Value::as_str) {
        normalized["summary"] = Value::String(summary.into());
    }
    match role {
        "user" => normalize_user(value, &mut normalized),
        "assistant" => normalize_assistant(value, &mut normalized),
        "file" => return normalize_file(value, id, timestamp),
        _ => return None,
    }
    serde_json::from_value(normalized).ok()
}

fn normalize_user(value: &Value, normalized: &mut Value) {
    if let Some(images) = value.get("images").and_then(Value::as_array) {
        let images: Vec<crate::ImageAttachment> = images
            .iter()
            .filter_map(|image| serde_json::from_value(image.clone()).ok())
            .collect();
        normalized["images"] = serde_json::to_value(images).expect("typed image serialization");
    }
}

fn normalize_assistant(value: &Value, normalized: &mut Value) {
    if let Some(provider) = value
        .get("provider")
        .and_then(Value::as_str)
        .filter(|provider| matches!(*provider, "codex" | "claude-code" | "gemini-cli"))
    {
        normalized["provider"] = Value::String(provider.into());
    }
    if let Some(model) = value.get("model").and_then(Value::as_str) {
        normalized["model"] = Value::String(model.into());
    }
    if value.get("incomplete") == Some(&Value::Bool(true)) {
        normalized["incomplete"] = Value::Bool(true);
    }
    if let Some(provenance) = value.get("provenance").and_then(normalize_provenance) {
        normalized["provenance"] =
            serde_json::to_value(provenance).expect("typed provenance serialization");
    }
}

fn normalize_file(value: &Value, id: &str, timestamp: f64) -> Option<GraphNode> {
    let file: crate::FileData = serde_json::from_value(value.clone()).ok()?;
    if !is_vault_relative_path(&file.path)
        || file.name.is_empty()
        || file.name.trim() != file.name
        || file.name == "."
        || file.name == ".."
        || file.name.contains(['/', '\\'])
    {
        return None;
    }
    // JS clients represent byte counts as safe integers.
    const MAX_SAFE: u64 = 9_007_199_254_740_991;
    if [file.size, file.seen_mtime, file.seen_size]
        .into_iter()
        .any(|number| number > MAX_SAFE)
    {
        return None;
    }
    Some(GraphNode {
        id: id.into(),
        content: String::new(),
        timestamp,
        content_updated_at: None,
        summary: None,
        summary_timestamp: None,
        kind: NodeKind::File(file),
    })
}

impl From<Graph> for Project {
    fn from(graph: Graph) -> Self {
        Self {
            graph,
            ..Self::default()
        }
    }
}

impl From<GraphJson> for Graph {
    fn from(json: GraphJson) -> Self {
        let nodes: IndexMap<_, _> = json
            .nodes
            .into_iter()
            .map(|node| (node.id.clone(), node))
            .collect();
        let edges = json
            .edges
            .into_iter()
            .filter(|edge| {
                !nodes
                    .get(&edge.target)
                    .is_some_and(|node| node.role() == Role::File)
            })
            .collect();
        let layout = json
            .layout
            .into_iter()
            .map(|entry| (entry.id, entry.position))
            .collect();
        Self {
            nodes,
            edges,
            layout,
        }
    }
}
