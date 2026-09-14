//! `session/new` handling that tolerates both ACP model-selection APIs.
//!
//! ACP moved model selection into session config options (a `model` selector
//! in `configOptions`, switched with `session/set_config_option`). The schema
//! crate has dropped the older `models` list and `session/set_model`, but
//! claude-code-acp 0.16 — the latest release and our bundled sidecar — still
//! speaks only the old API. codex-acp 1.11 sends both. This module reads
//! whichever the agent offers and issues the matching switch request.

use std::path::PathBuf;

use agent_client_protocol::schema::v1::{
    NewSessionRequest, NewSessionResponse, SessionConfigKind, SessionConfigOption,
    SessionConfigOptionCategory, SessionConfigSelectOption, SessionConfigSelectOptions, SessionId,
    SetSessionConfigOptionRequest,
};
use agent_client_protocol::{Agent, ConnectionTo, JsonRpcRequest};
use serde::{Deserialize, Serialize};
use tracing::debug;

use crate::types::{AgentProvider, ModelInfo};

/// Conventional config option id of the model selector; agents may also mark
/// theirs with `category: "model"` under another id.
const MODEL_CONFIG_ID: &str = "model";

/// `session/new` with the raw response kept, so the deprecated `models` list
/// can be read alongside the typed [`NewSessionResponse`].
#[derive(Debug, Clone, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "session/new", response = serde_json::Value)]
#[serde(transparent)]
struct RawNewSessionRequest(NewSessionRequest);

/// Deprecated `session/set_model`, for agents without config options.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, JsonRpcRequest)]
#[request(method = "session/set_model", response = serde_json::Value)]
#[serde(rename_all = "camelCase")]
struct LegacySetSessionModelRequest {
    session_id: SessionId,
    model_id: String,
}

/// Deprecated `models` field of the `session/new` response.
#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyModelState {
    current_model_id: String,
    available_models: Vec<LegacyModel>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
struct LegacyModel {
    model_id: String,
    name: String,
}

#[derive(Deserialize)]
struct LegacyFields {
    #[serde(default)]
    models: Option<LegacyModelState>,
}

/// What `session/new` told us, including model selection on either API.
#[derive(Debug)]
pub struct SessionSetup {
    pub session_id: SessionId,
    response: NewSessionResponse,
    legacy_models: Option<LegacyModelState>,
}

/// A model switch on whichever API the agent speaks.
#[derive(Debug, Clone, PartialEq)]
enum ModelSwitch {
    ConfigOption(SetSessionConfigOptionRequest),
    Legacy(LegacySetSessionModelRequest),
}

impl SessionSetup {
    pub fn parse(raw: serde_json::Value) -> anyhow::Result<Self> {
        let malformed =
            |e: serde_json::Error| anyhow::anyhow!("Malformed session/new response: {e}");
        let legacy: LegacyFields = serde_json::from_value(raw.clone()).map_err(malformed)?;
        let response: NewSessionResponse = serde_json::from_value(raw).map_err(malformed)?;
        Ok(Self {
            session_id: response.session_id.clone(),
            response,
            legacy_models: legacy.models,
        })
    }

    /// The session's model selector: a select-kind config option, found by
    /// id or category. Anything else can't be listed or switched, so it is
    /// ignored in favour of the legacy list.
    fn model_config_option(&self) -> Option<&SessionConfigOption> {
        self.response
            .config_options
            .as_ref()?
            .iter()
            .filter(|option| matches!(option.kind, SessionConfigKind::Select(_)))
            .find(|option| {
                option.id.0.as_ref() == MODEL_CONFIG_ID
                    || option.category == Some(SessionConfigOptionCategory::Model)
            })
    }

    /// Models offered for selection: the config option when present, else the
    /// legacy list. Codex's legacy ids carry a `[effort]` suffix per reasoning
    /// level; ThoughtTree selects effort separately, so those collapse to one.
    /// (Claude's `opus[1m]` is a distinct model and is kept as is.)
    pub fn available_models(&self, provider: &AgentProvider) -> Vec<ModelInfo> {
        if let Some(option) = self.model_config_option() {
            return select_entries(option)
                .into_iter()
                .map(|entry| ModelInfo {
                    model_id: entry.value.0.to_string(),
                    display_name: entry.name.clone(),
                })
                .collect();
        }
        let collapse_effort_variants = matches!(provider, AgentProvider::Codex);
        let mut models = Vec::<ModelInfo>::new();
        for model in self.legacy_models.iter().flat_map(|s| &s.available_models) {
            let (id, display_name) = match model.model_id.split_once('[') {
                // The per-effort names ("5.5 (low)") don't fit a collapsed entry.
                Some((base, _)) if collapse_effort_variants => (base, base),
                _ => (model.model_id.as_str(), model.name.as_str()),
            };
            if !models.iter().any(|m| m.model_id == id) {
                models.push(ModelInfo {
                    model_id: id.to_string(),
                    display_name: display_name.to_string(),
                });
            }
        }
        models
    }

    /// Model the session starts with, if the agent said.
    pub fn current_model(&self) -> Option<String> {
        if let Some(SessionConfigKind::Select(select)) =
            self.model_config_option().map(|option| &option.kind)
        {
            return Some(select.current_value.0.to_string());
        }
        self.legacy_models
            .as_ref()
            .map(|state| state.current_model_id.clone())
    }

    /// Whether the agent exposes model switching on either API.
    pub fn offers_model_switch(&self) -> bool {
        self.model_config_option().is_some() || self.legacy_models.is_some()
    }

    fn model_switch(&self, model: &str) -> Option<ModelSwitch> {
        if let Some(option) = self.model_config_option() {
            return Some(ModelSwitch::ConfigOption(
                SetSessionConfigOptionRequest::new(
                    self.session_id.clone(),
                    option.id.clone(),
                    model,
                ),
            ));
        }
        self.legacy_models.as_ref().map(|_| {
            ModelSwitch::Legacy(LegacySetSessionModelRequest {
                session_id: self.session_id.clone(),
                model_id: model.to_string(),
            })
        })
    }
}

/// Selectable values of a config option, flattened across groups.
fn select_entries(option: &SessionConfigOption) -> Vec<&SessionConfigSelectOption> {
    match &option.kind {
        SessionConfigKind::Select(select) => match &select.options {
            SessionConfigSelectOptions::Ungrouped(entries) => entries.iter().collect(),
            SessionConfigSelectOptions::Grouped(groups) => groups
                .iter()
                .flat_map(|group| group.options.iter())
                .collect(),
            _ => vec![],
        },
        _ => vec![],
    }
}

/// Create a session in `cwd`. Must run outside the dispatch loop (the
/// `connect_with` foreground is fine).
pub async fn new_session(
    cx: &ConnectionTo<Agent>,
    cwd: impl Into<PathBuf>,
) -> anyhow::Result<SessionSetup> {
    let raw = cx
        .send_request(RawNewSessionRequest(NewSessionRequest::new(cwd)))
        .block_task()
        .await
        .map_err(|e| anyhow::anyhow!("Failed to create session: {e:?}"))?;
    let setup = SessionSetup::parse(raw)?;
    debug!(
        "session/new: config options {:?}, legacy models {:?}",
        setup.response.config_options, setup.legacy_models
    );
    Ok(setup)
}

/// Switch the session to `model` on whichever API the agent speaks.
pub async fn set_model(
    cx: &ConnectionTo<Agent>,
    setup: &SessionSetup,
    model: &str,
) -> anyhow::Result<()> {
    let result = match setup.model_switch(model) {
        Some(ModelSwitch::ConfigOption(request)) => {
            cx.send_request(request).block_task().await.map(|_| ())
        }
        Some(ModelSwitch::Legacy(request)) => {
            cx.send_request(request).block_task().await.map(|_| ())
        }
        None => anyhow::bail!("Agent does not support model switching"),
    };
    result.map_err(|e| anyhow::anyhow!("Failed to set model: {e:?}"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use agent_client_protocol::JsonRpcMessage;

    fn setup(json: serde_json::Value) -> SessionSetup {
        SessionSetup::parse(json).unwrap()
    }

    fn ids(models: &[ModelInfo]) -> Vec<&str> {
        models.iter().map(|m| m.model_id.as_str()).collect()
    }

    #[test]
    fn prefers_model_config_option_over_legacy_list() {
        // Minimized codex-acp 1.11.0 session/new response: it sends both.
        let setup = setup(serde_json::json!({
            "sessionId": "s1",
            "models": {
                "currentModelId": "gpt-6-astra[high]",
                "availableModels": [
                    {"modelId": "gpt-6-astra[low]", "name": "Astra (low)"},
                    {"modelId": "gpt-6-astra[high]", "name": "Astra (high)"}
                ]
            },
            "configOptions": [{
                "id": "model", "name": "Model", "category": "model", "type": "select",
                "currentValue": "gpt-6-astra",
                "options": [
                    {"value": "gpt-6-astra", "name": "6 Astra"},
                    {"value": "gpt-future", "name": "Future model"}
                ]
            }]
        }));

        let models = setup.available_models(&AgentProvider::Codex);
        assert_eq!(ids(&models), ["gpt-6-astra", "gpt-future"]);
        assert_eq!(models[1].display_name, "Future model");
        assert_eq!(setup.current_model().as_deref(), Some("gpt-6-astra"));
        assert!(setup.offers_model_switch());
        assert_eq!(
            setup.model_switch("gpt-future"),
            Some(ModelSwitch::ConfigOption(
                SetSessionConfigOptionRequest::new("s1", "model", "gpt-future")
            ))
        );
    }

    #[test]
    fn falls_back_to_legacy_models_and_set_model() {
        // claude-code-acp 0.16 shape: legacy list only.
        let setup = setup(serde_json::json!({
            "sessionId": "s1",
            "models": {
                "currentModelId": "claude-opus-4-5",
                "availableModels": [
                    {"modelId": "claude-opus-4-5", "name": "Opus 4.5"},
                    {"modelId": "opus[1m]", "name": "Opus (1M context)"},
                    {"modelId": "claude-haiku-4-5", "name": "Haiku 4.5"}
                ]
            }
        }));

        let models = setup.available_models(&AgentProvider::ClaudeCode);
        assert_eq!(
            ids(&models),
            ["claude-opus-4-5", "opus[1m]", "claude-haiku-4-5"]
        );
        assert_eq!(models[0].display_name, "Opus 4.5");
        assert_eq!(setup.current_model().as_deref(), Some("claude-opus-4-5"));
        assert_eq!(
            setup.model_switch("claude-haiku-4-5"),
            Some(ModelSwitch::Legacy(LegacySetSessionModelRequest {
                session_id: SessionId::new("s1"),
                model_id: "claude-haiku-4-5".to_string(),
            }))
        );
    }

    #[test]
    fn legacy_codex_effort_variants_collapse_to_one_model() {
        let setup = setup(serde_json::json!({
            "sessionId": "s1",
            "models": {"currentModelId": "gpt-5.5[high]", "availableModels": [
                {"modelId": "gpt-5.5[low]", "name": "5.5 (low)"},
                {"modelId": "gpt-5.5[high]", "name": "5.5 (high)"}
            ]}
        }));
        let models = setup.available_models(&AgentProvider::Codex);
        assert_eq!(ids(&models), ["gpt-5.5"]);
        assert_eq!(models[0].display_name, "gpt-5.5");
    }

    #[test]
    fn model_option_under_another_id_is_switched_by_that_id() {
        let setup = setup(serde_json::json!({
            "sessionId": "s1",
            "configOptions": [{
                "id": "llm", "name": "Model", "category": "model", "type": "select",
                "currentValue": "a",
                "options": [{"value": "a", "name": "A"}]
            }]
        }));
        assert_eq!(ids(&setup.available_models(&AgentProvider::Codex)), ["a"]);
        assert_eq!(
            setup.model_switch("a"),
            Some(ModelSwitch::ConfigOption(
                SetSessionConfigOptionRequest::new("s1", "llm", "a")
            ))
        );
    }

    #[test]
    fn non_select_model_option_does_not_hide_legacy_models() {
        let setup = setup(serde_json::json!({
            "sessionId": "s1",
            "configOptions": [{
                "id": "model", "name": "Model", "category": "model", "type": "boolean",
                "currentValue": true
            }],
            "models": {"currentModelId": "sonnet", "availableModels": [
                {"modelId": "sonnet", "name": "Sonnet"}
            ]}
        }));
        assert_eq!(
            ids(&setup.available_models(&AgentProvider::ClaudeCode)),
            ["sonnet"]
        );
        assert!(matches!(
            setup.model_switch("sonnet"),
            Some(ModelSwitch::Legacy(_))
        ));
    }

    #[test]
    fn flattens_grouped_model_options() {
        let setup = setup(serde_json::json!({
            "sessionId": "s1",
            "configOptions": [{
                "id": "model", "name": "Model", "category": "model", "type": "select",
                "currentValue": "a",
                "options": [
                    {"group": "fast", "name": "Fast", "options": [{"value": "a", "name": "A"}]},
                    {"group": "smart", "name": "Smart", "options": [{"value": "b", "name": "B"}]}
                ]
            }]
        }));
        assert_eq!(
            ids(&setup.available_models(&AgentProvider::ClaudeCode)),
            ["a", "b"]
        );
    }

    #[test]
    fn session_without_model_api_offers_nothing() {
        let setup = setup(serde_json::json!({"sessionId": "s1"}));
        assert!(setup.available_models(&AgentProvider::Codex).is_empty());
        assert_eq!(setup.current_model(), None);
        assert!(!setup.offers_model_switch());
        assert_eq!(setup.model_switch("x"), None);
    }

    #[test]
    fn raw_new_session_request_matches_typed_wire_format() {
        let typed = NewSessionRequest::new("/notes");
        let raw = RawNewSessionRequest(typed.clone());
        assert_eq!(raw.method(), "session/new");
        assert_eq!(
            serde_json::to_value(&raw).unwrap(),
            serde_json::to_value(&typed).unwrap()
        );
    }

    #[test]
    fn legacy_set_model_request_uses_deprecated_wire_format() {
        let request = LegacySetSessionModelRequest {
            session_id: SessionId::new("s1"),
            model_id: "sonnet".to_string(),
        };
        assert_eq!(request.method(), "session/set_model");
        assert_eq!(
            serde_json::to_value(&request).unwrap(),
            serde_json::json!({"sessionId": "s1", "modelId": "sonnet"})
        );
    }
}
