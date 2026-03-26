use std::{
    future::Future,
    pin::Pin,
    sync::{Arc, atomic::AtomicBool},
};

#[derive(Debug, Clone)]
pub struct ToolContext {
    pub session_id: String,
    pub message_id: String,
    pub agent: String,
    pub directory: std::path::PathBuf,
    pub abort: Arc<AtomicBool>,
}

impl ToolContext {
    #[must_use]
    pub fn new(directory: std::path::PathBuf) -> Self {
        Self {
            session_id: String::new(),
            message_id: String::new(),
            agent: "build".into(),
            directory,
            abort: Arc::new(AtomicBool::new(false)),
        }
    }

    #[must_use]
    pub fn is_aborted(&self) -> bool {
        self.abort.load(std::sync::atomic::Ordering::Relaxed)
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ToolOutput {
    pub title: String,
    pub output: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub metadata: Option<serde_json::Value>,
}

pub const MAX_OUTPUT_LINES: usize = 1000;
pub const MAX_OUTPUT_BYTES: usize = 512 * 1024;

/// Truncate output to the first limit hit (lines or bytes).
/// Appends "\n[output truncated]" if truncated.
#[must_use]
pub fn truncate_output(s: &str, max_lines: usize, max_bytes: usize) -> String {
    let mut line_count = 0usize;
    let mut byte_count = 0usize;
    for (i, ch) in s.char_indices() {
        byte_count = i + ch.len_utf8();
        if ch == '\n' {
            line_count += 1;
            if line_count >= max_lines || byte_count >= max_bytes {
                return format!("{}\n[output truncated]", &s[..i]);
            }
        }
    }
    if byte_count >= max_bytes {
        let truncated = &s[..max_bytes.min(s.len())];
        return format!("{truncated}\n[output truncated]");
    }
    s.to_owned()
}

/// Resolve a path string relative to a base directory, or use it as-is if absolute.
#[must_use]
pub fn resolve_path(path_str: &str, base: &std::path::Path) -> std::path::PathBuf {
    let p = std::path::Path::new(path_str);
    if p.is_absolute() {
        p.to_path_buf()
    } else {
        base.join(p)
    }
}

pub trait Tool: Send + Sync {
    fn id(&self) -> &'static str;
    fn description(&self) -> &'static str;
    fn parameters_schema(&self) -> serde_json::Value;
    fn execute<'a>(
        &'a self,
        args: serde_json::Value,
        ctx: &'a ToolContext,
    ) -> Pin<Box<dyn Future<Output = Result<ToolOutput, crate::tool::ToolError>> + Send + 'a>>;
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn truncate_by_line_count() {
        let input = "a\nb\nc\nd\ne\nf";
        let result = truncate_output(input, 3, usize::MAX);
        assert!(result.ends_with("\n[output truncated]"));
        assert!(!result.contains("d\n"));
    }

    #[test]
    fn no_truncate_under_limits() {
        let input = "hello\nworld\n";
        let result = truncate_output(input, 1000, usize::MAX);
        assert_eq!(result, input);
    }

    #[test]
    fn truncate_by_bytes() {
        let input = "abcdefghij";
        let result = truncate_output(input, usize::MAX, 5);
        assert!(result.ends_with("\n[output truncated]"));
    }
}
