#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProviderInfo {
    pub id: String,
    pub name: String,
    pub env: Vec<String>,
    pub models: Vec<ModelInfo>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub name: String,
    pub provider_id: String,
    pub family: Option<String>,
    pub capabilities: ModelCapabilities,
    pub cost: ModelCost,
    pub context_length: Option<u64>,
    pub output_length: Option<u64>,
    pub status: ModelStatus,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ModelCapabilities {
    pub tools: bool,
    pub vision: bool,
    pub reasoning: bool,
    pub streaming: bool,
    pub temperature: bool,
    pub attachment: bool,
    pub computer_use: bool,
}

#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
pub struct ModelCost {
    pub input: f64,
    pub output: f64,
    pub cache_read: Option<f64>,
    pub cache_write: Option<f64>,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "lowercase")]
pub enum ModelStatus {
    #[default]
    Active,
    Beta,
    Alpha,
    Deprecated,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_model_info_serialization_roundtrip() {
        let model = ModelInfo {
            id: "claude-sonnet-4-5".to_string(),
            name: "Claude Sonnet 4.5".to_string(),
            provider_id: "anthropic".to_string(),
            family: Some("claude-sonnet".to_string()),
            capabilities: ModelCapabilities {
                tools: true,
                vision: true,
                reasoning: false,
                streaming: true,
                temperature: true,
                attachment: true,
                computer_use: false,
            },
            cost: ModelCost {
                input: 3.0,
                output: 15.0,
                cache_read: Some(0.3),
                cache_write: Some(3.75),
            },
            context_length: Some(200_000),
            output_length: Some(8192),
            status: ModelStatus::Active,
        };

        let json = serde_json::to_string(&model).expect("serialization failed");
        let deserialized: ModelInfo = serde_json::from_str(&json).expect("deserialization failed");

        assert_eq!(model.id, deserialized.id);
        assert_eq!(model.name, deserialized.name);
        assert_eq!(model.provider_id, deserialized.provider_id);
        assert_eq!(model.family, deserialized.family);
        assert_eq!(model.capabilities.tools, deserialized.capabilities.tools);
        assert_eq!(model.capabilities.vision, deserialized.capabilities.vision);
        assert_eq!(model.cost.input, deserialized.cost.input);
        assert_eq!(model.cost.output, deserialized.cost.output);
        assert_eq!(model.cost.cache_read, deserialized.cost.cache_read);
        assert_eq!(model.context_length, deserialized.context_length);
        assert_eq!(model.status, deserialized.status);
    }

    #[test]
    fn test_model_status_default() {
        let status = ModelStatus::default();
        assert_eq!(status, ModelStatus::Active);
    }
}
