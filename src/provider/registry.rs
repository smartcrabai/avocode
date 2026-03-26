use crate::provider::schema::{ModelCapabilities, ModelCost, ModelInfo, ModelStatus, ProviderInfo};
use std::collections::HashMap;

pub struct ProviderRegistry {
    providers: HashMap<String, ProviderInfo>,
}

impl ProviderRegistry {
    #[must_use]
    pub fn new(providers: Vec<ProviderInfo>) -> Self {
        let mut map = HashMap::new();
        for p in providers {
            map.insert(p.id.clone(), p);
        }
        Self { providers: map }
    }

    #[must_use]
    pub fn get_provider(&self, id: &str) -> Option<&ProviderInfo> {
        self.providers.get(id)
    }

    #[must_use]
    pub fn get_model(&self, provider_id: &str, model_id: &str) -> Option<&ModelInfo> {
        self.providers
            .get(provider_id)
            .and_then(|p| p.models.iter().find(|m| m.id == model_id))
    }

    #[must_use]
    pub fn list_providers(&self) -> Vec<&ProviderInfo> {
        let mut providers: Vec<&ProviderInfo> = self.providers.values().collect();
        providers.sort_by(|a, b| a.id.cmp(&b.id));
        providers
    }

    #[must_use]
    pub fn list_models(&self, provider_id: &str) -> Vec<&ModelInfo> {
        self.providers
            .get(provider_id)
            .map(|p| p.models.iter().collect())
            .unwrap_or_default()
    }

    #[must_use]
    pub fn all_models(&self) -> Vec<&ModelInfo> {
        self.providers
            .values()
            .flat_map(|p| p.models.iter())
            .collect()
    }

    #[must_use]
    pub fn has_api_key(provider: &ProviderInfo) -> bool {
        provider.env.iter().any(|k| std::env::var(k).is_ok())
    }

    #[must_use]
    pub fn available_providers(&self) -> Vec<&ProviderInfo> {
        self.list_providers()
            .into_iter()
            .filter(|p| Self::has_api_key(p))
            .collect()
    }
}

fn model(
    id: &str,
    name: &str,
    provider_id: &str,
    family: &str,
    capabilities: ModelCapabilities,
    cost: ModelCost,
    limits: (Option<u64>, Option<u64>),
) -> ModelInfo {
    ModelInfo {
        id: id.to_string(),
        name: name.to_string(),
        provider_id: provider_id.to_string(),
        family: Some(family.to_string()),
        capabilities,
        cost,
        context_length: limits.0,
        output_length: limits.1,
        status: ModelStatus::Active,
    }
}

fn cost(input: f64, output: f64, cache_read: Option<f64>, cache_write: Option<f64>) -> ModelCost {
    ModelCost {
        input,
        output,
        cache_read,
        cache_write,
    }
}

fn provider(id: &str, name: &str, env_key: Option<&str>, models: Vec<ModelInfo>) -> ProviderInfo {
    ProviderInfo {
        id: id.to_string(),
        name: name.to_string(),
        env: env_key.map(|k| vec![k.to_string()]).unwrap_or_default(),
        models,
    }
}

fn anthropic_provider() -> ProviderInfo {
    provider(
        "anthropic",
        "Anthropic",
        Some("ANTHROPIC_API_KEY"),
        vec![
            model(
                "claude-opus-4-5",
                "Claude Opus 4.5",
                "anthropic",
                "claude-opus",
                ModelCapabilities {
                    tools: true,
                    vision: true,
                    reasoning: true,
                    streaming: true,
                    temperature: true,
                    attachment: true,
                    computer_use: true,
                },
                cost(15.0, 75.0, Some(1.5), Some(18.75)),
                (Some(200_000), Some(8192)),
            ),
            model(
                "claude-sonnet-4-5",
                "Claude Sonnet 4.5",
                "anthropic",
                "claude-sonnet",
                ModelCapabilities {
                    tools: true,
                    vision: true,
                    reasoning: false,
                    streaming: true,
                    temperature: true,
                    attachment: true,
                    computer_use: true,
                },
                cost(3.0, 15.0, Some(0.3), Some(3.75)),
                (Some(200_000), Some(8192)),
            ),
            model(
                "claude-haiku-4-5",
                "Claude Haiku 4.5",
                "anthropic",
                "claude-haiku",
                ModelCapabilities {
                    tools: true,
                    vision: true,
                    reasoning: false,
                    streaming: true,
                    temperature: true,
                    attachment: true,
                    computer_use: false,
                },
                cost(0.8, 4.0, Some(0.08), Some(1.0)),
                (Some(200_000), Some(8192)),
            ),
        ],
    )
}

fn openai_provider() -> ProviderInfo {
    provider(
        "openai",
        "OpenAI",
        Some("OPENAI_API_KEY"),
        vec![
            model(
                "gpt-4o",
                "GPT-4o",
                "openai",
                "gpt",
                ModelCapabilities {
                    tools: true,
                    vision: true,
                    reasoning: false,
                    streaming: true,
                    temperature: true,
                    attachment: true,
                    computer_use: false,
                },
                cost(2.5, 10.0, Some(1.25), None),
                (Some(128_000), Some(16_384)),
            ),
            model(
                "gpt-4o-mini",
                "GPT-4o Mini",
                "openai",
                "gpt",
                ModelCapabilities {
                    tools: true,
                    vision: true,
                    reasoning: false,
                    streaming: true,
                    temperature: true,
                    attachment: true,
                    computer_use: false,
                },
                cost(0.15, 0.6, Some(0.075), None),
                (Some(128_000), Some(16_384)),
            ),
            model(
                "o3",
                "o3",
                "openai",
                "o-series",
                ModelCapabilities {
                    tools: true,
                    vision: true,
                    reasoning: true,
                    streaming: true,
                    temperature: false,
                    attachment: true,
                    computer_use: false,
                },
                cost(10.0, 40.0, Some(2.5), None),
                (Some(200_000), Some(100_000)),
            ),
        ],
    )
}

fn google_provider() -> ProviderInfo {
    provider(
        "google",
        "Google",
        Some("GOOGLE_GENERATIVE_AI_API_KEY"),
        vec![
            model(
                "gemini-2.0-flash",
                "Gemini 2.0 Flash",
                "google",
                "gemini",
                ModelCapabilities {
                    tools: true,
                    vision: true,
                    reasoning: false,
                    streaming: true,
                    temperature: true,
                    attachment: true,
                    computer_use: false,
                },
                cost(0.1, 0.4, None, None),
                (Some(1_048_576), Some(8192)),
            ),
            model(
                "gemini-2.5-pro",
                "Gemini 2.5 Pro",
                "google",
                "gemini",
                ModelCapabilities {
                    tools: true,
                    vision: true,
                    reasoning: true,
                    streaming: true,
                    temperature: true,
                    attachment: true,
                    computer_use: false,
                },
                cost(1.25, 10.0, None, None),
                (Some(1_048_576), Some(65_536)),
            ),
        ],
    )
}

fn xai_provider() -> ProviderInfo {
    provider(
        "xai",
        "xAI",
        Some("XAI_API_KEY"),
        vec![
            model(
                "grok-3",
                "Grok 3",
                "xai",
                "grok",
                ModelCapabilities {
                    tools: true,
                    streaming: true,
                    temperature: true,
                    ..ModelCapabilities::default()
                },
                cost(3.0, 15.0, None, None),
                (Some(131_072), None),
            ),
            model(
                "grok-3-mini",
                "Grok 3 Mini",
                "xai",
                "grok",
                ModelCapabilities {
                    tools: true,
                    reasoning: true,
                    streaming: true,
                    temperature: true,
                    ..ModelCapabilities::default()
                },
                cost(0.3, 0.5, None, None),
                (Some(131_072), None),
            ),
        ],
    )
}

fn mistral_provider() -> ProviderInfo {
    let text_caps = ModelCapabilities {
        tools: true,
        streaming: true,
        temperature: true,
        ..ModelCapabilities::default()
    };
    provider(
        "mistral",
        "Mistral",
        Some("MISTRAL_API_KEY"),
        vec![
            model(
                "mistral-large-latest",
                "Mistral Large",
                "mistral",
                "mistral",
                text_caps.clone(),
                cost(2.0, 6.0, None, None),
                (Some(131_072), None),
            ),
            model(
                "mistral-small-latest",
                "Mistral Small",
                "mistral",
                "mistral",
                text_caps,
                cost(0.1, 0.3, None, None),
                (Some(32_768), None),
            ),
        ],
    )
}

fn groq_provider() -> ProviderInfo {
    let llama_caps = ModelCapabilities {
        tools: true,
        streaming: true,
        temperature: true,
        ..ModelCapabilities::default()
    };
    provider(
        "groq",
        "Groq",
        Some("GROQ_API_KEY"),
        vec![
            model(
                "llama-3.3-70b-versatile",
                "Llama 3.3 70B Versatile",
                "groq",
                "llama",
                llama_caps.clone(),
                cost(0.59, 0.79, None, None),
                (Some(128_000), Some(32_768)),
            ),
            model(
                "llama-3.1-8b-instant",
                "Llama 3.1 8B Instant",
                "groq",
                "llama",
                llama_caps,
                cost(0.05, 0.08, None, None),
                (Some(128_000), Some(8192)),
            ),
        ],
    )
}

fn secondary_providers_a() -> Vec<ProviderInfo> {
    let tool_text = ModelCapabilities {
        tools: true,
        streaming: true,
        temperature: true,
        ..ModelCapabilities::default()
    };
    vec![
        provider(
            "openrouter",
            "OpenRouter",
            Some("OPENROUTER_API_KEY"),
            vec![model(
                "meta-llama/llama-4-maverick",
                "Llama 4 Maverick",
                "openrouter",
                "llama",
                ModelCapabilities {
                    tools: true,
                    vision: true,
                    streaming: true,
                    temperature: true,
                    ..ModelCapabilities::default()
                },
                cost(0.18, 0.6, None, None),
                (Some(128_000), None),
            )],
        ),
        provider(
            "cohere",
            "Cohere",
            Some("COHERE_API_KEY"),
            vec![model(
                "command-r-plus",
                "Command R+",
                "cohere",
                "command",
                tool_text.clone(),
                cost(2.5, 10.0, None, None),
                (Some(128_000), Some(4096)),
            )],
        ),
        provider(
            "perplexity",
            "Perplexity",
            Some("PERPLEXITY_API_KEY"),
            vec![model(
                "sonar-pro",
                "Sonar Pro",
                "perplexity",
                "sonar",
                ModelCapabilities {
                    streaming: true,
                    temperature: true,
                    ..ModelCapabilities::default()
                },
                cost(3.0, 15.0, None, None),
                (Some(127_072), None),
            )],
        ),
        provider(
            "cerebras",
            "Cerebras",
            Some("CEREBRAS_API_KEY"),
            vec![model(
                "llama-4-scout-17b-16e-instruct",
                "Llama 4 Scout 17B",
                "cerebras",
                "llama",
                tool_text,
                cost(0.1, 0.1, None, None),
                (Some(128_000), None),
            )],
        ),
    ]
}

fn secondary_providers_b() -> Vec<ProviderInfo> {
    vec![
        provider(
            "azure",
            "Azure OpenAI",
            Some("AZURE_OPENAI_API_KEY"),
            vec![model(
                "gpt-4o",
                "GPT-4o",
                "azure",
                "gpt",
                ModelCapabilities {
                    tools: true,
                    vision: true,
                    streaming: true,
                    temperature: true,
                    attachment: true,
                    ..ModelCapabilities::default()
                },
                cost(2.5, 10.0, Some(1.25), None),
                (Some(128_000), Some(16_384)),
            )],
        ),
        provider(
            "together",
            "Together AI",
            Some("TOGETHER_API_KEY"),
            vec![model(
                "meta-llama/Llama-3-70b-chat-hf",
                "Llama 3 70B Chat",
                "together",
                "llama",
                ModelCapabilities {
                    streaming: true,
                    temperature: true,
                    ..ModelCapabilities::default()
                },
                cost(0.9, 0.9, None, None),
                (Some(8192), None),
            )],
        ),
        provider(
            "deepinfra",
            "DeepInfra",
            Some("DEEPINFRA_API_KEY"),
            vec![model(
                "meta-llama/Meta-Llama-3.1-70B-Instruct",
                "Llama 3.1 70B Instruct",
                "deepinfra",
                "llama",
                ModelCapabilities {
                    tools: true,
                    streaming: true,
                    temperature: true,
                    ..ModelCapabilities::default()
                },
                cost(0.23, 0.4, None, None),
                (Some(131_072), None),
            )],
        ),
        provider(
            "github-copilot",
            "GitHub Copilot",
            None,
            vec![model(
                "gpt-4o",
                "GPT-4o (Copilot)",
                "github-copilot",
                "gpt",
                ModelCapabilities {
                    tools: true,
                    vision: true,
                    streaming: true,
                    temperature: true,
                    attachment: true,
                    ..ModelCapabilities::default()
                },
                cost(0.0, 0.0, None, None),
                (Some(128_000), Some(16_384)),
            )],
        ),
        provider(
            "openai-codex",
            "OpenAI Codex",
            None,
            vec![model(
                "codex-mini-latest",
                "Codex Mini",
                "openai-codex",
                "codex",
                ModelCapabilities {
                    tools: true,
                    reasoning: true,
                    streaming: true,
                    ..ModelCapabilities::default()
                },
                cost(1.5, 6.0, Some(0.375), None),
                (Some(200_000), None),
            )],
        ),
    ]
}

#[must_use]
pub fn builtin_providers() -> Vec<ProviderInfo> {
    let mut providers = vec![
        anthropic_provider(),
        openai_provider(),
        google_provider(),
        xai_provider(),
        mistral_provider(),
        groq_provider(),
    ];
    providers.extend(secondary_providers_a());
    providers.extend(secondary_providers_b());
    providers
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_builtin_providers_non_empty() {
        let providers = builtin_providers();
        assert!(!providers.is_empty());
    }

    #[test]
    fn test_registry_new_stores_all_providers() {
        let providers = builtin_providers();
        let count = providers.len();
        let registry = ProviderRegistry::new(providers);
        assert_eq!(registry.list_providers().len(), count);
    }

    #[test]
    fn test_get_provider_anthropic() {
        let registry = ProviderRegistry::new(builtin_providers());
        let provider = registry.get_provider("anthropic");
        assert!(provider.is_some());
        assert_eq!(provider.map(|p| p.id.as_str()), Some("anthropic"));
    }

    #[test]
    fn test_get_model_claude_sonnet() {
        let registry = ProviderRegistry::new(builtin_providers());
        let model = registry.get_model("anthropic", "claude-sonnet-4-5");
        assert!(model.is_some());
        assert_eq!(model.map(|m| m.id.as_str()), Some("claude-sonnet-4-5"));
    }

    #[test]
    fn test_has_api_key_false_when_not_set() {
        let provider = ProviderInfo {
            id: "test-provider".to_string(),
            name: "Test Provider".to_string(),
            env: vec!["AVOCODE_TEST_KEY_THAT_DOES_NOT_EXIST_12345".to_string()],
            models: vec![],
        };
        assert!(!ProviderRegistry::has_api_key(&provider));
    }

    #[test]
    fn test_all_models_returns_models_from_all_providers() {
        let registry = ProviderRegistry::new(builtin_providers());
        let all = registry.all_models();
        assert!(!all.is_empty());
    }

    #[test]
    fn test_list_models_for_provider() {
        let registry = ProviderRegistry::new(builtin_providers());
        let models = registry.list_models("anthropic");
        assert!(!models.is_empty());
        for m in &models {
            assert_eq!(m.provider_id, "anthropic");
        }
    }

    #[test]
    fn test_get_provider_not_found() {
        let registry = ProviderRegistry::new(builtin_providers());
        assert!(registry.get_provider("nonexistent-provider").is_none());
    }
}
