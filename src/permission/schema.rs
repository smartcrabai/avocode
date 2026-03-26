#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum PermissionAction {
    Allow,
    Deny,
    Ask,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionRule {
    pub permission: String,
    pub pattern: String,
    pub action: PermissionAction,
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct PermissionRequest {
    pub id: String,
    pub session_id: String,
    pub permission: String,
    pub pattern: String,
    pub metadata: serde_json::Value,
    pub always_patterns: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PermissionReply {
    Once,
    Always,
    Deny,
}

/// Default permission rulesets for built-in agents.
#[must_use]
pub fn default_build_rules() -> Vec<PermissionRule> {
    vec![
        PermissionRule {
            permission: "read".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        },
        PermissionRule {
            permission: "write".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        },
        PermissionRule {
            permission: "edit".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        },
        PermissionRule {
            permission: "glob".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        },
        PermissionRule {
            permission: "grep".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        },
        PermissionRule {
            permission: "ls".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        },
        PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Ask,
        },
        PermissionRule {
            permission: "read".into(),
            pattern: "**/.env".into(),
            action: PermissionAction::Ask,
        },
        PermissionRule {
            permission: "read".into(),
            pattern: "**/.env.*".into(),
            action: PermissionAction::Ask,
        },
    ]
}

#[must_use]
pub fn default_plan_rules() -> Vec<PermissionRule> {
    vec![
        PermissionRule {
            permission: "write".into(),
            pattern: "*".into(),
            action: PermissionAction::Deny,
        },
        PermissionRule {
            permission: "edit".into(),
            pattern: "*".into(),
            action: PermissionAction::Deny,
        },
        PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Deny,
        },
        PermissionRule {
            permission: "read".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        },
        PermissionRule {
            permission: "glob".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        },
        PermissionRule {
            permission: "grep".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        },
        PermissionRule {
            permission: "ls".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        },
    ]
}

#[must_use]
pub fn default_explore_rules() -> Vec<PermissionRule> {
    let mut rules = default_plan_rules();
    rules.push(PermissionRule {
        permission: "webfetch".into(),
        pattern: "*".into(),
        action: PermissionAction::Allow,
    });
    rules
}
