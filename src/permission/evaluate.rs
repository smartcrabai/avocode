use std::collections::HashSet;

use super::schema::{PermissionAction, PermissionRule};
use super::wildcard::wildcard_match;

/// Evaluate permission for a `permission` + `pattern` combination against ordered rulesets.
///
/// Last-match-wins: later rules in later rulesets override earlier ones.
/// Returns `Allow` if no rule matches.
#[must_use]
pub fn evaluate(
    permission: &str,
    pattern: &str,
    rulesets: &[&[PermissionRule]],
) -> PermissionAction {
    let mut result = PermissionAction::Allow;
    for ruleset in rulesets {
        for rule in *ruleset {
            if wildcard_match(permission, &rule.permission)
                && wildcard_match(pattern, &rule.pattern)
            {
                result = rule.action.clone();
            }
        }
    }
    result
}

/// Returns tool IDs that are fully denied (pattern `"*"` with `Deny` action).
#[must_use]
pub fn denied_tools(tools: &[&str], ruleset: &[PermissionRule]) -> HashSet<String> {
    tools
        .iter()
        .filter(|&&tool| evaluate(tool, "*", &[ruleset]) == PermissionAction::Deny)
        .map(|&t| t.to_owned())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::permission::schema::{default_build_rules, default_plan_rules};

    #[test]
    fn allow_then_deny_last_match_wins() {
        let rules = vec![
            PermissionRule {
                permission: "read".into(),
                pattern: "*".into(),
                action: PermissionAction::Allow,
            },
            PermissionRule {
                permission: "read".into(),
                pattern: "*".into(),
                action: PermissionAction::Deny,
            },
        ];
        assert_eq!(
            evaluate("read", "foo.txt", &[&rules]),
            PermissionAction::Deny
        );
    }

    #[test]
    fn no_matching_rules_returns_allow() {
        let rules: Vec<PermissionRule> = vec![];
        assert_eq!(
            evaluate("write", "foo.txt", &[&rules]),
            PermissionAction::Allow
        );
    }

    #[test]
    fn denied_tools_finds_denied_tool() {
        let rules = default_plan_rules();
        let tools = &["write", "read", "bash"];
        let denied = denied_tools(tools, &rules);
        assert!(denied.contains("write"));
        assert!(denied.contains("bash"));
        assert!(!denied.contains("read"));
    }

    #[test]
    fn default_build_rules_bash_is_ask() {
        let rules = default_build_rules();
        let action = evaluate("bash", "*", &[&rules]);
        assert_eq!(action, PermissionAction::Ask);
    }

    #[test]
    fn default_plan_rules_write_is_deny() {
        let rules = default_plan_rules();
        let action = evaluate("write", "*", &[&rules]);
        assert_eq!(action, PermissionAction::Deny);
    }

    #[test]
    fn multiple_rulesets_later_overrides_earlier() {
        let agent_rules = vec![PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Deny,
        }];
        let config_rules = vec![PermissionRule {
            permission: "bash".into(),
            pattern: "*".into(),
            action: PermissionAction::Allow,
        }];
        // config_rules comes last -> Allow wins
        assert_eq!(
            evaluate("bash", "*", &[&agent_rules, &config_rules]),
            PermissionAction::Allow
        );
    }
}
