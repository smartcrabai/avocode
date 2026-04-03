#![expect(dead_code)]
#![expect(clippy::expect_used)]

use std::path::{Path, PathBuf};
use tempfile::TempDir;

/// Isolated filesystem environment for integration tests.
///
/// Provides dedicated temp directories for `HOME` and the project root.
/// All platform-specific subdirectories (config, cache, data) are computed
/// relative to `home_dir` so that overriding `HOME` in a child process is
/// sufficient to fully isolate it from the developer's real state.
///
/// A `.git` directory is created inside `project_dir` so that
/// `config::paths::project_config_files()` stops its ancestor walk at the
/// project root and does not accidentally pick up real `opencode.jsonc` files
/// higher in the directory tree.
pub struct TestEnv {
    /// Temporary home directory -- keep alive for the life of the test.
    pub home_dir: TempDir,
    /// Temporary project directory -- keep alive for the life of the test.
    pub project_dir: TempDir,
}

impl TestEnv {
    /// Create a fresh isolated environment.
    pub fn new() -> Self {
        let home_dir = tempfile::tempdir().expect("failed to create temp home dir");
        let project_dir = tempfile::tempdir().expect("failed to create temp project dir");

        // Stop `project_config_files()` ancestor walk here.
        std::fs::create_dir(project_dir.path().join(".git"))
            .expect("failed to create .git marker in temp project dir");

        Self {
            home_dir,
            project_dir,
        }
    }

    /// Absolute path to the temp project directory.
    pub fn project_path(&self) -> &Path {
        self.project_dir.path()
    }

    // ---- config file helpers ----

    /// Write `opencode.jsonc` into the project directory with model
    /// `openai/gpt-4o` and the given `base_url` for the `openai` provider.
    pub fn write_openai_config(&self, base_url: &str) {
        let config = format!(
            r#"{{
  "model": "openai/gpt-4o",
  "provider": {{
    "openai": {{
      "api_key": "test",
      "base_url": "{base_url}"
    }}
  }}
}}"#
        );
        std::fs::write(self.project_dir.path().join("opencode.jsonc"), config)
            .expect("failed to write opencode.jsonc");
    }

    /// Write `opencode.jsonc` into the project directory selecting the
    /// `openai/credit-error` model (triggers a quota error from the mock).
    pub fn write_credit_error_config(&self, base_url: &str) {
        let config = format!(
            r#"{{
  "model": "openai/credit-error",
  "provider": {{
    "openai": {{
      "api_key": "test",
      "base_url": "{base_url}"
    }}
  }}
}}"#
        );
        std::fs::write(self.project_dir.path().join("opencode.jsonc"), config)
            .expect("failed to write opencode.jsonc (credit-error)");
    }

    // ---- cache helpers ----

    /// Pre-seed the models cache at the platform-specific path that
    /// `provider::models_dev::cache_path()` resolves to when the child
    /// process runs with `HOME` set to `self.home_dir`.
    ///
    /// The cache contains `openai/gpt-4o` and `openai/credit-error` so that
    /// TUI startup never reaches out to live `models.dev`.
    ///
    /// Format: `Vec<ProviderInfo>` serialised as JSON
    /// (see `src/provider/schema.rs` and `src/provider/models_dev.rs`).
    pub fn write_models_cache(&self) {
        self.write_models_cache_json(&serde_json::json!([{
            "id": "openai",
            "name": "OpenAI",
            "env": ["OPENAI_API_KEY"],
            "models": [
                openai_model_entry("gpt-4o", "GPT-4o", true, true),
                openai_model_entry("credit-error", "credit-error", false, false),
            ]
        }]));
    }

    // ---- process environment helpers ----

    /// Returns the minimal set of environment variable overrides to pass to
    /// a child `avocode` process so it uses the isolated temp directories.
    ///
    /// On Unix, setting `HOME` is sufficient: `dirs` derives all
    /// platform-specific subdirectories (config, cache, data) from it.
    /// On Windows, `USERPROFILE`, `LOCALAPPDATA`, and `APPDATA` are also set
    /// so that `dirs` resolves platform-specific paths under the temp tree.
    pub fn env_overrides(&self) -> Vec<(String, String)> {
        let home = self.home_dir.path();
        #[cfg(target_os = "windows")]
        let mut overrides = vec![("HOME".to_owned(), home.display().to_string())];
        #[cfg(not(target_os = "windows"))]
        let overrides = vec![("HOME".to_owned(), home.display().to_string())];
        #[cfg(target_os = "windows")]
        {
            overrides.push(("USERPROFILE".to_owned(), home.display().to_string()));
            overrides.push((
                "LOCALAPPDATA".to_owned(),
                home.join("AppData").join("Local").display().to_string(),
            ));
            overrides.push((
                "APPDATA".to_owned(),
                home.join("AppData").join("Roaming").display().to_string(),
            ));
        }
        overrides
    }

    /// Pre-seed the models cache with **two** `OpenAI` models (`gpt-4o` and
    /// `gpt-3.5-turbo`) so that model-picker tests have two selectable entries
    /// to navigate between.
    ///
    /// Does **not** add non-openai providers to avoid changing the default
    /// model ordering expected by unrelated tests.
    pub fn write_two_openai_models_cache(&self) {
        self.write_models_cache_json(&serde_json::json!([{
            "id": "openai",
            "name": "OpenAI",
            "env": ["OPENAI_API_KEY"],
            "models": [
                openai_model_entry("gpt-4o", "GPT-4o", true, true),
                openai_model_entry("gpt-3.5-turbo", "GPT-3.5 Turbo", false, true),
            ]
        }]));
    }

    // ---- private helpers ----

    /// Write `data` as `models.json` into the platform-specific avocode cache dir,
    /// creating the directory if it does not yet exist.
    fn write_models_cache_json(&self, data: &serde_json::Value) {
        let cache_dir = self.avocode_cache_dir();
        std::fs::create_dir_all(&cache_dir).expect("failed to create avocode cache dir");
        std::fs::write(cache_dir.join("models.json"), data.to_string())
            .expect("failed to write models.json cache");
    }

    // ---- private path helpers ----

    /// Path equivalent to `dirs::cache_dir().unwrap().join("avocode")` when
    /// the child process runs with `HOME = self.home_dir`.
    fn avocode_cache_dir(&self) -> PathBuf {
        platform_cache_base(self.home_dir.path()).join("avocode")
    }
}

/// Build a single `OpenAI` model entry for the models-cache JSON fixture.
fn openai_model_entry(id: &str, name: &str, vision: bool, tools: bool) -> serde_json::Value {
    serde_json::json!({
        "id": id,
        "name": name,
        "provider_id": "openai",
        "family": null,
        "capabilities": {
            "tools": tools,
            "vision": vision,
            "reasoning": false,
            "streaming": true,
            "temperature": true,
            "attachment": false,
            "computer_use": false
        },
        "cost": { "input": 0.0, "output": 0.0, "cache_read": null, "cache_write": null },
        "context_length": null,
        "output_length": null,
        "status": "active"
    })
}

/// Returns the platform-specific base cache directory given a `home` path.
///
/// - macOS: `<home>/Library/Caches`
/// - Windows: `<home>/AppData/Local`
/// - Other (Linux, etc.): `<home>/.cache`
fn platform_cache_base(home: &Path) -> PathBuf {
    if cfg!(target_os = "macos") {
        home.join("Library").join("Caches")
    } else if cfg!(target_os = "windows") {
        home.join("AppData").join("Local")
    } else {
        home.join(".cache")
    }
}
