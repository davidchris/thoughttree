use serde::{Deserialize, Serialize};

/// Supported agent providers for ACP connections
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AgentProvider {
    #[default]
    ClaudeCode,
    GeminiCli,
}

impl AgentProvider {
    /// Human-readable display name for UI
    pub(crate) fn display_name(&self) -> &'static str {
        match self {
            AgentProvider::ClaudeCode => "Claude Code",
            AgentProvider::GeminiCli => "Gemini CLI",
        }
    }
}

/// Provider availability status for frontend
#[derive(Clone, Debug, Serialize)]
pub(crate) struct ProviderStatus {
    pub provider: AgentProvider,
    pub available: bool,
    pub error_message: Option<String>,
}

/// Model info discovered from ACP CreateSessionResponse.models.available_models
#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ModelInfo {
    pub model_id: String,
    pub display_name: String,
}

/// User's preferred model per provider (stores model_id strings)
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub(crate) struct ModelPreferences {
    #[serde(default, rename = "claude-code")]
    pub claude_code: Option<String>,
    #[serde(default, rename = "gemini-cli")]
    pub gemini_cli: Option<String>,
}

impl ModelPreferences {
    /// Set the model preference for a given provider
    pub(crate) fn set(&mut self, provider: &AgentProvider, model_id: Option<String>) {
        match provider {
            AgentProvider::ClaudeCode => self.claude_code = model_id,
            AgentProvider::GeminiCli => self.gemini_cli = model_id,
        }
    }
}

/// Custom executable paths for providers (user-configured overrides)
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
pub(crate) struct ProviderPaths {
    #[serde(default, rename = "claude-code")]
    pub claude_code: Option<String>,
    #[serde(default, rename = "gemini-cli")]
    pub gemini_cli: Option<String>,
}

impl ProviderPaths {
    /// Set the custom path for a given provider
    pub(crate) fn set(&mut self, provider: &AgentProvider, path: Option<String>) {
        match provider {
            AgentProvider::ClaudeCode => self.claude_code = path,
            AgentProvider::GeminiCli => self.gemini_cli = path,
        }
    }
}

// Types for frontend communication
#[derive(Clone, Serialize)]
pub(crate) struct ChunkPayload {
    pub node_id: String,
    pub chunk: String,
}

#[derive(Clone, Serialize)]
pub(crate) struct PermissionPayload {
    pub id: String,
    pub tool_type: String,
    pub tool_name: String,
    pub description: String,
    pub options: Vec<PermissionOption>,
}

#[derive(Clone, Serialize)]
pub(crate) struct PermissionOption {
    pub id: String,
    pub label: String,
}

// Message types from frontend (with optional images)
#[derive(Clone, Deserialize)]
pub(crate) struct MessageImage {
    pub data: String,
    pub mime_type: String,
}

#[derive(Clone, Deserialize)]
pub(crate) struct Message {
    pub role: String,
    pub content: String,
    pub images: Option<Vec<MessageImage>>,
}

#[derive(Clone, Serialize)]
pub(crate) struct SummaryResult {
    pub node_id: String,
    pub summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_default_is_claude_code() {
        let provider = AgentProvider::default();
        assert_eq!(provider, AgentProvider::ClaudeCode);
    }

    #[test]
    fn test_provider_serializes_to_kebab_case() {
        let claude = AgentProvider::ClaudeCode;
        let gemini = AgentProvider::GeminiCli;

        let claude_json = serde_json::to_string(&claude).unwrap();
        let gemini_json = serde_json::to_string(&gemini).unwrap();

        assert_eq!(claude_json, "\"claude-code\"");
        assert_eq!(gemini_json, "\"gemini-cli\"");
    }

    #[test]
    fn test_provider_deserializes_from_kebab_case() {
        let claude: AgentProvider = serde_json::from_str("\"claude-code\"").unwrap();
        let gemini: AgentProvider = serde_json::from_str("\"gemini-cli\"").unwrap();

        assert_eq!(claude, AgentProvider::ClaudeCode);
        assert_eq!(gemini, AgentProvider::GeminiCli);
    }

    #[test]
    fn test_provider_display_names() {
        assert_eq!(AgentProvider::ClaudeCode.display_name(), "Claude Code");
        assert_eq!(AgentProvider::GeminiCli.display_name(), "Gemini CLI");
    }
}
