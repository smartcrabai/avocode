use std::collections::HashMap;

/// Accepts `Vec<PermissionRule>` or any non-array value (e.g. opencode's `"allow"`
/// shorthand), treating non-array values as an empty list.
/// Array values are parsed strictly -- malformed entries return a deserialization error.
fn deserialize_permission_rules<'de, D>(deserializer: D) -> Result<Vec<PermissionRule>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    use serde::Deserialize as _;
    let value = serde_json::Value::deserialize(deserializer)?;
    if value.is_array() {
        serde_json::from_value(value).map_err(serde::de::Error::custom)
    } else {
        Ok(Vec::new())
    }
}

/// Top-level configuration structure matching `OpenCode`'s `Config.Info` schema.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct Config {
    pub model: Option<String>,
    pub default_agent: Option<String>,
    pub provider: HashMap<String, ProviderConfig>,
    pub disabled_providers: Vec<String>,
    pub agent: HashMap<String, AgentConfig>,
    pub mcp: HashMap<String, McpConfig>,
    #[serde(default, deserialize_with = "deserialize_permission_rules")]
    pub permission: Vec<PermissionRule>,
    pub instructions: Vec<String>,
    pub experimental: ExperimentalConfig,
    pub share: Option<String>,
}

impl Config {
    /// Returns the set of provider IDs that are explicitly configured.
    #[must_use]
    pub fn configured_provider_ids(&self) -> std::collections::HashSet<String> {
        self.provider.keys().cloned().collect()
    }
}

/// Provider-specific configuration including API keys and model overrides.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ProviderConfig {
    pub api_key: Option<String>,
    pub base_url: Option<String>,
    pub options: serde_json::Value,
    pub models: HashMap<String, ModelOverride>,
}

/// Agent-specific configuration for customizing model behaviour.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct AgentConfig {
    pub model: Option<String>,
    pub prompt: Option<String>,
    pub description: Option<String>,
    pub temperature: Option<f64>,
    pub max_tokens: Option<u32>,
    pub tools: Vec<String>,
    pub disabled_tools: Vec<String>,
}

/// MCP server configuration, either stdio subprocess or SSE endpoint.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(tag = "type")]
pub enum McpConfig {
    Stdio {
        command: String,
        args: Vec<String>,
        env: HashMap<String, String>,
    },
    Sse {
        url: String,
        headers: HashMap<String, String>,
    },
}

/// A single permission rule associating a pattern with an allow/deny/ask action.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionRule {
    pub permission: String,
    pub pattern: String,
    pub action: PermissionAction,
}

/// The action to take when a permission rule matches.
#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,
}

/// Per-model overrides within a provider configuration.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ModelOverride {
    pub disabled: bool,
}

/// Experimental feature flags.
#[derive(Debug, Clone, Default, serde::Serialize, serde::Deserialize)]
#[serde(default)]
pub struct ExperimentalConfig {
    pub batch_tool: Option<bool>,
    pub continue_loop_on_deny: Option<bool>,
    pub mcp_timeout: Option<u64>,
}
