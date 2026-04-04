//! Skill discovery and slash-skill resolution.
//!
//! Discovers `SKILL.md` files from documented locations compatible with
//! `OpenCode`'s skill system. Skills are exposed as slash-command prompt
//! templates in the TUI and expanded transparently in the processor.

use std::path::{Path, PathBuf};

/// Metadata and content of a discovered skill.
#[derive(Debug, Clone)]
pub struct SkillInfo {
    /// Skill name (must match directory basename).
    pub name: String,
    /// Human-readable description.
    pub description: String,
    /// Full skill body (after frontmatter).
    pub content: String,
    /// Path to the `SKILL.md` file.
    pub location: PathBuf,
}

/// Result of resolving a potential slash-skill in user input.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ResolvedSlashInput {
    /// Input was expanded from a slash skill.
    Expanded(String),
    /// Input did not match any slash skill and should be used as-is.
    Unchanged(String),
}

/// Discover all skills visible from the given project directory.
///
/// Walks ancestor directories (git-root bounded) looking for skill directories
/// in the documented locations, then checks global user-level directories.
/// Returns skills in deterministic precedence order (last-wins on duplicate names).
#[must_use]
pub fn discover(directory: &Path) -> Vec<SkillInfo> {
    let mut skills: Vec<SkillInfo> = Vec::new();

    // 1. Global skill roots (lowest priority, overridden by project-local).
    for root in global_skill_roots() {
        scan_skill_root(&root, &mut skills);
    }

    // 2. Walk ancestors outermost-to-innermost, git-root bounded.
    let mut ancestors: Vec<PathBuf> = Vec::new();
    for ancestor in directory.ancestors() {
        let is_root = ancestor.join(".git").exists();
        ancestors.push(ancestor.to_path_buf());
        if is_root {
            break;
        }
    }
    ancestors.reverse();

    for dir in ancestors {
        // Order: .agents < .claude < .opencode (last wins).
        for sub in [".agents/skills", ".claude/skills", ".opencode/skills"] {
            scan_skill_root(&dir.join(sub), &mut skills);
        }
    }

    deduplicate_skills(skills)
}

/// Resolve a potential slash-skill in user input.
///
/// If `input` starts with `/<skill-name>` (a single slash-token matching a
/// known skill), returns [`ResolvedSlashInput::Expanded`] with the skill body
/// plus any trailing arguments appended after a blank line.
/// Otherwise returns [`ResolvedSlashInput::Unchanged`].
pub fn resolve_slash_skill(skills: &[SkillInfo], input: &str) -> ResolvedSlashInput {
    let Some(rest) = input.strip_prefix('/') else {
        return ResolvedSlashInput::Unchanged(input.to_string());
    };

    let (skill_name, trailing) = match rest.find(char::is_whitespace) {
        Some(pos) => (&rest[..pos], rest[pos..].trim_start()),
        None => (rest, ""),
    };

    if skill_name.is_empty() {
        return ResolvedSlashInput::Unchanged(input.to_string());
    }

    let Some(skill) = skills.iter().find(|s| s.name == skill_name) else {
        return ResolvedSlashInput::Unchanged(input.to_string());
    };

    let expanded_text = if trailing.is_empty() {
        skill.content.clone()
    } else {
        format!("{}\n\n{}", skill.content, trailing)
    };

    ResolvedSlashInput::Expanded(expanded_text)
}

// ================================================================
// Private helpers
// ================================================================

/// Return global skill root directories in precedence order (lowest first).
fn global_skill_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();

    if let Some(home) = dirs::home_dir() {
        roots.push(home.join(".agents").join("skills"));
        roots.push(home.join(".claude").join("skills"));
    }

    // XDG_CONFIG_HOME/opencode/skills (or ~/.config/opencode/skills).
    #[cfg(not(target_os = "windows"))]
    {
        let xdg_base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")));
        if let Some(base) = xdg_base {
            roots.push(base.join("opencode").join("skills"));
        }
    }

    roots
}

/// Scan a skill root directory and append valid skills to `out`.
fn scan_skill_root(root: &Path, out: &mut Vec<SkillInfo>) {
    let Ok(entries) = std::fs::read_dir(root) else {
        return;
    };
    for entry in entries.flatten() {
        let path = entry.path();
        if !path.is_dir() {
            continue;
        }
        let skill_file = path.join("SKILL.md");
        let Some(dir_name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let Ok(content) = std::fs::read_to_string(&skill_file) else {
            continue;
        };
        let Some((name, description, body)) = parse_skill_file(&content) else {
            continue;
        };
        if dir_name != name {
            continue;
        }
        if !is_valid_skill_name(&name) {
            continue;
        }
        if description.is_empty() || description.len() > 1024 {
            continue;
        }
        out.push(SkillInfo {
            name,
            description,
            content: body,
            location: skill_file,
        });
    }
}

/// Parse a `SKILL.md` file and return `(name, description, body)`.
///
/// Returns `None` if the frontmatter is missing, malformed, or lacks the
/// required `name`/`description` fields.
fn parse_skill_file(content: &str) -> Option<(String, String, String)> {
    let rest = content.strip_prefix("---")?;
    let end = rest.find("---")?;
    let frontmatter = &rest[..end];
    let body = rest[end + 3..]
        .strip_prefix('\n')
        .unwrap_or(&rest[end + 3..]);

    let mut name: Option<String> = None;
    let mut description: Option<String> = None;

    for line in frontmatter.lines() {
        let line = line.trim();
        if line.is_empty() {
            continue;
        }
        if let Some(value) = line.strip_prefix("name:") {
            name = Some(parse_yaml_value(value.trim()));
        } else if let Some(value) = line.strip_prefix("description:") {
            description = Some(parse_yaml_value(value.trim()));
        }
    }

    Some((name?, description?, body.to_string()))
}

/// Strip optional surrounding quotes from a YAML scalar value.
fn parse_yaml_value(raw: &str) -> String {
    let trimmed = raw.trim();
    if trimmed.len() >= 2 {
        let first = trimmed.as_bytes()[0];
        let last = trimmed.as_bytes()[trimmed.len() - 1];
        if (first == b'"' && last == b'"') || (first == b'\'' && last == b'\'') {
            return trimmed[1..trimmed.len() - 1].to_string();
        }
    }
    trimmed.to_string()
}

/// Validate a skill name: `^[a-z0-9]+(-[a-z0-9]+)*$`, length 1..=64.
fn is_valid_skill_name(name: &str) -> bool {
    if name.is_empty() || name.len() > 64 {
        return false;
    }
    let bytes = name.as_bytes();
    if bytes[0] == b'-' || bytes[bytes.len() - 1] == b'-' {
        return false;
    }
    let mut prev_hyphen = false;
    for &b in bytes {
        match b {
            b'a'..=b'z' | b'0'..=b'9' => prev_hyphen = false,
            b'-' => {
                if prev_hyphen {
                    return false;
                }
                prev_hyphen = true;
            }
            _ => return false,
        }
    }
    true
}

/// Deduplicate skills by name, keeping the last occurrence (last-wins).
/// Preserves first-occurrence position in the output order.
fn deduplicate_skills(skills: Vec<SkillInfo>) -> Vec<SkillInfo> {
    let mut index: std::collections::HashMap<String, usize> = std::collections::HashMap::new();
    let mut result: Vec<SkillInfo> = Vec::new();
    for skill in skills {
        if let Some(&i) = index.get(&skill.name) {
            result[i] = skill;
        } else {
            index.insert(skill.name.clone(), result.len());
            result.push(skill);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    #![allow(clippy::expect_used, clippy::unwrap_used)]
    use super::*;

    // Helpers

    /// Create `<base>/<name>/SKILL.md` with YAML frontmatter and body.
    fn create_skill(
        base: &Path,
        name: &str,
        description: &str,
        content: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = base.join(name);
        std::fs::create_dir_all(&dir)?;
        let body = format!("---\nname: {name}\ndescription: {description}\n---\n{content}");
        std::fs::write(dir.join("SKILL.md"), body)?;
        Ok(())
    }

    /// Create a project root with a `.git` marker directory.
    fn create_project(dir: &Path) -> Result<(), Box<dyn std::error::Error>> {
        std::fs::create_dir_all(dir)?;
        std::fs::create_dir(dir.join(".git"))?;
        Ok(())
    }

    /// Create a SKILL.md with arbitrary frontmatter text (may be invalid).
    fn create_raw_skill(
        base: &Path,
        dir_name: &str,
        frontmatter: &str,
        content: &str,
    ) -> Result<(), Box<dyn std::error::Error>> {
        let dir = base.join(dir_name);
        std::fs::create_dir_all(&dir)?;
        let body = format!("---\n{frontmatter}\n---\n{content}");
        std::fs::write(dir.join("SKILL.md"), body)?;
        Ok(())
    }

    /// Call `discover` with HOME and `XDG_CONFIG_HOME` isolated to prevent real
    /// user-level skills from leaking into test results.
    fn isolated_discover(directory: &Path) -> Vec<SkillInfo> {
        let home = tempfile::tempdir().expect("temp home");
        let home_str = home.path().to_str().expect("home path");
        temp_env::with_vars(
            [("HOME", Some(home_str)), ("XDG_CONFIG_HOME", None::<&str>)],
            || discover(directory),
        )
    }

    // discover -- project-local locations

    #[test]
    fn discover_finds_skill_in_project_opencode_dir() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            "my-skill",
            "desc",
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "my-skill");
        assert_eq!(skills[0].description, "desc");
        assert_eq!(skills[0].content, "body");
        Ok(())
    }

    #[test]
    fn discover_finds_skill_in_project_claude_dir() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_skill(
            &tmp.path().join(".claude").join("skills"),
            "claude-skill",
            "desc",
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "claude-skill");
        Ok(())
    }

    #[test]
    fn discover_finds_skill_in_project_agents_dir() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_skill(
            &tmp.path().join(".agents").join("skills"),
            "agent-skill",
            "desc",
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "agent-skill");
        Ok(())
    }

    // discover -- global locations

    #[test]
    fn discover_finds_skill_in_global_opencode_config() -> Result<(), Box<dyn std::error::Error>> {
        let home = tempfile::tempdir()?;
        let xdg = home.path().join(".config");
        create_skill(
            &xdg.join("opencode").join("skills"),
            "global-skill",
            "desc",
            "body",
        )?;

        let skills = temp_env::with_vars(
            [
                ("HOME", home.path().to_str()),
                ("XDG_CONFIG_HOME", xdg.to_str()),
            ],
            || discover(home.path()),
        );
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "global-skill");
        Ok(())
    }

    #[test]
    fn discover_finds_skill_in_global_claude_dir() -> Result<(), Box<dyn std::error::Error>> {
        let home = tempfile::tempdir()?;
        create_skill(
            &home.path().join(".claude").join("skills"),
            "claude-global",
            "desc",
            "body",
        )?;

        let skills = temp_env::with_vars([("HOME", Some(home.path().to_str().unwrap()))], || {
            discover(home.path())
        });
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "claude-global");
        Ok(())
    }

    #[test]
    fn discover_finds_skill_in_global_agents_dir() -> Result<(), Box<dyn std::error::Error>> {
        let home = tempfile::tempdir()?;
        create_skill(
            &home.path().join(".agents").join("skills"),
            "agents-global",
            "desc",
            "body",
        )?;

        let skills = temp_env::with_vars([("HOME", Some(home.path().to_str().unwrap()))], || {
            discover(home.path())
        });
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "agents-global");
        Ok(())
    }

    // discover -- frontmatter validation

    #[test]
    fn discover_skips_skill_without_frontmatter() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        // Write a SKILL.md without YAML frontmatter delimiters.
        let skill_dir = tmp.path().join(".opencode").join("skills").join("no-fm");
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "just some text without frontmatter",
        )?;

        let skills = isolated_discover(tmp.path());
        assert!(
            skills.is_empty(),
            "skills without frontmatter should be skipped"
        );
        Ok(())
    }

    #[test]
    fn discover_skips_skill_with_missing_name() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_raw_skill(
            &tmp.path().join(".opencode").join("skills"),
            "no-name",
            "description: has desc but no name",
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert!(
            skills.is_empty(),
            "skill without 'name' field should be skipped"
        );
        Ok(())
    }

    #[test]
    fn discover_skips_skill_with_missing_description() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_raw_skill(
            &tmp.path().join(".opencode").join("skills"),
            "no-desc",
            "name: no-desc",
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert!(
            skills.is_empty(),
            "skill without 'description' field should be skipped"
        );
        Ok(())
    }

    #[test]
    fn discover_ignores_unknown_frontmatter_keys() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_raw_skill(
            &tmp.path().join(".opencode").join("skills"),
            "extra-keys",
            "name: extra-keys\ndescription: desc\nversion: \"1.0\"\nauthor: test",
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "extra-keys");
        Ok(())
    }

    // discover -- name validation

    #[test]
    fn discover_skips_name_directory_mismatch() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_raw_skill(
            &tmp.path().join(".opencode").join("skills"),
            "dir-name",
            "name: different-name\ndescription: desc",
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert!(
            skills.is_empty(),
            "skill where directory name != frontmatter name should be skipped"
        );
        Ok(())
    }

    #[test]
    fn discover_skips_uppercase_name() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_raw_skill(
            &tmp.path().join(".opencode").join("skills"),
            "MySkill",
            "name: MySkill\ndescription: desc",
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert!(
            skills.is_empty(),
            "uppercase name should be skipped (must match ^[a-z0-9]+(-[a-z0-9]+)*$)"
        );
        Ok(())
    }

    #[test]
    fn discover_skips_name_with_underscores() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_raw_skill(
            &tmp.path().join(".opencode").join("skills"),
            "my_skill",
            "name: my_skill\ndescription: desc",
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert!(
            skills.is_empty(),
            "name with underscores should be skipped (must match ^[a-z0-9]+(-[a-z0-9]+)*$)"
        );
        Ok(())
    }

    #[test]
    fn discover_skips_empty_name() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        // Empty directory name is not practical, but we can write a file with
        // empty name in frontmatter.
        let skill_dir = tmp.path().join(".opencode").join("skills").join("nonempty");
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\nname: \"\"\ndescription: desc\n---\nbody",
        )?;

        let skills = isolated_discover(tmp.path());
        assert!(skills.is_empty(), "empty name should be skipped");
        Ok(())
    }

    #[test]
    fn discover_skips_name_exceeding_64_chars() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        let long_name = "a".repeat(65);
        create_raw_skill(
            &tmp.path().join(".opencode").join("skills"),
            &long_name,
            &format!("name: {long_name}\ndescription: desc"),
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert!(skills.is_empty(), "name > 64 chars should be skipped");
        Ok(())
    }

    #[test]
    fn discover_accepts_name_at_max_64_chars() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        let max_name = "a".repeat(64);
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            &max_name,
            "desc",
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, max_name);
        Ok(())
    }

    #[test]
    fn discover_accepts_hyphenated_name() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            "my-cool-skill",
            "desc",
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].name, "my-cool-skill");
        Ok(())
    }

    // discover -- precedence (deterministic last-wins)

    #[test]
    fn discover_project_local_wins_over_global() -> Result<(), Box<dyn std::error::Error>> {
        let home = tempfile::tempdir()?;
        let project = tempfile::tempdir()?;
        create_project(project.path())?;

        // Global skill.
        create_skill(
            &home.path().join(".config").join("opencode").join("skills"),
            "dup-skill",
            "global desc",
            "global body",
        )?;
        // Project-local skill (should win).
        create_skill(
            &project.path().join(".opencode").join("skills"),
            "dup-skill",
            "local desc",
            "local body",
        )?;

        let skills = temp_env::with_vars(
            [
                ("HOME", home.path().to_str()),
                ("XDG_CONFIG_HOME", home.path().join(".config").to_str()),
            ],
            || discover(project.path()),
        );
        // Last-wins: project-local appears after global in the iteration order,
        // so it should override.
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "local desc");
        assert_eq!(skills[0].content, "local body");
        Ok(())
    }

    #[test]
    fn discover_opencode_wins_over_claude_at_same_depth() -> Result<(), Box<dyn std::error::Error>>
    {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;

        // Same project, two directories with same skill name.
        create_skill(
            &tmp.path().join(".claude").join("skills"),
            "dup-skill",
            "claude desc",
            "claude body",
        )?;
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            "dup-skill",
            "opencode desc",
            "opencode body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert_eq!(skills.len(), 1);
        assert_eq!(skills[0].description, "opencode desc");
        Ok(())
    }

    #[test]
    fn discover_returns_deduped_skills() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            "alpha",
            "desc-a",
            "body-a",
        )?;
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            "beta",
            "desc-b",
            "body-b",
        )?;

        let skills = isolated_discover(tmp.path());
        assert_eq!(skills.len(), 2);
        let names: Vec<&str> = skills.iter().map(|s| s.name.as_str()).collect();
        assert!(names.contains(&"alpha"));
        assert!(names.contains(&"beta"));
        Ok(())
    }

    // discover -- edge cases

    #[test]
    fn discover_returns_empty_for_directory_without_skills()
    -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        let skills = isolated_discover(tmp.path());
        assert!(skills.is_empty());
        Ok(())
    }

    #[test]
    fn discover_skips_malformed_yaml_frontmatter() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        let skill_dir = tmp.path().join(".opencode").join("skills").join("bad-yaml");
        std::fs::create_dir_all(&skill_dir)?;
        std::fs::write(
            skill_dir.join("SKILL.md"),
            "---\n: invalid yaml [\n---\nbody",
        )?;

        let skills = isolated_discover(tmp.path());
        assert!(skills.is_empty(), "malformed YAML should be skipped");
        Ok(())
    }

    #[test]
    fn discover_skips_description_exceeding_1024_chars() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        let long_desc = "x".repeat(1025);
        create_raw_skill(
            &tmp.path().join(".opencode").join("skills"),
            "long-desc",
            &format!("name: long-desc\ndescription: {long_desc}"),
            "body",
        )?;

        let skills = isolated_discover(tmp.path());
        assert!(
            skills.is_empty(),
            "description > 1024 chars should be skipped"
        );
        Ok(())
    }

    // resolve_slash_skill -- expansion

    #[test]
    fn resolve_expands_known_skill() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            "greet",
            "greeting skill",
            "You are a helpful assistant.",
        )?;

        let result = resolve_slash_skill(&discover(tmp.path()), "/greet");
        assert_eq!(
            result,
            ResolvedSlashInput::Expanded("You are a helpful assistant.".to_string())
        );
        Ok(())
    }

    #[test]
    fn resolve_appends_trailing_args_after_blank_line() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            "greet",
            "greeting skill",
            "You are a helpful assistant.",
        )?;

        let result = resolve_slash_skill(&discover(tmp.path()), "/greet focus on Rust");
        assert_eq!(
            result,
            ResolvedSlashInput::Expanded(
                "You are a helpful assistant.\n\nfocus on Rust".to_string()
            )
        );
        Ok(())
    }

    #[test]
    fn resolve_returns_unchanged_for_unknown_skill() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;

        let result = resolve_slash_skill(&discover(tmp.path()), "/nonexistent-skill");
        assert_eq!(
            result,
            ResolvedSlashInput::Unchanged("/nonexistent-skill".to_string())
        );
        Ok(())
    }

    #[test]
    fn resolve_returns_unchanged_for_plain_text() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;

        let result = resolve_slash_skill(&discover(tmp.path()), "hello world");
        assert_eq!(
            result,
            ResolvedSlashInput::Unchanged("hello world".to_string())
        );
        Ok(())
    }

    #[test]
    fn resolve_returns_unchanged_when_slash_is_not_at_start()
    -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            "greet",
            "greeting",
            "body",
        )?;

        let result = resolve_slash_skill(&discover(tmp.path()), "use /greet");
        assert_eq!(
            result,
            ResolvedSlashInput::Unchanged("use /greet".to_string())
        );
        Ok(())
    }

    #[test]
    fn resolve_selects_correct_skill_among_multiple() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            "alpha",
            "first skill",
            "Alpha body.",
        )?;
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            "beta",
            "second skill",
            "Beta body.",
        )?;

        let result = resolve_slash_skill(&discover(tmp.path()), "/beta");
        assert_eq!(
            result,
            ResolvedSlashInput::Expanded("Beta body.".to_string())
        );
        Ok(())
    }

    #[test]
    fn resolve_expands_skill_with_only_slash_token() -> Result<(), Box<dyn std::error::Error>> {
        let tmp = tempfile::tempdir()?;
        create_project(tmp.path())?;
        create_skill(
            &tmp.path().join(".opencode").join("skills"),
            "review",
            "code review",
            "Review the code carefully.",
        )?;

        // Input is exactly "/review" with no trailing text.
        let result = resolve_slash_skill(&discover(tmp.path()), "/review");
        assert_eq!(
            result,
            ResolvedSlashInput::Expanded("Review the code carefully.".to_string())
        );
        Ok(())
    }
}
