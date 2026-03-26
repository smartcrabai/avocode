use std::collections::HashMap;
use std::hash::BuildHasher;

use crate::agent::schema::{AgentInfo, AgentMode, AgentPermissionRule, PermissionAction};

fn deny_rule(permission: &str) -> AgentPermissionRule {
    AgentPermissionRule {
        permission: permission.into(),
        pattern: "*".into(),
        action: PermissionAction::Deny,
    }
}

fn ask_rule(permission: &str, pattern: &str) -> AgentPermissionRule {
    AgentPermissionRule {
        permission: permission.into(),
        pattern: pattern.into(),
        action: PermissionAction::Ask,
    }
}

fn build_agent() -> AgentInfo {
    AgentInfo {
        name: "build".into(),
        description: "Default coding agent with full tool access".into(),
        mode: AgentMode::Primary,
        native: true,
        hidden: false,
        color: Some("#4f46e5".into()), // indigo
        permission: vec![
            ask_rule("bash", "*"),
            ask_rule("read", "**/.env"),
            ask_rule("read", "**/.env.*"),
        ],
        ..Default::default()
    }
}

fn plan_agent() -> AgentInfo {
    AgentInfo {
        name: "plan".into(),
        description: "Read-only planning mode — only reads files, cannot make changes".into(),
        mode: AgentMode::Primary,
        native: true,
        hidden: false,
        color: Some("#0891b2".into()), // cyan
        permission: vec![deny_rule("write"), deny_rule("edit"), deny_rule("bash")],
        ..Default::default()
    }
}

fn general_agent() -> AgentInfo {
    AgentInfo {
        name: "general".into(),
        description: "General-purpose subagent for complex parallel tasks".into(),
        mode: AgentMode::Subagent,
        native: true,
        hidden: false,
        color: Some("#059669".into()), // green
        ..Default::default()
    }
}

fn explore_agent() -> AgentInfo {
    AgentInfo {
        name: "explore".into(),
        description:
            "Fast code exploration subagent — read-only access for quickly finding information"
                .into(),
        mode: AgentMode::Subagent,
        native: true,
        hidden: false,
        color: Some("#d97706".into()), // amber
        permission: vec![deny_rule("write"), deny_rule("edit"), deny_rule("bash")],
        ..Default::default()
    }
}

/// Returns all built-in agent definitions
#[must_use]
pub fn builtin_agents() -> HashMap<String, AgentInfo> {
    let mut agents = HashMap::new();
    agents.insert("build".into(), build_agent());
    agents.insert("plan".into(), plan_agent());
    agents.insert("general".into(), general_agent());
    agents.insert("explore".into(), explore_agent());
    agents
}

/// Resolve an agent by name. Returns config override merged with builtin, or just the builtin.
#[must_use]
pub fn resolve_agent<S: BuildHasher>(
    name: &str,
    config_overrides: &HashMap<String, AgentInfo, S>,
) -> Option<AgentInfo> {
    let builtins = builtin_agents();

    if let Some(builtin) = builtins.get(name) {
        if let Some(override_info) = config_overrides.get(name) {
            let mut merged = builtin.clone();
            if !override_info.description.is_empty() {
                merged.description.clone_from(&override_info.description);
            }
            if override_info.model.is_some() {
                merged.model.clone_from(&override_info.model);
            }
            if override_info.temperature.is_some() {
                merged.temperature = override_info.temperature;
            }
            if override_info.prompt.is_some() {
                merged.prompt.clone_from(&override_info.prompt);
            }
            if !override_info.permission.is_empty() {
                merged.permission.clone_from(&override_info.permission);
            }
            Some(merged)
        } else {
            Some(builtin.clone())
        }
    } else {
        // Check if it's a custom agent from config
        config_overrides.get(name).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn builtin_agents_returns_four_agents() {
        let agents = builtin_agents();
        assert_eq!(agents.len(), 4);
        assert!(agents.contains_key("build"));
        assert!(agents.contains_key("plan"));
        assert!(agents.contains_key("general"));
        assert!(agents.contains_key("explore"));
    }

    #[test]
    fn plan_agent_has_write_deny_rule() {
        let agents = builtin_agents();
        let Some(plan) = agents.get("plan") else {
            panic!("plan agent should exist");
        };
        let has_write_deny = plan.permission.iter().any(|r| {
            r.permission == "write" && r.pattern == "*" && r.action == PermissionAction::Deny
        });
        assert!(has_write_deny, "plan agent should have write:deny rule");
    }

    #[test]
    fn build_agent_has_bash_ask_rule() {
        let agents = builtin_agents();
        let Some(build) = agents.get("build") else {
            panic!("build agent should exist");
        };
        let has_bash_ask = build.permission.iter().any(|r| {
            r.permission == "bash" && r.pattern == "*" && r.action == PermissionAction::Ask
        });
        assert!(has_bash_ask, "build agent should have bash:ask rule");
    }

    #[test]
    fn resolve_agent_returns_build_agent() {
        let overrides = HashMap::new();
        let agent = resolve_agent("build", &overrides);
        assert!(agent.is_some());
        assert_eq!(agent.as_ref().map(|a| &a.name), Some(&"build".to_string()));
    }

    #[test]
    fn resolve_agent_nonexistent_returns_none() {
        let overrides = HashMap::new();
        let agent = resolve_agent("nonexistent", &overrides);
        assert!(agent.is_none());
    }

    #[test]
    fn resolve_agent_merges_overrides() {
        let mut overrides = HashMap::new();
        overrides.insert(
            "build".into(),
            AgentInfo {
                description: "Custom build description".into(),
                model: Some("gpt-4o".into()),
                ..Default::default()
            },
        );
        let Some(agent) = resolve_agent("build", &overrides) else {
            panic!("build agent should exist");
        };
        assert_eq!(agent.description, "Custom build description");
        assert_eq!(agent.model, Some("gpt-4o".into()));
        // native should still be true from builtin
        assert!(agent.native);
    }
}
