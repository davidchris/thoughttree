use serde::{Deserialize, Serialize};

use crate::TurnProvenance;

pub type NodeId = String;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Role {
    User,
    Assistant,
    File,
}

impl Role {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::User => "user",
            Self::Assistant => "assistant",
            Self::File => "file",
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum GraphProvider {
    ClaudeCode,
    Codex,
    /// Retained in historical Project files, never offered as a runnable provider.
    GeminiCli,
}

impl GraphProvider {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::ClaudeCode => "claude-code",
            Self::Codex => "codex",
            Self::GeminiCli => "gemini-cli",
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ImageAttachment {
    pub data: String,
    pub mime_type: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct UserData {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageAttachment>,
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct AssistantData {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provider: Option<GraphProvider>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub incomplete: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub provenance: Option<TurnProvenance>,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileData {
    pub path: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
    pub seen_mtime: u64,
    pub seen_size: u64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(tag = "role", rename_all = "lowercase")]
pub enum NodeKind {
    User(UserData),
    Assistant(AssistantData),
    File(FileData),
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GraphNode {
    pub id: NodeId,
    pub content: String,
    pub timestamp: f64,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub content_updated_at: Option<f64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary_timestamp: Option<f64>,
    #[serde(flatten)]
    pub kind: NodeKind,
}

impl GraphNode {
    fn new(
        id: impl Into<String>,
        content: impl Into<String>,
        timestamp: f64,
        kind: NodeKind,
    ) -> Self {
        Self {
            id: id.into(),
            content: content.into(),
            timestamp,
            content_updated_at: Some(timestamp),
            summary: None,
            summary_timestamp: None,
            kind,
        }
    }

    pub fn user(id: impl Into<String>, content: impl Into<String>, timestamp: f64) -> Self {
        Self::new(id, content, timestamp, NodeKind::User(UserData::default()))
    }

    pub fn assistant(id: impl Into<String>, content: impl Into<String>, timestamp: f64) -> Self {
        Self::new(
            id,
            content,
            timestamp,
            NodeKind::Assistant(AssistantData::default()),
        )
    }

    pub fn file(id: impl Into<String>, file: FileData, timestamp: f64) -> Self {
        let mut node = Self::new(id, "", timestamp, NodeKind::File(file));
        node.content_updated_at = None;
        node
    }

    pub fn role(&self) -> Role {
        match self.kind {
            NodeKind::User(_) => Role::User,
            NodeKind::Assistant(_) => Role::Assistant,
            NodeKind::File(_) => Role::File,
        }
    }

    pub fn images(&self) -> &[ImageAttachment] {
        match &self.kind {
            NodeKind::User(data) => &data.images,
            _ => &[],
        }
    }

    pub fn file_data(&self) -> Option<&FileData> {
        match &self.kind {
            NodeKind::File(data) => Some(data),
            _ => None,
        }
    }

    pub fn assistant_data(&self) -> Option<&AssistantData> {
        match &self.kind {
            NodeKind::Assistant(data) => Some(data),
            _ => None,
        }
    }

    pub fn provenance(&self) -> Option<&TurnProvenance> {
        self.assistant_data()
            .and_then(|data| data.provenance.as_ref())
    }

    pub fn title(&self) -> String {
        if let Some(file) = self.file_data() {
            return file.name.clone();
        }
        self.summary
            .clone()
            .filter(|text| !text.is_empty())
            .unwrap_or_else(|| {
                self.content
                    .trim_start()
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .chars()
                    .take(80)
                    .collect()
            })
    }
}

#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct GraphEdge {
    pub id: String,
    pub source: NodeId,
    pub target: NodeId,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FileRef {
    pub path: String,
    pub name: String,
    pub mime_type: String,
    pub size: u64,
}

impl From<&FileData> for FileRef {
    fn from(file: &FileData) -> Self {
        Self {
            path: file.path.clone(),
            name: file.name.clone(),
            mime_type: file.mime_type.clone(),
            size: file.size,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConversationMessage {
    pub role: Role,
    pub content: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub images: Vec<ImageAttachment>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub files: Vec<FileRef>,
}
