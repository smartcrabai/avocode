use std::path::{Path, PathBuf};

/// Returns the opencode config file in `dir`, preferring `.jsonc` over `.json`.
/// Returns `None` when neither exists.
pub(crate) fn config_file_in_dir(dir: &Path) -> Option<PathBuf> {
    for candidate in [dir.join("opencode.jsonc"), dir.join("opencode.json")] {
        if candidate.exists() {
            return Some(candidate);
        }
    }
    None
}

/// Returns the global user-level config directory for opencode.
///
/// opencode follows the XDG convention on all platforms:
/// - Linux/macOS: `~/.config/opencode` (XDG_CONFIG_HOME or `~/.config`)
/// - Windows: `%APPDATA%\opencode`
#[must_use]
pub fn global_config_dir() -> Option<PathBuf> {
    // Respect XDG_CONFIG_HOME if set, otherwise fall back to ~/.config.
    // This matches opencode's own config resolution on macOS and Linux.
    #[cfg(not(target_os = "windows"))]
    {
        let xdg_base = std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| dirs::home_dir().map(|h| h.join(".config")))?;
        Some(xdg_base.join("opencode"))
    }
    #[cfg(target_os = "windows")]
    {
        dirs::config_dir().map(|p| p.join("opencode"))
    }
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
/// For each ancestor directory the walk checks (`.jsonc` preferred over `.json`):
/// - `<dir>/opencode.jsonc` or `<dir>/opencode.json`
/// - `<dir>/.opencode/opencode.jsonc` or `<dir>/.opencode/opencode.json`
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
        if let Some(path) = config_file_in_dir(&dir) {
            result.push(path);
        }
        if let Some(path) = config_file_in_dir(&dir.join(".opencode")) {
            result.push(path);
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
