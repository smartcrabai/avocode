use crate::provider::ProviderError;
use crate::provider::registry::builtin_providers;
use crate::provider::schema::{ModelCapabilities, ModelCost, ModelInfo, ModelStatus, ProviderInfo};

/// Fetch provider/model definitions from models.dev with local file caching (24h TTL).
///
/// # Errors
///
/// Returns [`ProviderError`] if the HTTP request fails and the cache is unavailable.
/// Falls back to [`builtin_providers`] on any parse or network error.
pub async fn fetch_providers() -> Result<Vec<ProviderInfo>, ProviderError> {
    if let Some(cached) = try_cache() {
        return Ok(cached);
    }
    let client = reqwest::Client::new();
    match fetch_from_api(&client).await {
        Ok(providers) => {
            let _ = write_cache(&providers);
            Ok(providers)
        }
        Err(_) => Ok(builtin_providers()),
    }
}

fn cache_path() -> std::path::PathBuf {
    dirs::cache_dir()
        .unwrap_or_else(|| std::path::PathBuf::from("."))
        .join("avocode")
        .join("models.json")
}

fn try_cache() -> Option<Vec<ProviderInfo>> {
    let path = cache_path();
    let data = std::fs::read_to_string(&path).ok()?;
    let age = std::fs::metadata(&path)
        .ok()?
        .modified()
        .ok()?
        .elapsed()
        .ok()?;
    if age > std::time::Duration::from_secs(86400) {
        return None;
    }
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

async fn fetch_from_api(client: &reqwest::Client) -> Result<Vec<ProviderInfo>, ProviderError> {
    let resp = client
        .get("https://models.dev/api.json")
        .send()
        .await?
        .json::<serde_json::Value>()
        .await?;

    Ok(parse_api_response(&resp))
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

fn parse_api_response(value: &serde_json::Value) -> Vec<ProviderInfo> {
    let Some(obj) = value.as_object() else {
        return builtin_providers();
    };

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

    if providers.is_empty() {
        return builtin_providers();
    }

    providers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_api_response_empty_object_falls_back_to_builtin() {
        let value = serde_json::json!({});
        let providers = parse_api_response(&value);
        // Should fall back to builtin providers when response is empty
        assert!(!providers.is_empty());
    }

    #[test]
    fn test_parse_api_response_non_object_falls_back_to_builtin() {
        let value = serde_json::json!(null);
        let providers = parse_api_response(&value);
        assert!(!providers.is_empty());
    }

    #[test]
    fn test_parse_api_response_valid_provider() {
        let value = serde_json::json!({
            "openai": {
                "id": "openai",
                "name": "OpenAI",
                "env": ["OPENAI_API_KEY"],
                "models": {
                    "gpt-4o": {
                        "id": "gpt-4o",
                        "name": "GPT-4o",
                        "family": "gpt",
                        "tool_call": true,
                        "reasoning": false,
                        "temperature": true,
                        "attachment": true,
                        "modalities": {
                            "input": ["text", "image"],
                            "output": ["text"]
                        },
                        "cost": {
                            "input": 2.5,
                            "output": 10.0,
                            "cache_read": 1.25
                        },
                        "limit": {
                            "context": 128_000,
                            "output": 16384
                        }
                    }
                }
            }
        });

        let providers = parse_api_response(&value);
        assert_eq!(providers.len(), 1);
        assert_eq!(providers[0].id, "openai");
        assert_eq!(providers[0].models.len(), 1);
        let model = &providers[0].models[0];
        assert_eq!(model.id, "gpt-4o");
        assert!(model.capabilities.tools);
        assert!(model.capabilities.vision);
        assert!((model.cost.input - 2.5).abs() < f64::EPSILON);
        assert_eq!(model.context_length, Some(128_000));
    }

    #[test]
    fn test_cache_path_not_empty() {
        let path = cache_path();
        assert!(path.to_str().is_some_and(|s| !s.is_empty()));
        assert!(path.ends_with("models.json"));
    }
}
