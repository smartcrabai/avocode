use crate::agent::schema::AgentInfo;

/// Selects a system prompt based on model API ID
#[must_use]
pub fn system_prompt_for_model(model_api_id: &str) -> &'static str {
    let lower = model_api_id.to_lowercase();
    if lower.contains("claude") {
        PROMPT_ANTHROPIC
    } else if lower.contains("codex") {
        PROMPT_CODEX
    } else if lower.contains("gemini") {
        PROMPT_GEMINI
    } else if lower.starts_with("gpt") || lower.contains("openai") {
        PROMPT_GPT
    } else {
        PROMPT_DEFAULT
    }
}

/// Assembles the full system prompt for a session
#[must_use]
pub fn assemble_system(
    model_api_id: &str,
    agent: &AgentInfo,
    user_system: Option<&str>,
    working_directory: &str,
    is_git_repo: bool,
    platform: &str,
    date: &str,
) -> String {
    let mut parts = Vec::new();

    parts.push(system_prompt_for_model(model_api_id).to_owned());

    if let Some(agent_prompt) = &agent.prompt {
        parts.push(agent_prompt.clone());
    }

    if let Some(user) = user_system {
        parts.push(user.to_owned());
    }

    parts.push(environment_block(
        working_directory,
        is_git_repo,
        platform,
        date,
    ));

    parts.join("\n\n")
}

/// Generates the `<env>...</env>` block with environment info
#[must_use]
pub fn environment_block(
    working_directory: &str,
    is_git_repo: bool,
    platform: &str,
    date: &str,
) -> String {
    format!(
        "<env>\nWorking directory: {working_directory}\nIs git repository: {is_git}\nPlatform: {platform}\nDate: {date}\n</env>",
        is_git = if is_git_repo { "true" } else { "false" }
    )
}

pub const PROMPT_ANTHROPIC: &str = "You are avocode, an AI coding assistant. You help users write, understand, debug, and improve code.

You have access to tools for reading and writing files, executing commands, searching code, and browsing the web. Use these tools proactively to understand codebases and implement changes.

Be direct and concise. Show code rather than explaining at length. When making changes, explain what you changed and why briefly.";

pub const PROMPT_GPT: &str = "You are avocode, an AI coding assistant. You help users write, understand, debug, and improve code.

You have access to tools for reading and writing files, executing shell commands, searching codebases, and fetching web content. Use these tools to understand context before making changes.

Be direct and concise. Prefer showing code over lengthy explanations.";

pub const PROMPT_GEMINI: &str = "You are avocode, an AI coding assistant helping with software development tasks.

You can use tools to read and write files, run commands, search code, and access the web. Always read relevant files before making changes.

Be clear and concise in your responses.";

pub const PROMPT_CODEX: &str = "You are avocode, an AI coding agent. Your primary task is to implement code changes requested by the user.

Use file tools to read existing code, understand patterns, then implement changes carefully. Always verify your changes make sense in context.";

pub const PROMPT_DEFAULT: &str = "You are avocode, an AI coding assistant. Help the user with their software development tasks.

Use available tools to read files, execute commands, and search code. Be concise and show code examples.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn system_prompt_for_claude_returns_anthropic() {
        assert_eq!(
            system_prompt_for_model("claude-3-5-sonnet"),
            PROMPT_ANTHROPIC
        );
    }

    #[test]
    fn system_prompt_for_gpt_returns_gpt() {
        assert_eq!(system_prompt_for_model("gpt-4o"), PROMPT_GPT);
    }

    #[test]
    fn system_prompt_for_gemini_returns_gemini() {
        assert_eq!(system_prompt_for_model("gemini-2.0-flash"), PROMPT_GEMINI);
    }

    #[test]
    fn system_prompt_for_codex_returns_codex() {
        assert_eq!(system_prompt_for_model("codex-mini-latest"), PROMPT_CODEX);
    }

    #[test]
    fn system_prompt_for_unknown_returns_default() {
        assert_eq!(system_prompt_for_model("unknown-model-xyz"), PROMPT_DEFAULT);
    }

    #[test]
    fn assemble_system_includes_all_parts() {
        let agent = AgentInfo {
            prompt: Some("Agent-specific instructions.".into()),
            ..Default::default()
        };
        let result = assemble_system(
            "claude-3-5-sonnet",
            &agent,
            Some("User instructions."),
            "/workspace/project",
            true,
            "linux",
            "2026-01-01",
        );

        assert!(result.contains(PROMPT_ANTHROPIC));
        assert!(result.contains("Agent-specific instructions."));
        assert!(result.contains("User instructions."));
        assert!(result.contains("<env>"));
        assert!(result.contains("/workspace/project"));
        assert!(result.contains("2026-01-01"));
    }

    #[test]
    fn environment_block_contains_working_directory_and_date() {
        let block = environment_block("/home/user/project", false, "darwin", "2026-03-27");
        assert!(block.contains("/home/user/project"));
        assert!(block.contains("2026-03-27"));
        assert!(block.contains("darwin"));
        assert!(block.contains("false"));
    }

    #[test]
    fn assemble_system_skips_absent_optional_parts() {
        let agent = AgentInfo::default();
        let result = assemble_system("gpt-4o", &agent, None, "/tmp", false, "linux", "2026-01-01");
        // Should contain base prompt and env block
        assert!(result.contains(PROMPT_GPT));
        assert!(result.contains("<env>"));
        // No agent-specific prompt or user prompt since both are absent
        // The result is exactly: PROMPT_GPT + "\n\n" + env_block
        let expected = format!(
            "{}\n\n{}",
            PROMPT_GPT,
            environment_block("/tmp", false, "linux", "2026-01-01")
        );
        assert_eq!(result, expected);
    }
}
