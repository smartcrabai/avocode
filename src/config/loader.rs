use std::path::Path;

use crate::config::{
    ConfigError, paths,
    schema::{AgentConfig, Config, ExperimentalConfig, ProviderConfig},
};

/// Strips `//` line comments and `/* */` block comments from a JSONC string,
/// respecting string literals so comment sequences inside strings are preserved.
#[must_use]
pub fn strip_jsonc_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let chars: Vec<char> = input.chars().collect();
    let len = chars.len();
    let mut i = 0;

    while i < len {
        // Inside a string literal -- copy verbatim until the closing `"`.
        if chars[i] == '"' {
            out.push(chars[i]);
            i += 1;
            while i < len {
                let c = chars[i];
                out.push(c);
                i += 1;
                if c == '\\' && i < len {
                    out.push(chars[i]);
                    i += 1;
                } else if c == '"' {
                    break;
                }
            }
            continue;
        }

        // Block comment `/* ... */`.
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '*' {
            i += 2;
            while i + 1 < len && !(chars[i] == '*' && chars[i + 1] == '/') {
                i += 1;
            }
            i += 2;
            continue;
        }

        // Line comment `// ...`.
        if i + 1 < len && chars[i] == '/' && chars[i + 1] == '/' {
            while i < len && chars[i] != '\n' {
                i += 1;
            }
            continue;
        }

        out.push(chars[i]);
        i += 1;
    }

    out
}

/// Parses a JSONC string by first stripping comments and then deserialising
/// with `serde_json`.
///
/// # Errors
///
/// Returns [`ConfigError::Parse`] if the input cannot be deserialised.
pub fn parse_jsonc<T: serde::de::DeserializeOwned>(input: &str) -> Result<T, ConfigError> {
    let stripped = strip_jsonc_comments(input);
    serde_json::from_str(&stripped).map_err(|e| ConfigError::Parse {
        file: String::new(),
        message: e.to_string(),
    })
}

/// Deep-merges two [`Config`] values.  `overlay` takes precedence for scalar
/// (Option) fields.  [`std::collections::HashMap`] fields are merged with
/// overlay keys winning on collision.  [`Vec`] fields are concatenated
/// (base first, then overlay).
pub fn merge(base: Config, overlay: Config) -> Config {
    Config {
        model: overlay.model.or(base.model),
        default_agent: overlay.default_agent.or(base.default_agent),
        provider: merge_map(base.provider, overlay.provider, merge_provider),
        disabled_providers: concat_vec(base.disabled_providers, overlay.disabled_providers),
        agent: merge_map(base.agent, overlay.agent, merge_agent),
        mcp: merge_map_replace(base.mcp, overlay.mcp),
        permission: concat_vec(base.permission, overlay.permission),
        instructions: concat_vec(base.instructions, overlay.instructions),
        experimental: merge_experimental(&base.experimental, &overlay.experimental),
        share: overlay.share.or(base.share),
    }
}

fn merge_provider(base: ProviderConfig, overlay: ProviderConfig) -> ProviderConfig {
    ProviderConfig {
        api_key: overlay.api_key.or(base.api_key),
        base_url: overlay.base_url.or(base.base_url),
        options: if overlay.options.is_null() {
            base.options
        } else {
            overlay.options
        },
        models: merge_map_replace(base.models, overlay.models),
    }
}

fn merge_agent(base: AgentConfig, overlay: AgentConfig) -> AgentConfig {
    AgentConfig {
        model: overlay.model.or(base.model),
        prompt: overlay.prompt.or(base.prompt),
        description: overlay.description.or(base.description),
        temperature: overlay.temperature.or(base.temperature),
        max_tokens: overlay.max_tokens.or(base.max_tokens),
        tools: concat_vec(base.tools, overlay.tools),
        disabled_tools: concat_vec(base.disabled_tools, overlay.disabled_tools),
    }
}

fn merge_experimental(
    base: &ExperimentalConfig,
    overlay: &ExperimentalConfig,
) -> ExperimentalConfig {
    ExperimentalConfig {
        batch_tool: overlay.batch_tool.or(base.batch_tool),
        continue_loop_on_deny: overlay.continue_loop_on_deny.or(base.continue_loop_on_deny),
        mcp_timeout: overlay.mcp_timeout.or(base.mcp_timeout),
    }
}

fn merge_map<V, F>(
    mut base: std::collections::HashMap<String, V>,
    overlay: std::collections::HashMap<String, V>,
    merge_fn: F,
) -> std::collections::HashMap<String, V>
where
    F: Fn(V, V) -> V,
{
    for (k, v) in overlay {
        let entry = base.remove(&k);
        let merged = match entry {
            Some(existing) => merge_fn(existing, v),
            None => v,
        };
        base.insert(k, merged);
    }
    base
}

fn merge_map_replace<V>(
    mut base: std::collections::HashMap<String, V>,
    overlay: std::collections::HashMap<String, V>,
) -> std::collections::HashMap<String, V> {
    merge_map(std::mem::take(&mut base), overlay, |_b, o| o)
}

fn concat_vec<T>(mut base: Vec<T>, mut overlay: Vec<T>) -> Vec<T> {
    base.append(&mut overlay);
    base
}

/// Reads a file from `path` and parses it as JSONC, returning a [`Config`].
/// Returns `Ok(Config::default())` if the file does not exist.
fn load_file(path: &Path) -> Result<Config, ConfigError> {
    match std::fs::read_to_string(path) {
        Ok(contents) => parse_jsonc(&contents).map_err(|e| match e {
            ConfigError::Parse { message, .. } => ConfigError::Parse {
                file: path.display().to_string(),
                message,
            },
            other @ ConfigError::Io(_) => other,
        }),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(Config::default()),
        Err(e) => Err(ConfigError::Io(e)),
    }
}

/// Loads and merges all config layers for a given project `directory`.
///
/// Merge order (later layers override earlier ones):
/// 1. System config (`/etc/opencode/opencode.jsonc` or platform equivalent)
/// 2. Global user config (`~/.config/opencode/opencode.jsonc`)
/// 3. Project configs ordered from outermost to innermost directory
///
/// Each layer tries `opencode.jsonc` first, then falls back to `opencode.json`.
///
/// # Errors
///
/// Returns [`ConfigError`] if any config file exists but cannot be read or parsed.
pub fn load(directory: &Path) -> Result<Config, ConfigError> {
    let mut config = Config::default();

    // 1. System config.
    if let Some(sys_dir) = paths::system_config_dir()
        && let Some(path) = paths::config_file_in_dir(&sys_dir)
    {
        config = merge(config, load_file(&path)?);
    }

    // 2. Global user config.
    if let Some(global_dir) = paths::global_config_dir()
        && let Some(path) = paths::config_file_in_dir(&global_dir)
    {
        config = merge(config, load_file(&path)?);
    }

    // 3. Project configs (outermost -> innermost).
    for project_file in paths::project_config_files(directory) {
        config = merge(config, load_file(&project_file)?);
    }

    Ok(config)
}

/// Loads only the global user config (`~/.config/opencode/opencode.jsonc` or
/// `~/.config/opencode/opencode.json`).
///
/// # Errors
///
/// Returns [`ConfigError`] if the config file exists but cannot be read or parsed.
pub fn load_global() -> Result<Config, ConfigError> {
    match paths::global_config_dir() {
        Some(dir) => match paths::config_file_in_dir(&dir) {
            Some(path) => load_file(&path),
            None => Ok(Config::default()),
        },
        None => Ok(Config::default()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::config::schema::{McpConfig, PermissionAction, PermissionRule};

    #[test]
    fn strip_line_comment() -> Result<(), Box<dyn std::error::Error>> {
        let input = "{ \"key\": \"value\" // this is a comment\n}";
        let result = strip_jsonc_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&result)?;
        assert_eq!(parsed["key"], "value");
        Ok(())
    }

    #[test]
    fn strip_block_comment() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{ /* block comment */ "key": "value" }"#;
        let result = strip_jsonc_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&result)?;
        assert_eq!(parsed["key"], "value");
        Ok(())
    }

    #[test]
    fn strip_preserves_comment_syntax_in_strings() -> Result<(), Box<dyn std::error::Error>> {
        // Slashes inside a string value must not be stripped.
        let input = r#"{ "url": "https://example.com" }"#;
        let result = strip_jsonc_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&result)?;
        assert_eq!(parsed["url"], "https://example.com");
        Ok(())
    }

    #[test]
    fn strip_multiline_block_comment() -> Result<(), Box<dyn std::error::Error>> {
        let input = "{ /* line1\nline2 */ \"key\": 42 }";
        let result = strip_jsonc_comments(input);
        let parsed: serde_json::Value = serde_json::from_str(&result)?;
        assert_eq!(parsed["key"], 42);
        Ok(())
    }

    #[test]
    fn parse_jsonc_parses_valid_jsonc() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{
            // model selection
            "model": "gpt-4o" /* default model */
        }"#;
        let cfg: Config = parse_jsonc(input)?;
        assert_eq!(cfg.model.as_deref(), Some("gpt-4o"));
        Ok(())
    }

    #[test]
    fn merge_scalar_overlay_wins() {
        let base = Config {
            model: Some("base-model".to_string()),
            ..Default::default()
        };
        let overlay = Config {
            model: Some("overlay-model".to_string()),
            ..Default::default()
        };
        let merged = merge(base, overlay);
        assert_eq!(merged.model.as_deref(), Some("overlay-model"));
    }

    #[test]
    fn merge_scalar_keeps_base_when_overlay_is_none() {
        let base = Config {
            model: Some("base-model".to_string()),
            ..Default::default()
        };
        let overlay = Config::default();
        let merged = merge(base, overlay);
        assert_eq!(merged.model.as_deref(), Some("base-model"));
    }

    #[test]
    fn merge_vecs_are_concatenated() {
        let base = Config {
            instructions: vec!["base instruction".to_string()],
            ..Default::default()
        };
        let overlay = Config {
            instructions: vec!["overlay instruction".to_string()],
            ..Default::default()
        };
        let merged = merge(base, overlay);
        assert_eq!(merged.instructions.len(), 2);
        assert_eq!(merged.instructions[0], "base instruction");
        assert_eq!(merged.instructions[1], "overlay instruction");
    }

    #[test]
    fn merge_maps_overlay_wins_on_collision() {
        use std::collections::HashMap;

        let mut base_providers: HashMap<String, ProviderConfig> = HashMap::new();
        base_providers.insert(
            "openai".to_string(),
            ProviderConfig {
                api_key: Some("base-key".to_string()),
                ..Default::default()
            },
        );

        let mut overlay_providers: HashMap<String, ProviderConfig> = HashMap::new();
        overlay_providers.insert(
            "openai".to_string(),
            ProviderConfig {
                api_key: Some("overlay-key".to_string()),
                ..Default::default()
            },
        );

        let base = Config {
            provider: base_providers,
            ..Default::default()
        };
        let overlay = Config {
            provider: overlay_providers,
            ..Default::default()
        };
        let merged = merge(base, overlay);
        assert_eq!(
            merged.provider["openai"].api_key.as_deref(),
            Some("overlay-key")
        );
    }

    #[test]
    fn merge_maps_preserves_base_only_keys() {
        use std::collections::HashMap;

        let mut base_providers: HashMap<String, ProviderConfig> = HashMap::new();
        base_providers.insert(
            "anthropic".to_string(),
            ProviderConfig {
                api_key: Some("anthropic-key".to_string()),
                ..Default::default()
            },
        );

        let base = Config {
            provider: base_providers,
            ..Default::default()
        };
        let overlay = Config::default();
        let merged = merge(base, overlay);
        assert_eq!(
            merged.provider["anthropic"].api_key.as_deref(),
            Some("anthropic-key")
        );
    }

    #[test]
    fn mcp_config_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{
            "mcp": {
                "my-server": {
                    "type": "Stdio",
                    "command": "npx",
                    "args": ["-y", "my-server"],
                    "env": {}
                }
            }
        }"#;
        let cfg: Config = parse_jsonc(input)?;
        let server = cfg.mcp.get("my-server").ok_or("server not present")?;
        match server {
            McpConfig::Stdio { command, .. } => assert_eq!(command, "npx"),
            McpConfig::Sse { .. } => panic!("expected Stdio variant"),
        }
        Ok(())
    }

    #[test]
    fn permission_action_roundtrip() -> Result<(), Box<dyn std::error::Error>> {
        let rule = PermissionRule {
            permission: "write".to_string(),
            pattern: "**/*.rs".to_string(),
            action: PermissionAction::Allow,
        };
        let json = serde_json::to_string(&rule)?;
        let back: PermissionRule = serde_json::from_str(&json)?;
        assert!(matches!(back.action, PermissionAction::Allow));
        Ok(())
    }

    #[test]
    fn model_override_disabled_default_false() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{ "provider": { "openai": { "models": { "gpt-4": {} } } } }"#;
        let cfg: Config = parse_jsonc(input)?;
        let model = &cfg.provider["openai"].models["gpt-4"];
        assert!(!model.disabled);
        Ok(())
    }

    #[test]
    fn experimental_flags_parsed() -> Result<(), Box<dyn std::error::Error>> {
        let input = r#"{ "experimental": { "batch_tool": true, "mcp_timeout": 5000 } }"#;
        let cfg: Config = parse_jsonc(input)?;
        assert_eq!(cfg.experimental.batch_tool, Some(true));
        assert_eq!(cfg.experimental.mcp_timeout, Some(5000));
        Ok(())
    }
}
