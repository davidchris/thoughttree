use serde::{Deserialize, Serialize};

/// Supported agent providers for ACP connections
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "kebab-case")]
pub enum AgentProvider {
    ClaudeCode,
    #[default]
    Codex,
}

/// Static per-provider data. Adding a Provider means adding a variant,
/// a descriptor entry, and a spawn arm — no other code changes.
pub struct ProviderDescriptor {
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
    models_via_acp: true,
    // Fallback for older adapters without discovery. Verified 2026-09-14
    // against the current Codex documentation and codex-acp 1.11.0.
    fallback_models: &[
        ("gpt-6-astra", "GPT-6 Astra"),
        ("gpt-5.6-sol", "GPT-5.6 Sol"),
        ("gpt-5.6-terra", "GPT-5.6 Terra"),
        ("gpt-5.6-luna", "GPT-5.6 Luna"),
        ("gpt-5.5", "GPT-5.5"),
        ("gpt-5.3-codex-spark", "GPT-5.3 Codex Spark"),
    ],
};

impl AgentProvider {
    /// Every supported provider — drives availability lists and config maps
    pub const ALL: &'static [AgentProvider] = &[AgentProvider::ClaudeCode, AgentProvider::Codex];

    pub fn descriptor(&self) -> &'static ProviderDescriptor {
        match self {
            AgentProvider::ClaudeCode => &CLAUDE_CODE_DESCRIPTOR,
            AgentProvider::Codex => &CODEX_DESCRIPTOR,
        }
    }

    /// Human-readable display name for UI
    pub fn display_name(&self) -> &'static str {
        self.descriptor().display_name
    }
}

/// Provider availability status for frontend
#[derive(Clone, Debug, Serialize)]
pub struct ProviderStatus {
    pub provider: AgentProvider,
    pub available: bool,
    pub error_message: Option<String>,
}

/// Model info discovered from ACP CreateSessionResponse.models.available_models
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ModelInfo {
    pub model_id: String,
    pub display_name: String,
}

/// Per-provider config map keyed by descriptor id. Serde-transparent so the
/// on-disk shape stays a plain JSON object; `Option<T>` values keep legacy
/// `null` entries, and String keys keep unknown provider keys from newer
/// app versions.
#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(transparent)]
pub struct PerProvider<T>(std::collections::BTreeMap<String, Option<T>>);

impl<T> Default for PerProvider<T> {
    fn default() -> Self {
        Self(std::collections::BTreeMap::new())
    }
}

impl<T> PerProvider<T> {
    pub fn get(&self, provider: &AgentProvider) -> Option<&T> {
        self.0
            .get(provider.descriptor().id)
            .and_then(|value| value.as_ref())
    }

    pub fn set(&mut self, provider: &AgentProvider, value: Option<T>) {
        self.0.insert(provider.descriptor().id.to_string(), value);
    }
}

/// Unified reasoning-effort scale (see ADR-0002). Serde strings are the
/// cross-language contract with the TS `ReasoningEffort` type.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum ReasoningEffort {
    Low,
    Medium,
    High,
    XHigh,
}

impl ReasoningEffort {
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Medium => "medium",
            Self::High => "high",
            Self::XHigh => "xhigh",
        }
    }
}

/// User's preferred model per provider (stores model_id strings)
pub type ModelPreferences = PerProvider<String>;

/// User's preferred reasoning effort per provider
pub type EffortPreferences = PerProvider<ReasoningEffort>;

/// Custom executable paths for providers (user-configured overrides)
pub type ProviderPaths = PerProvider<String>;

// Message types from frontend (with optional images)
#[derive(Clone, Deserialize)]
pub struct MessageImage {
    pub data: String,
    pub mime_type: String,
}

#[derive(Clone, Deserialize)]
pub struct Message {
    pub role: String,
    pub content: String,
    pub images: Option<Vec<MessageImage>>,
}

#[derive(Clone, Serialize)]
pub struct SummaryResult {
    pub node_id: String,
    pub summary: String,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_provider_default_is_codex() {
        let provider = AgentProvider::default();
        assert_eq!(provider, AgentProvider::Codex);
    }

    #[test]
    fn test_provider_serializes_to_kebab_case() {
        let claude = AgentProvider::ClaudeCode;

        let claude_json = serde_json::to_string(&claude).unwrap();

        assert_eq!(claude_json, "\"claude-code\"");
    }

    #[test]
    fn test_provider_deserializes_from_kebab_case() {
        let claude: AgentProvider = serde_json::from_str("\"claude-code\"").unwrap();

        assert_eq!(claude, AgentProvider::ClaudeCode);
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
        // Older adapters still receive a current fallback catalog.
        let ids: Vec<&str> = AgentProvider::Codex
            .descriptor()
            .fallback_models
            .iter()
            .map(|(id, _)| *id)
            .collect();

        assert_eq!(
            ids,
            vec![
                "gpt-6-astra",
                "gpt-5.6-sol",
                "gpt-5.6-terra",
                "gpt-5.6-luna",
                "gpt-5.5",
                "gpt-5.3-codex-spark"
            ]
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
        assert_eq!(paths.get(&AgentProvider::Codex), None);
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
        assert_eq!(prefs.get(&AgentProvider::Codex), None);

        prefs.set(&AgentProvider::Codex, Some("gpt-6-astra".to_string()));
        assert_eq!(
            prefs.get(&AgentProvider::Codex),
            Some(&"gpt-6-astra".to_string())
        );

        prefs.set(&AgentProvider::Codex, None);
        assert_eq!(prefs.get(&AgentProvider::Codex), None);
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

    #[test]
    fn test_reasoning_effort_serde_contract() {
        let cases = ["low", "medium", "high", "xhigh"];

        for expected in cases {
            let effort =
                serde_json::from_str::<ReasoningEffort>(&format!("\"{expected}\"")).unwrap();
            assert_eq!(effort.as_str(), expected);
            assert_eq!(
                serde_json::to_string(&effort).unwrap(),
                format!("\"{expected}\"")
            );
        }

        assert!(serde_json::from_str::<ReasoningEffort>("\"minimal\"").is_err());
        assert!(serde_json::from_str::<ReasoningEffort>("\"max\"").is_err());

        let json =
            r#"{"claude-code":"low","codex":"xhigh","gemini-cli":null,"future-provider":"high"}"#;
        let preferences: EffortPreferences = serde_json::from_str(json).unwrap();
        assert_eq!(
            preferences.get(&AgentProvider::ClaudeCode),
            Some(&ReasoningEffort::Low)
        );
        assert_eq!(
            preferences.get(&AgentProvider::Codex),
            Some(&ReasoningEffort::XHigh)
        );
        assert_eq!(
            serde_json::to_string(&preferences).unwrap(),
            r#"{"claude-code":"low","codex":"xhigh","future-provider":"high","gemini-cli":null}"#
        );
    }
}
