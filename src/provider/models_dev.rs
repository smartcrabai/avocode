use crate::provider::ProviderError;
use crate::provider::schema::{ModelCapabilities, ModelCost, ModelInfo, ModelStatus, ProviderInfo};

/// User-facing flattened representation of a model choice.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModelChoice {
    pub provider_id: String,
    pub model_id: String,
    pub display_name: String,
    pub context_length: Option<u64>,
}

impl ModelChoice {
    /// Returns `"provider_id/model_id"`.
    #[must_use]
    pub fn qualified_id(&self) -> String {
        format!("{}/{}", self.provider_id, self.model_id)
    }
}

/// Strict dynamic loader: returns only API or TTL-valid cache data.
///
/// # Errors
///
/// Returns [`ProviderError::EmptyCatalog`] when the catalog is empty or invalid.
/// Returns other [`ProviderError`] variants on network or IO failure.
pub async fn fetch_dynamic_providers() -> Result<Vec<ProviderInfo>, ProviderError> {
    if let Some(providers) = try_cache()
        && !providers.is_empty()
    {
        return Ok(providers);
    }

    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(10))
        .build()?;
    let resp = client
        .get("https://models.dev/api.json")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    let providers = parse_api_response_strict(&resp)?;
    let _ = write_cache(&providers);

    Ok(providers)
}

/// Convert a provider list into a deterministically sorted, flat `ModelChoice` list.
/// Sort order: `provider_id` ascending, then `model_id` ascending.
#[must_use]
pub fn to_model_choices(providers: &[ProviderInfo]) -> Vec<ModelChoice> {
    let mut choices: Vec<ModelChoice> = providers
        .iter()
        .flat_map(|p| {
            p.models.iter().map(|m| ModelChoice {
                provider_id: p.id.clone(),
                model_id: m.id.clone(),
                display_name: m.name.clone(),
                context_length: m.context_length,
            })
        })
        .collect();
    choices.sort_by(|a, b| {
        a.provider_id
            .cmp(&b.provider_id)
            .then(a.model_id.cmp(&b.model_id))
    });
    choices
}

fn parse_providers_from_object(
    obj: &serde_json::Map<String, serde_json::Value>,
) -> Vec<ProviderInfo> {
    let mut providers = Vec::new();

    for (provider_key, provider_val) in obj {
        let id = provider_val
            .get("id")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(provider_key)
            .to_string();

        let name = provider_val
            .get("name")
            .and_then(serde_json::Value::as_str)
            .unwrap_or(&id)
            .to_string();

        let env: Vec<String> = provider_val
            .get("env")
            .and_then(serde_json::Value::as_array)
            .map(|arr| {
                arr.iter()
                    .filter_map(|e| e.as_str().map(str::to_string))
                    .collect()
            })
            .unwrap_or_default();

        let Some(models_val) = provider_val
            .get("models")
            .and_then(serde_json::Value::as_object)
        else {
            providers.push(ProviderInfo {
                id,
                name,
                env,
                models: vec![],
            });
            continue;
        };

        let models: Vec<ModelInfo> = models_val
            .iter()
            .map(|(model_key, model_val)| parse_model(model_key, model_val, &id))
            .collect();

        providers.push(ProviderInfo {
            id,
            name,
            env,
            models,
        });
    }

    providers
}

fn parse_api_response_strict(
    value: &serde_json::Value,
) -> Result<Vec<ProviderInfo>, ProviderError> {
    let Some(obj) = value.as_object() else {
        return Err(ProviderError::EmptyCatalog);
    };

    if obj.is_empty() {
        return Err(ProviderError::EmptyCatalog);
    }

    Ok(parse_providers_from_object(obj))
}

fn cache_path() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("avocode")
        .join("models.json")
}

fn try_cache() -> Option<Vec<ProviderInfo>> {
    let path = cache_path();
    let age = std::fs::metadata(&path)
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .ok()?;
    if age > std::time::Duration::from_secs(86400) {
        return None;
    }
    let data = std::fs::read_to_string(&path).ok()?;
    serde_json::from_str(&data).ok()
}

fn write_cache(providers: &[ProviderInfo]) -> Result<(), ProviderError> {
    let path = cache_path();
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let data = serde_json::to_string(providers)?;
    std::fs::write(&path, data)?;
    Ok(())
}

fn parse_model(model_key: &str, model_val: &serde_json::Value, provider_id: &str) -> ModelInfo {
    let model_id = model_val
        .get("id")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(model_key)
        .to_string();

    let model_name = model_val
        .get("name")
        .and_then(serde_json::Value::as_str)
        .unwrap_or(&model_id)
        .to_string();

    let family = model_val
        .get("family")
        .and_then(serde_json::Value::as_str)
        .map(str::to_string);

    let modalities_input: Vec<&str> = model_val
        .get("modalities")
        .and_then(|m| m.get("input"))
        .and_then(serde_json::Value::as_array)
        .map(|arr| {
            arr.iter()
                .filter_map(serde_json::Value::as_str)
                .collect::<Vec<_>>()
        })
        .unwrap_or_default();

    let vision = modalities_input.contains(&"image");

    let capabilities = ModelCapabilities {
        tools: model_val
            .get("tool_call")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        vision,
        reasoning: model_val
            .get("reasoning")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        streaming: true,
        temperature: model_val
            .get("temperature")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(true),
        attachment: model_val
            .get("attachment")
            .and_then(serde_json::Value::as_bool)
            .unwrap_or(false),
        computer_use: false,
    };

    let cost_val = model_val.get("cost");
    let cost = ModelCost {
        input: cost_val
            .and_then(|c| c.get("input"))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
        output: cost_val
            .and_then(|c| c.get("output"))
            .and_then(serde_json::Value::as_f64)
            .unwrap_or(0.0),
        cache_read: cost_val
            .and_then(|c| c.get("cache_read"))
            .and_then(serde_json::Value::as_f64),
        cache_write: cost_val
            .and_then(|c| c.get("cache_write"))
            .and_then(serde_json::Value::as_f64),
    };

    let limit_val = model_val.get("limit");
    let context_length = limit_val
        .and_then(|l| l.get("context"))
        .and_then(serde_json::Value::as_u64);
    let output_length = limit_val
        .and_then(|l| l.get("output"))
        .and_then(serde_json::Value::as_u64);

    ModelInfo {
        id: model_id,
        name: model_name,
        provider_id: provider_id.to_string(),
        family,
        capabilities,
        cost,
        context_length,
        output_length,
        status: ModelStatus::Active,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cache_path_not_empty() {
        let path = cache_path();
        assert!(path.to_str().is_some_and(|s| !s.is_empty()));
        assert!(path.ends_with("models.json"));
    }

    // ---- parse_api_response_strict tests ----

    #[test]
    fn test_parse_api_response_strict_empty_object_returns_error() {
        let value = serde_json::json!({});
        let result = parse_api_response_strict(&value);
        assert!(
            matches!(result, Err(ProviderError::EmptyCatalog)),
            "expected EmptyCatalog, got {result:?}"
        );
    }

    #[test]
    fn test_parse_api_response_strict_non_object_returns_error() {
        let value = serde_json::json!(null);
        let result = parse_api_response_strict(&value);
        assert!(result.is_err(), "expected Err but got Ok");
    }

    #[test]
    fn test_parse_api_response_strict_valid_provider_returns_ok() {
        let value = serde_json::json!({
            "openai": {
                "id": "openai",
                "name": "OpenAI",
                "models": {
                    "gpt-4o": { "id": "gpt-4o", "name": "GPT-4o" }
                }
            }
        });
        let result = parse_api_response_strict(&value);
        let providers = result.unwrap_or_else(|e| panic!("should parse valid response: {e}"));
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "openai");
        assert_eq!(providers[0].models.len(), 1);
    }

    // ---- to_model_choices tests ----

    fn make_provider(id: &str, model_ids: &[&str]) -> ProviderInfo {
        let models = model_ids
            .iter()
            .map(|&mid| ModelInfo {
                id: mid.to_string(),
                name: mid.to_string(),
                provider_id: id.to_string(),
                family: None,
                capabilities: ModelCapabilities::default(),
                cost: ModelCost::default(),
                context_length: None,
                output_length: None,
                status: ModelStatus::Active,
            })
            .collect();
        ProviderInfo {
            id: id.to_string(),
            name: id.to_string(),
            env: vec![],
            models,
        }
    }

    #[test]
    fn test_to_model_choices_qualified_id_format() {
        let providers = vec![make_provider("anthropic", &["claude-3-5-sonnet"])];
        let choices = to_model_choices(&providers);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].qualified_id(), "anthropic/claude-3-5-sonnet");
        assert_eq!(choices[0].provider_id, "anthropic");
        assert_eq!(choices[0].model_id, "claude-3-5-sonnet");
    }

    #[test]
    fn test_to_model_choices_sorted_deterministically() {
        let providers = vec![
            make_provider("openai", &["gpt-4o", "gpt-3.5-turbo"]),
            make_provider("anthropic", &["claude-opus-4", "claude-sonnet-4"]),
        ];
        let choices1 = to_model_choices(&providers);
        let choices2 = to_model_choices(&providers);
        assert_eq!(choices1.len(), choices2.len());
        for (a, b) in choices1.iter().zip(choices2.iter()) {
            assert_eq!(a.qualified_id(), b.qualified_id());
        }
        // sorted: provider_id ascending, then model_id ascending
        assert_eq!(choices1[0].provider_id, "anthropic");
        assert_eq!(choices1[0].model_id, "claude-opus-4");
        assert_eq!(choices1[1].model_id, "claude-sonnet-4");
        assert_eq!(choices1[2].provider_id, "openai");
        assert_eq!(choices1[2].model_id, "gpt-3.5-turbo");
        assert_eq!(choices1[3].model_id, "gpt-4o");
    }

    #[test]
    fn test_to_model_choices_same_model_id_different_providers_get_separate_entries() {
        let providers = vec![
            make_provider("openai", &["gpt-4o"]),
            make_provider("azure", &["gpt-4o"]),
        ];
        let choices = to_model_choices(&providers);
        assert_eq!(choices.len(), 2);
        assert!(choices.iter().any(|c| c.qualified_id() == "azure/gpt-4o"));
        assert!(choices.iter().any(|c| c.qualified_id() == "openai/gpt-4o"));
    }

    #[test]
    fn test_to_model_choices_empty_providers_returns_empty() {
        let providers: Vec<ProviderInfo> = vec![];
        let choices = to_model_choices(&providers);
        assert!(choices.is_empty());
    }

    #[test]
    fn test_to_model_choices_provider_without_models_skipped() {
        let providers = vec![
            make_provider("empty-provider", &[]),
            make_provider("anthropic", &["claude-opus-4"]),
        ];
        let choices = to_model_choices(&providers);
        assert_eq!(choices.len(), 1);
        assert_eq!(choices[0].provider_id, "anthropic");
    }
}
