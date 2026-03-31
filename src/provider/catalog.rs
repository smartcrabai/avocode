use std::collections::HashMap;

use crate::auth::store::AuthInfo;
use crate::config::schema::Config;
use crate::provider::schema::{ModelInfo, ProviderInfo};

/// How models should be fetched from this provider's API.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FetchStrategy {
    /// OpenAI-compatible `GET /v1/models`
    OpenAiCompatible,
    /// Anthropic native `GET /v1/models`
    AnthropicNative,
    /// Google Generative AI `models.list`
    GoogleGenerativeAi,
    /// No known public list API; use curated fallback
    Unsupported,
}

/// Static descriptor for a provider.  Does not contain a model list.
#[derive(Debug, Clone)]
pub struct ProviderDescriptor {
    pub id: String,
    pub name: String,
    /// Environment variable names that may hold an API key for this provider.
    pub env_keys: Vec<String>,
    pub default_base_url: Option<String>,
    pub fetch_strategy: FetchStrategy,
}

/// How the active credential for a connection was discovered.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConnectionSource {
    /// API key found in an environment variable (key name included).
    Env(String),
    /// API key found in `config.provider.<id>.api_key`.
    Config,
    /// OAuth / `WellKnown` token found in `AuthStore`.
    OAuth,
}

/// A provider that has been confirmed to have usable credentials.
#[derive(Debug, Clone)]
pub struct ProviderConnection {
    pub descriptor: ProviderDescriptor,
    pub source: ConnectionSource,
    /// Resolved API key, if the credential is key-based.
    pub api_key: Option<String>,
    /// Base URL override from config, if any.
    pub base_url: Option<String>,
}

/// Outcome of fetching models for one provider.
#[derive(Debug)]
pub struct ProviderCatalogEntry {
    pub provider_id: String,
    pub models: Vec<ModelInfo>,
    /// Non-fatal fetch error for this provider only.
    pub fetch_error: Option<String>,
}

/// Errors that can occur when resolving which model to use for a session.
#[derive(Debug, thiserror::Error, PartialEq, Eq)]
pub enum ModelResolutionError {
    #[error("Model not found: {0}")]
    ModelNotFound(String),
    #[error("No connected providers available")]
    NoConnectedProviders,
}

/// Resolve connected providers from descriptors, config, and auth entries.
///
/// A provider is "connected" when **any** of the following is true:
/// 1. An env var in `descriptor.env_keys` is set.
/// 2. `config.provider.<id>.api_key` is non-empty.
/// 3. `auth_entries` contains an entry for `<id>`.
///
/// Providers listed in `config.disabled_providers` are unconditionally excluded.
#[must_use]
pub fn resolve_connections<S: ::std::hash::BuildHasher>(
    descriptors: &[ProviderDescriptor],
    config: &Config,
    auth_entries: &HashMap<String, AuthInfo, S>,
) -> Vec<ProviderConnection> {
    let mut connections = Vec::new();

    for descriptor in descriptors {
        if config.disabled_providers.contains(&descriptor.id) {
            continue;
        }

        let provider_config = config.provider.get(&descriptor.id);
        let base_url = provider_config.and_then(|c| c.base_url.clone());

        let mut found = false;
        for env_key in &descriptor.env_keys {
            if let Ok(val) = std::env::var(env_key)
                && !val.is_empty()
            {
                connections.push(ProviderConnection {
                    descriptor: descriptor.clone(),
                    source: ConnectionSource::Env(env_key.clone()),
                    api_key: Some(val),
                    base_url: base_url.clone(),
                });
                found = true;
                break;
            }
        }
        if found {
            continue;
        }

        if let Some(api_key) = provider_config.and_then(|c| c.api_key.as_ref())
            && !api_key.is_empty()
        {
            connections.push(ProviderConnection {
                descriptor: descriptor.clone(),
                source: ConnectionSource::Config,
                api_key: Some(api_key.clone()),
                base_url,
            });
            continue;
        }

        if auth_entries.contains_key(&descriptor.id) {
            connections.push(ProviderConnection {
                descriptor: descriptor.clone(),
                source: ConnectionSource::OAuth,
                api_key: None,
                base_url,
            });
        }
    }

    connections
}

/// Parse an OpenAI-compatible `GET /v1/models` JSON response.
///
/// Expected structure:
/// ```json
/// { "data": [ { "id": "gpt-4o" }, … ] }
/// ```
///
/// Returns an empty list for unrecognised or malformed input.
#[must_use]
pub fn parse_openai_models_response(provider_id: &str, json: &serde_json::Value) -> Vec<ModelInfo> {
    let Some(data) = json.get("data").and_then(|v| v.as_array()) else {
        return vec![];
    };

    data.iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?;
            Some(ModelInfo {
                id: id.to_string(),
                name: id.to_string(),
                provider_id: provider_id.to_string(),
                family: None,
                capabilities: crate::provider::schema::ModelCapabilities::default(),
                cost: crate::provider::schema::ModelCost::default(),
                context_length: None,
                output_length: None,
                status: crate::provider::schema::ModelStatus::Active,
            })
        })
        .collect()
}

/// Parse an Anthropic `GET /v1/models` JSON response.
///
/// Expected structure:
/// ```json
/// { "data": [ { "id": "claude-…", "display_name": "Claude …" }, … ] }
/// ```
///
/// Returns an empty list for unrecognised or malformed input.
#[must_use]
pub fn parse_anthropic_models_response(
    provider_id: &str,
    json: &serde_json::Value,
) -> Vec<ModelInfo> {
    let Some(data) = json.get("data").and_then(|v| v.as_array()) else {
        return vec![];
    };

    data.iter()
        .filter_map(|item| {
            let id = item.get("id")?.as_str()?;
            let name = item
                .get("display_name")
                .and_then(|v| v.as_str())
                .unwrap_or(id);
            Some(ModelInfo {
                id: id.to_string(),
                name: name.to_string(),
                provider_id: provider_id.to_string(),
                family: None,
                capabilities: crate::provider::schema::ModelCapabilities::default(),
                cost: crate::provider::schema::ModelCost::default(),
                context_length: None,
                output_length: None,
                status: crate::provider::schema::ModelStatus::Active,
            })
        })
        .collect()
}

/// Parse a Google Generative AI `models.list` JSON response.
///
/// Expected structure:
/// ```json
/// { "models": [ { "name": "models/gemini-…", "displayName": "…" }, … ] }
/// ```
///
/// The `models/` prefix is stripped from the `name` field to derive the model id.
/// Returns an empty list for unrecognised or malformed input.
#[must_use]
pub fn parse_google_models_response(provider_id: &str, json: &serde_json::Value) -> Vec<ModelInfo> {
    let Some(models) = json.get("models").and_then(|v| v.as_array()) else {
        return vec![];
    };

    models
        .iter()
        .filter_map(|item| {
            let raw_name = item.get("name")?.as_str()?;
            let id = raw_name.strip_prefix("models/").unwrap_or(raw_name);
            let name = item
                .get("displayName")
                .and_then(|v| v.as_str())
                .unwrap_or(id);
            let context_length = item
                .get("inputTokenLimit")
                .and_then(serde_json::Value::as_u64);
            let output_length = item
                .get("outputTokenLimit")
                .and_then(serde_json::Value::as_u64);
            Some(ModelInfo {
                id: id.to_string(),
                name: name.to_string(),
                provider_id: provider_id.to_string(),
                family: None,
                capabilities: crate::provider::schema::ModelCapabilities::default(),
                cost: crate::provider::schema::ModelCost::default(),
                context_length,
                output_length,
                status: crate::provider::schema::ModelStatus::Active,
            })
        })
        .collect()
}

/// Apply per-model disable overrides from config to a fetched model list.
///
/// Any model whose id appears in `config.provider.<provider_id>.models` with
/// `disabled = true` is removed from the result.
#[must_use]
pub fn apply_model_overrides(
    models: Vec<ModelInfo>,
    provider_id: &str,
    config: &Config,
) -> Vec<ModelInfo> {
    let Some(provider_config) = config.provider.get(provider_id) else {
        return models;
    };

    models
        .into_iter()
        .filter(|model| {
            provider_config
                .models
                .get(&model.id)
                .is_none_or(|override_| !override_.disabled)
        })
        .collect()
}

/// Resolve the model to use for a session.
///
/// Priority order:
/// 1. `preferred` (from CLI arg or `config.model`)
/// 2. First active model from the first provider in `catalog` (deterministic)
///
/// # Errors
///
/// - [`ModelResolutionError::ModelNotFound`] when `preferred` is given but absent from catalog.
/// - [`ModelResolutionError::NoConnectedProviders`] when catalog is empty and no preference given.
pub fn resolve_default_model<'a>(
    preferred: Option<&str>,
    catalog: &'a [ProviderInfo],
) -> Result<&'a ModelInfo, ModelResolutionError> {
    if let Some(model_id) = preferred {
        // Search all providers in the catalog for the requested model
        for provider in catalog {
            if let Some(model) = provider.models.iter().find(|m| m.id == model_id) {
                return Ok(model);
            }
        }
        return Err(ModelResolutionError::ModelNotFound(model_id.to_string()));
    }

    catalog
        .iter()
        .flat_map(|p| p.models.iter())
        .next()
        .ok_or(ModelResolutionError::NoConnectedProviders)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{ModelOverride, ProviderConfig};
    use crate::provider::schema::{ModelCapabilities, ModelCost, ModelStatus};

    // ─── helpers ──────────────────────────────────────────────────────────────

    fn make_descriptor(id: &str, env_key: &str) -> ProviderDescriptor {
        ProviderDescriptor {
            id: id.to_string(),
            name: id.to_string(),
            env_keys: vec![env_key.to_string()],
            default_base_url: None,
            fetch_strategy: FetchStrategy::OpenAiCompatible,
        }
    }

    fn minimal_model(id: &str, provider_id: &str) -> ModelInfo {
        ModelInfo {
            id: id.to_string(),
            name: id.to_string(),
            provider_id: provider_id.to_string(),
            family: None,
            capabilities: ModelCapabilities::default(),
            cost: ModelCost::default(),
            context_length: None,
            output_length: None,
            status: ModelStatus::Active,
        }
    }

    // ─── connection resolver ──────────────────────────────────────────────────

    #[test]
    fn test_resolve_connections_env_only() {
        // Given: env var set for the descriptor; no config or auth entry
        let env_key = "AVOCODE_TEST_CAT_ENV_ONLY_12345";
        let descriptors = vec![ProviderDescriptor {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            env_keys: vec![env_key.to_string()],
            default_base_url: None,
            fetch_strategy: FetchStrategy::AnthropicNative,
        }];
        // SAFETY: test-only env mutation; unique key avoids parallel test interference
        unsafe { std::env::set_var(env_key, "test-key") };
        let config = Config::default();
        let auth_entries = HashMap::new();

        // When: resolve connections
        let connections = resolve_connections(&descriptors, &config, &auth_entries);

        // Then: one connection with source=Env
        // SAFETY: reverting env var set above
        unsafe { std::env::remove_var(env_key) };
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].descriptor.id, "anthropic");
        assert_eq!(
            connections[0].source,
            ConnectionSource::Env(env_key.to_string())
        );
    }

    #[test]
    fn test_resolve_connections_config_api_key_only() {
        // Given: no env var set; config has api_key for openai
        let env_key = "AVOCODE_TEST_CAT_OPENAI_NOT_SET_12345";
        let descriptors = vec![make_descriptor("openai", env_key)];
        let mut config = Config::default();
        let provider_config = ProviderConfig {
            api_key: Some("config-api-key".to_string()),
            ..Default::default()
        };
        config
            .provider
            .insert("openai".to_string(), provider_config);
        let auth_entries = HashMap::new();

        // When: resolve connections
        let connections = resolve_connections(&descriptors, &config, &auth_entries);

        // Then: one connection with source=Config carrying the key
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].descriptor.id, "openai");
        assert_eq!(connections[0].source, ConnectionSource::Config);
        assert_eq!(connections[0].api_key, Some("config-api-key".to_string()));
    }

    #[test]
    fn test_resolve_connections_oauth_only() {
        // Given: no env key, no config key; AuthStore has OAuth entry
        let descriptors = vec![ProviderDescriptor {
            id: "github-copilot".to_string(),
            name: "GitHub Copilot".to_string(),
            env_keys: vec![],
            default_base_url: None,
            fetch_strategy: FetchStrategy::Unsupported,
        }];
        let config = Config::default();
        let mut auth_entries = HashMap::new();
        auth_entries.insert(
            "github-copilot".to_string(),
            AuthInfo::Oauth {
                refresh: Some("refresh-token".to_string()),
                access: "access-token".to_string(),
                expires: None,
                account_id: None,
                enterprise_url: None,
            },
        );

        // When: resolve connections
        let connections = resolve_connections(&descriptors, &config, &auth_entries);

        // Then: one connection with source=OAuth
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].descriptor.id, "github-copilot");
        assert_eq!(connections[0].source, ConnectionSource::OAuth);
    }

    #[test]
    fn test_resolve_connections_disabled_provider_excluded() {
        // Given: provider would be connected via env, but is listed in disabled_providers
        let env_key = "AVOCODE_TEST_CAT_DISABLED_12345";
        let descriptors = vec![make_descriptor("anthropic", env_key)];
        // SAFETY: test-only env mutation; unique key avoids parallel test interference
        unsafe { std::env::set_var(env_key, "some-key") };
        let config = Config {
            disabled_providers: vec!["anthropic".to_string()],
            ..Default::default()
        };
        let auth_entries = HashMap::new();

        // When: resolve connections
        let connections = resolve_connections(&descriptors, &config, &auth_entries);

        // Then: no connections because anthropic is disabled
        // SAFETY: reverting env var set above
        unsafe { std::env::remove_var(env_key) };
        assert!(connections.is_empty(), "disabled provider must be excluded");
    }

    #[test]
    fn test_resolve_connections_no_credentials_returns_empty() {
        // Given: descriptor present but no env var, no config key, no auth entry
        let descriptors = vec![make_descriptor(
            "xai",
            "AVOCODE_TEST_CAT_XAI_NOT_ANYWHERE_12345",
        )];
        let config = Config::default();
        let auth_entries = HashMap::new();

        // When: resolve connections
        let connections = resolve_connections(&descriptors, &config, &auth_entries);

        // Then: no connections
        assert!(connections.is_empty());
    }

    #[test]
    fn test_resolve_connections_multiple_descriptors_partial_match() {
        // Given: two descriptors; only one is connected via env
        let env_key = "AVOCODE_TEST_CAT_MULTI_12345";
        let descriptors = vec![
            make_descriptor("anthropic", env_key),
            make_descriptor("openai", "AVOCODE_TEST_CAT_MULTI_OPENAI_NOT_SET_12345"),
        ];
        // SAFETY: test-only env mutation; unique key avoids parallel test interference
        unsafe { std::env::set_var(env_key, "key-value") };
        let config = Config::default();
        let auth_entries = HashMap::new();

        // When: resolve connections
        let connections = resolve_connections(&descriptors, &config, &auth_entries);

        // Then: only anthropic is connected
        // SAFETY: reverting env var set above
        unsafe { std::env::remove_var(env_key) };
        assert_eq!(connections.len(), 1);
        assert_eq!(connections[0].descriptor.id, "anthropic");
    }

    // ─── OpenAI response parser ───────────────────────────────────────────────

    #[test]
    fn test_parse_openai_models_happy_path() {
        // Given: a valid OpenAI /v1/models JSON response
        let json = serde_json::json!({
            "object": "list",
            "data": [
                { "id": "gpt-4o",      "object": "model", "created": 1_715_367_049, "owned_by": "system" },
                { "id": "gpt-4o-mini", "object": "model", "created": 1_721_172_717, "owned_by": "system" }
            ]
        });

        // When: parse the response
        let models = parse_openai_models_response("openai", &json);

        // Then: two models with correct ids and provider_id
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.id == "gpt-4o"));
        assert!(models.iter().any(|m| m.id == "gpt-4o-mini"));
        assert!(models.iter().all(|m| m.provider_id == "openai"));
    }

    #[test]
    fn test_parse_openai_models_empty_data() {
        // Given: OpenAI response with an empty data array
        let json = serde_json::json!({ "object": "list", "data": [] });

        // When: parse
        let models = parse_openai_models_response("openai", &json);

        // Then: empty result, no panic
        assert!(models.is_empty());
    }

    #[test]
    fn test_parse_openai_models_missing_data_field() {
        // Given: malformed JSON without "data" key
        let json = serde_json::json!({ "object": "list" });

        // When: parse
        let models = parse_openai_models_response("openai", &json);

        // Then: gracefully returns empty
        assert!(models.is_empty());
    }

    // ─── Anthropic response parser ────────────────────────────────────────────

    #[test]
    fn test_parse_anthropic_models_happy_path() {
        // Given: a valid Anthropic /v1/models JSON response
        let json = serde_json::json!({
            "data": [
                {
                    "type": "model",
                    "id": "claude-opus-4-5-20251101",
                    "display_name": "Claude Opus 4.5",
                    "created_at": "2024-09-30T00:00:00Z"
                },
                {
                    "type": "model",
                    "id": "claude-sonnet-4-5-20251101",
                    "display_name": "Claude Sonnet 4.5",
                    "created_at": "2024-10-22T00:00:00Z"
                }
            ],
            "has_more": false
        });

        // When: parse
        let models = parse_anthropic_models_response("anthropic", &json);

        // Then: two models; ids and display names correctly mapped
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.id == "claude-opus-4-5-20251101"));
        assert!(models.iter().any(|m| m.id == "claude-sonnet-4-5-20251101"));
        assert!(models.iter().any(|m| m.name == "Claude Opus 4.5"));
        assert!(models.iter().all(|m| m.provider_id == "anthropic"));
    }

    #[test]
    fn test_parse_anthropic_models_empty_data() {
        // Given: response with empty data array
        let json = serde_json::json!({ "data": [], "has_more": false });

        // When: parse
        let models = parse_anthropic_models_response("anthropic", &json);

        // Then: empty result
        assert!(models.is_empty());
    }

    #[test]
    fn test_parse_anthropic_models_missing_data_field() {
        // Given: malformed JSON without "data" key
        let json = serde_json::json!({ "has_more": false });

        // When: parse
        let models = parse_anthropic_models_response("anthropic", &json);

        // Then: gracefully returns empty
        assert!(models.is_empty());
    }

    // ─── Google response parser ───────────────────────────────────────────────

    #[test]
    fn test_parse_google_models_happy_path() {
        // Given: a valid Google models.list JSON response
        let json = serde_json::json!({
            "models": [
                {
                    "name": "models/gemini-2.0-flash",
                    "displayName": "Gemini 2.0 Flash",
                    "description": "Fast model.",
                    "supportedGenerationMethods": ["generateContent"],
                    "inputTokenLimit": 1_048_576_u64,
                    "outputTokenLimit": 8192_u64
                },
                {
                    "name": "models/gemini-2.5-pro",
                    "displayName": "Gemini 2.5 Pro",
                    "description": "Powerful model.",
                    "supportedGenerationMethods": ["generateContent"],
                    "inputTokenLimit": 1_048_576_u64,
                    "outputTokenLimit": 65_536_u64
                }
            ]
        });

        // When: parse
        let models = parse_google_models_response("google", &json);

        // Then: two models; "models/" prefix stripped from id
        assert_eq!(models.len(), 2);
        assert!(models.iter().any(|m| m.id == "gemini-2.0-flash"));
        assert!(models.iter().any(|m| m.id == "gemini-2.5-pro"));
        assert!(models.iter().any(|m| m.name == "Gemini 2.0 Flash"));
        assert!(models.iter().all(|m| m.provider_id == "google"));
        // context_length mapped from inputTokenLimit
        assert!(models.iter().any(|m| m.context_length == Some(1_048_576)));
    }

    #[test]
    fn test_parse_google_models_empty_list() {
        // Given: empty models array
        let json = serde_json::json!({ "models": [] });

        // When: parse
        let models = parse_google_models_response("google", &json);

        // Then: empty result
        assert!(models.is_empty());
    }

    #[test]
    fn test_parse_google_models_missing_models_field() {
        // Given: malformed JSON without "models" key
        let json = serde_json::json!({});

        // When: parse
        let models = parse_google_models_response("google", &json);

        // Then: gracefully returns empty
        assert!(models.is_empty());
    }

    // ─── post-fetch filter ────────────────────────────────────────────────────

    #[test]
    fn test_apply_model_overrides_removes_disabled_model() {
        // Given: two models; one is disabled in config
        let models = vec![
            minimal_model("gpt-4o", "openai"),
            minimal_model("gpt-4o-mini", "openai"),
        ];
        let mut config = Config::default();
        let mut provider_config = ProviderConfig::default();
        provider_config
            .models
            .insert("gpt-4o-mini".to_string(), ModelOverride { disabled: true });
        config
            .provider
            .insert("openai".to_string(), provider_config);

        // When: apply overrides
        let result = apply_model_overrides(models, "openai", &config);

        // Then: only the non-disabled model remains
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].id, "gpt-4o");
    }

    #[test]
    fn test_apply_model_overrides_no_disabled_keeps_all() {
        // Given: two models; no override
        let models = vec![
            minimal_model("gpt-4o", "openai"),
            minimal_model("gpt-4o-mini", "openai"),
        ];
        let config = Config::default();

        // When: apply overrides
        let result = apply_model_overrides(models, "openai", &config);

        // Then: all models remain
        assert_eq!(result.len(), 2);
    }

    #[test]
    fn test_apply_model_overrides_disabled_false_keeps_model() {
        // Given: model override exists but disabled = false
        let models = vec![minimal_model("gpt-4o", "openai")];
        let mut config = Config::default();
        let mut provider_config = ProviderConfig::default();
        provider_config
            .models
            .insert("gpt-4o".to_string(), ModelOverride { disabled: false });
        config
            .provider
            .insert("openai".to_string(), provider_config);

        // When: apply overrides
        let result = apply_model_overrides(models, "openai", &config);

        // Then: model is not removed
        assert_eq!(result.len(), 1);
    }

    #[test]
    fn test_apply_model_overrides_different_provider_unaffected() {
        // Given: disable override is for "openai" but models belong to "anthropic"
        let models = vec![minimal_model("claude-sonnet-4-5", "anthropic")];
        let mut config = Config::default();
        let mut openai_config = ProviderConfig::default();
        openai_config.models.insert(
            "claude-sonnet-4-5".to_string(),
            ModelOverride { disabled: true },
        );
        config.provider.insert("openai".to_string(), openai_config);

        // When: apply overrides for anthropic
        let result = apply_model_overrides(models, "anthropic", &config);

        // Then: model is kept because the override is scoped to openai
        assert_eq!(result.len(), 1);
    }

    // ─── default model resolution ─────────────────────────────────────────────

    #[test]
    fn test_resolve_default_model_no_preference_picks_first() {
        // Given: catalog with one provider and two models
        let catalog = vec![ProviderInfo {
            id: "anthropic".to_string(),
            name: "Anthropic".to_string(),
            env: vec![],
            models: vec![
                minimal_model("claude-sonnet-4-5", "anthropic"),
                minimal_model("claude-haiku-4-5", "anthropic"),
            ],
        }];

        // When: resolve without preference
        let result = resolve_default_model(None, &catalog);

        // Then: succeeds and returns a model from anthropic
        let model =
            result.unwrap_or_else(|e| panic!("resolve_default_model should succeed: {e:?}"));
        assert_eq!(model.provider_id, "anthropic");
    }

    #[test]
    fn test_resolve_default_model_preferred_model_found() {
        // Given: catalog with gpt-4o and gpt-4o-mini
        let catalog = vec![ProviderInfo {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            env: vec![],
            models: vec![
                minimal_model("gpt-4o", "openai"),
                minimal_model("gpt-4o-mini", "openai"),
            ],
        }];

        // When: resolve with preference for gpt-4o-mini
        let result = resolve_default_model(Some("gpt-4o-mini"), &catalog);

        // Then: returns exactly gpt-4o-mini
        let model =
            result.unwrap_or_else(|e| panic!("resolve_default_model should succeed: {e:?}"));
        assert_eq!(model.id, "gpt-4o-mini");
    }

    #[test]
    fn test_resolve_default_model_preferred_not_found_returns_error() {
        // Given: catalog that does not contain the requested model
        let catalog = vec![ProviderInfo {
            id: "openai".to_string(),
            name: "OpenAI".to_string(),
            env: vec![],
            models: vec![minimal_model("gpt-4o", "openai")],
        }];

        // When: resolve with preference for a non-existent model
        let result = resolve_default_model(Some("nonexistent-model-xyz-12345"), &catalog);

        // Then: ModelNotFound error
        assert!(matches!(
            result,
            Err(ModelResolutionError::ModelNotFound(_))
        ));
    }

    #[test]
    fn test_resolve_default_model_empty_catalog_returns_error() {
        // Given: empty catalog
        let catalog: Vec<ProviderInfo> = vec![];

        // When: resolve without preference
        let result = resolve_default_model(None, &catalog);

        // Then: NoConnectedProviders error
        assert!(matches!(
            result,
            Err(ModelResolutionError::NoConnectedProviders)
        ));
    }

    #[test]
    fn test_resolve_default_model_prefers_explicit_over_first() {
        // Given: catalog with two providers; preference points to the second
        let catalog = vec![
            ProviderInfo {
                id: "anthropic".to_string(),
                name: "Anthropic".to_string(),
                env: vec![],
                models: vec![minimal_model("claude-sonnet-4-5", "anthropic")],
            },
            ProviderInfo {
                id: "openai".to_string(),
                name: "OpenAI".to_string(),
                env: vec![],
                models: vec![minimal_model("gpt-4o", "openai")],
            },
        ];

        // When: resolve with explicit preference for gpt-4o
        let result = resolve_default_model(Some("gpt-4o"), &catalog);

        // Then: returns gpt-4o (not claude-sonnet-4-5)
        let model =
            result.unwrap_or_else(|e| panic!("resolve_default_model should succeed: {e:?}"));
        assert_eq!(model.id, "gpt-4o");
    }

    // ─── per-provider failure isolation ──────────────────────────────────────

    #[test]
    fn test_catalog_entry_fetch_error_is_isolated() {
        // Given: two entries; one successful and one with a fetch error
        let entry_ok = ProviderCatalogEntry {
            provider_id: "anthropic".to_string(),
            models: vec![minimal_model("claude-sonnet-4-5", "anthropic")],
            fetch_error: None,
        };
        let entry_err = ProviderCatalogEntry {
            provider_id: "openai".to_string(),
            models: vec![],
            fetch_error: Some("HTTP 401 Unauthorized".to_string()),
        };

        // When: collect all available models across entries
        let all_models: Vec<&ModelInfo> = [&entry_ok, &entry_err]
            .iter()
            .flat_map(|e| e.models.iter())
            .collect();

        // Then: only the successful provider's models are present; no panic from error entry
        assert_eq!(all_models.len(), 1);
        assert_eq!(all_models[0].provider_id, "anthropic");
        assert!(entry_err.fetch_error.is_some());
    }

    #[test]
    fn test_catalog_entry_all_errors_yields_empty_models() {
        // Given: all entries have fetch errors
        let entries = [
            ProviderCatalogEntry {
                provider_id: "anthropic".to_string(),
                models: vec![],
                fetch_error: Some("timeout".to_string()),
            },
            ProviderCatalogEntry {
                provider_id: "openai".to_string(),
                models: vec![],
                fetch_error: Some("401".to_string()),
            },
        ];

        // When: collect models
        let all_models: Vec<&ModelInfo> = entries.iter().flat_map(|e| e.models.iter()).collect();

        // Then: empty — no panic or unwrap required
        assert!(all_models.is_empty());
    }
}
