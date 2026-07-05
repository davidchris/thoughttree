use serde::{Deserialize, Serialize};

/// Supported agent providers for ACP connections
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub(crate) enum AgentProvider {
    #[default]
    ClaudeCode,
    GeminiCli,
    Codex,
}

/// Static per-provider data. Adding a Provider means adding a variant,
/// a descriptor entry, and a spawn arm — no other code changes.
pub(crate) struct ProviderDescriptor {
    /// Serde value of the variant AND key in per-provider config maps
    pub id: &'static str,
    pub display_name: &'static str,
    /// Binary name the discovery paths point at
    pub executable_name: &'static str,
    /// Absolute install locations, in order of preference
    pub known_paths: &'static [&'static str],
    /// Install locations relative to the user's home directory
    pub home_relative_paths: &'static [&'static str],
    /// Env var that overrides discovery entirely
    pub env_override: Option<&'static str>,
    pub install_hint: &'static str,
    /// Substring expected in `--version` output
    pub version_pattern: &'static str,
    /// Whether the provider needs the bundled ACP sidecar (see ADR-0001)
    /// in addition to its CLI to serve sessions
    pub requires_sidecar: bool,
    /// Whether the agent reports models via ACP session creation. When
    /// false, model discovery skips spawning and serves `fallback_models`.
    pub models_via_acp: bool,
    /// (model_id, display_name) offered when ACP model discovery returns nothing
    pub fallback_models: &'static [(&'static str, &'static str)],
}

const CLAUDE_CODE_DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "claude-code",
    display_name: "Claude Code",
    executable_name: "claude",
    known_paths: &["/opt/homebrew/bin/claude", "/usr/local/bin/claude"],
    home_relative_paths: &[
        ".claude/local/claude",
        ".local/bin/claude",
        ".bun/bin/claude",
        ".npm-global/bin/claude",
    ],
    env_override: Some("CLAUDE_CODE_EXECUTABLE"),
    install_hint:
        "Install via: brew install --cask claude-code\nOr: npm install -g @anthropic-ai/claude-code",
    version_pattern: "claude",
    requires_sidecar: true,
    models_via_acp: true,
    fallback_models: &[],
};

const GEMINI_CLI_DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "gemini-cli",
    display_name: "Gemini CLI",
    executable_name: "gemini",
    known_paths: &["/opt/homebrew/bin/gemini", "/usr/local/bin/gemini"],
    home_relative_paths: &[".bun/bin/gemini", ".npm-global/bin/gemini"],
    env_override: None,
    install_hint: "Install via: brew install gemini-cli\nOr: bun install -g @google/gemini-cli",
    version_pattern: "gemini",
    requires_sidecar: false,
    models_via_acp: false,
    fallback_models: &[
        ("gemini-3", "Gemini 3 (Auto)"),
        ("gemini-2.5", "Gemini 2.5 (Auto)"),
    ],
};

const CODEX_DESCRIPTOR: ProviderDescriptor = ProviderDescriptor {
    id: "codex",
    display_name: "Codex",
    executable_name: "codex-acp",
    known_paths: &["/opt/homebrew/bin/codex-acp", "/usr/local/bin/codex-acp"],
    home_relative_paths: &[".bun/bin/codex-acp", ".npm-global/bin/codex-acp"],
    env_override: None,
    install_hint: "Install adapter: npm install -g @agentclientprotocol/codex-acp — then login: npm install -g @openai/codex && codex login",
    version_pattern: "codex",
    requires_sidecar: false,
    models_via_acp: false,
    // ACP model discovery returns nothing for codex-acp, so this curated list
    // drives the selector. Ids verified against codex-cli 0.142.5 (2026-07).
    fallback_models: &[
        ("gpt-5.5", "GPT-5.5"),
        ("gpt-5.4", "GPT-5.4"),
        ("gpt-5.4-mini", "GPT-5.4 Mini"),
        ("gpt-5.3-codex", "GPT-5.3 Codex"),
    ],
};

impl AgentProvider {
    /// Every supported provider — drives availability lists and config maps
    pub(crate) const ALL: &'static [AgentProvider] = &[
        AgentProvider::ClaudeCode,
        AgentProvider::GeminiCli,
        AgentProvider::Codex,
    ];

    pub(crate) fn descriptor(&self) -> &'static ProviderDescriptor {
        match self {
            AgentProvider::ClaudeCode => &CLAUDE_CODE_DESCRIPTOR,
            AgentProvider::GeminiCli => &GEMINI_CLI_DESCRIPTOR,
            AgentProvider::Codex => &CODEX_DESCRIPTOR,
        }
    }

    /// Human-readable display name for UI
    pub(crate) fn display_name(&self) -> &'static str {
        self.descriptor().display_name
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

/// Per-provider config map keyed by descriptor id. Serde-transparent so the
/// on-disk shape stays a plain JSON object; `Option<T>` values keep legacy
/// `null` entries, and String keys keep unknown provider keys from newer
/// app versions.
#[derive(Clone, Debug, Serialize, Deserialize, Default)]
#[serde(transparent)]
pub(crate) struct PerProvider<T>(std::collections::BTreeMap<String, Option<T>>);

impl<T> PerProvider<T> {
    pub(crate) fn get(&self, provider: &AgentProvider) -> Option<&T> {
        self.0
            .get(provider.descriptor().id)
            .and_then(|value| value.as_ref())
    }

    pub(crate) fn set(&mut self, provider: &AgentProvider, value: Option<T>) {
        self.0.insert(provider.descriptor().id.to_string(), value);
    }
}

/// User's preferred model per provider (stores model_id strings)
pub(crate) type ModelPreferences = PerProvider<String>;

/// Custom executable paths for providers (user-configured overrides)
pub(crate) type ProviderPaths = PerProvider<String>;

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
    fn test_codex_serde_round_trip() {
        let codex: AgentProvider = serde_json::from_str("\"codex\"").unwrap();
        assert_eq!(codex, AgentProvider::Codex);
        assert_eq!(serde_json::to_string(&codex).unwrap(), "\"codex\"");
    }

    #[test]
    fn test_provider_display_names() {
        assert_eq!(AgentProvider::ClaudeCode.display_name(), "Claude Code");
        assert_eq!(AgentProvider::GeminiCli.display_name(), "Gemini CLI");
        assert_eq!(AgentProvider::Codex.display_name(), "Codex");
    }

    #[test]
    fn test_codex_descriptor_targets_acp_adapter() {
        // Availability = adapter binary found, so discovery targets codex-acp
        assert_eq!(
            AgentProvider::Codex.descriptor().executable_name,
            "codex-acp"
        );
    }

    #[test]
    fn test_codex_fallback_models_offer_current_codex_lineup() {
        // Codex ACP discovery returns nothing, so this static list drives the
        // model selector. Ids verified against codex-cli 0.142.5 built-in
        // model table (2026-07).
        let ids: Vec<&str> = AgentProvider::Codex
            .descriptor()
            .fallback_models
            .iter()
            .map(|(id, _)| *id)
            .collect();

        assert_eq!(
            ids,
            vec!["gpt-5.5", "gpt-5.4", "gpt-5.4-mini", "gpt-5.3-codex"]
        );
    }

    #[test]
    fn test_codex_install_hint_first_line_covers_adapter_and_login() {
        // The provider dropdown surfaces only the hint's first line — both
        // steps (adapter install + vendor login) must fit there
        let first_line = AgentProvider::Codex
            .descriptor()
            .install_hint
            .lines()
            .next()
            .unwrap();

        // @zed-industries/codex-acp is deprecated; successor package
        assert!(first_line.contains("@agentclientprotocol/codex-acp"));
        assert!(first_line.contains("codex login"));
    }

    #[test]
    fn test_per_provider_loads_legacy_config_with_null_entries() {
        let json = r#"{"claude-code":null,"gemini-cli":"/usr/local/bin/gemini"}"#;
        let paths: PerProvider<String> = serde_json::from_str(json).unwrap();

        assert_eq!(paths.get(&AgentProvider::ClaudeCode), None);
        assert_eq!(
            paths.get(&AgentProvider::GeminiCli),
            Some(&"/usr/local/bin/gemini".to_string())
        );
    }

    #[test]
    fn test_per_provider_round_trip_preserves_nulls_and_unknown_keys() {
        let json =
            r#"{"claude-code":null,"codex":"/opt/codex","gemini-cli":"/usr/local/bin/gemini"}"#;
        let paths: PerProvider<String> = serde_json::from_str(json).unwrap();
        let round_tripped = serde_json::to_string(&paths).unwrap();

        assert_eq!(round_tripped, json);
    }

    #[test]
    fn test_per_provider_set_and_get() {
        let mut prefs: PerProvider<String> = PerProvider::default();
        assert_eq!(prefs.get(&AgentProvider::GeminiCli), None);

        prefs.set(&AgentProvider::GeminiCli, Some("gemini-3".to_string()));
        assert_eq!(
            prefs.get(&AgentProvider::GeminiCli),
            Some(&"gemini-3".to_string())
        );

        prefs.set(&AgentProvider::GeminiCli, None);
        assert_eq!(prefs.get(&AgentProvider::GeminiCli), None);
    }

    #[test]
    fn test_providers_without_acp_models_declare_fallbacks() {
        // Discovery short-circuits on models_via_acp = false, so those
        // providers must ship a curated list or the selector goes empty
        for provider in AgentProvider::ALL {
            let descriptor = provider.descriptor();
            assert!(
                descriptor.models_via_acp || !descriptor.fallback_models.is_empty(),
                "{provider:?} reports no models via ACP but has no fallback_models"
            );
        }
    }

    #[test]
    fn test_descriptor_id_matches_serde_string_for_all_providers() {
        for provider in AgentProvider::ALL {
            let serde_string = serde_json::to_value(provider).unwrap();
            assert_eq!(
                serde_string.as_str().unwrap(),
                provider.descriptor().id,
                "descriptor id drifted from serde representation for {provider:?}"
            );
        }
    }
}
