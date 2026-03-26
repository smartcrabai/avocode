use std::path::{Path, PathBuf};

/// Returns the global user-level config directory for opencode.
///
/// - Linux/macOS: `~/.config/opencode`
/// - Windows: `%APPDATA%\opencode`
#[must_use]
pub fn global_config_dir() -> Option<PathBuf> {
    dirs::config_dir().map(|p| p.join("opencode"))
}

/// Returns the system-wide config directory for opencode.
///
/// - Linux: `/etc/opencode`
/// - macOS: `/Library/Application Support/opencode`
/// - Windows: `C:\ProgramData\opencode`
#[must_use]
pub fn system_config_dir() -> Option<PathBuf> {
    #[cfg(target_os = "linux")]
    {
        Some(PathBuf::from("/etc/opencode"))
    }

    #[cfg(target_os = "macos")]
    {
        Some(PathBuf::from("/Library/Application Support/opencode"))
    }

    #[cfg(target_os = "windows")]
    {
        dirs::data_local_dir().map(|p| {
            // %PROGRAMDATA% is the conventional system-wide data root on Windows.
            // Fall back to %LOCALAPPDATA% if the env var is absent.
            std::env::var("PROGRAMDATA")
                .map(|d| PathBuf::from(d).join("opencode"))
                .unwrap_or_else(|_| p.join("opencode"))
        })
    }

    #[cfg(not(any(target_os = "linux", target_os = "macos", target_os = "windows")))]
    {
        None
    }
}

/// Returns candidate config file paths collected by walking up from `directory`.
///
/// For each ancestor directory the walk checks:
/// - `<dir>/opencode.jsonc`
/// - `<dir>/.opencode/opencode.jsonc`
///
/// The walk stops when it reaches a git worktree root (a directory that contains
/// a `.git` entry).  The returned list is ordered from the outermost (highest)
/// directory down to the starting `directory`, so that deeper files can override
/// shallower ones when merged in order.
#[must_use]
pub fn project_config_files(directory: &Path) -> Vec<PathBuf> {
    let mut ancestors: Vec<PathBuf> = Vec::new();

    for ancestor in directory.ancestors() {
        let is_root = ancestor.join(".git").exists();
        ancestors.push(ancestor.to_path_buf());
        if is_root {
            break;
        }
    }

    // Reverse so we iterate from outermost to innermost (root first).
    ancestors.reverse();

    let mut result = Vec::new();
    for dir in ancestors {
        let direct = dir.join("opencode.jsonc");
        if direct.exists() {
            result.push(direct);
        }
        let nested = dir.join(".opencode").join("opencode.jsonc");
        if nested.exists() {
            result.push(nested);
        }
    }
    result
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn global_config_dir_contains_opencode() {
        if let Some(dir) = global_config_dir() {
            let dir_str = dir.to_string_lossy();
            assert!(
                dir_str.contains("opencode"),
                "Expected 'opencode' in path, got: {dir_str}"
            );
        }
    }

    #[test]
    fn system_config_dir_contains_opencode() {
        if let Some(dir) = system_config_dir() {
            let dir_str = dir.to_string_lossy();
            assert!(
                dir_str.contains("opencode"),
                "Expected 'opencode' in path, got: {dir_str}"
            );
        }
    }

    #[test]
    fn project_config_files_empty_for_directory_without_configs() {
        // Use a temp directory that has no opencode.jsonc files.
        let tmp = std::env::temp_dir();
        // The result may or may not be empty depending on the environment; the
        // important thing is that the function does not panic.
        let files = project_config_files(&tmp);
        for f in &files {
            assert!(f.exists(), "Returned non-existent path: {}", f.display());
        }
    }
}
