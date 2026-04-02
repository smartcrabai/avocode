/// Parses a qualified `"provider_id/model_id"` string into its two components.
///
/// Returns `Some((provider_id, model_id))` when `qualified` contains at least
/// one `/` with a non-empty string on each side of the first `/`.
/// Returns `None` otherwise (empty string, no `/`, empty provider, or empty
/// model segment).
///
/// When the model id itself contains a `/` (e.g. `"openai/some/variant"`),
/// the provider id is the part before the first `/` and the model id is
/// everything after it.
#[must_use]
pub fn parse_qualified_model(qualified: &str) -> Option<(String, String)> {
    let pos = qualified.find('/')?;
    let provider = &qualified[..pos];
    let model = &qualified[pos + 1..];
    if provider.is_empty() || model.is_empty() {
        return None;
    }
    Some((provider.to_owned(), model.to_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    // ---- happy path ----

    #[test]
    fn parse_standard_qualified_model_returns_provider_and_model() {
        let result = parse_qualified_model("openai/gpt-4o");
        assert_eq!(result, Some(("openai".to_owned(), "gpt-4o".to_owned())));
    }

    #[test]
    fn parse_anthropic_qualified_model() {
        let result = parse_qualified_model("anthropic/claude-sonnet-4-5");
        assert_eq!(
            result,
            Some(("anthropic".to_owned(), "claude-sonnet-4-5".to_owned()))
        );
    }

    #[test]
    fn parse_google_qualified_model() {
        let result = parse_qualified_model("google/gemini-1.5-pro");
        assert_eq!(
            result,
            Some(("google".to_owned(), "gemini-1.5-pro".to_owned()))
        );
    }

    #[test]
    fn parse_credit_error_model_for_negative_path_tests() {
        // The mock server's error-triggering model must parse as openai/credit-error.
        let result = parse_qualified_model("openai/credit-error");
        assert_eq!(
            result,
            Some(("openai".to_owned(), "credit-error".to_owned()))
        );
    }

    #[test]
    fn parse_preserves_case_in_provider_and_model_ids() {
        let result = parse_qualified_model("OpenAI/GPT-4o");
        assert_eq!(result, Some(("OpenAI".to_owned(), "GPT-4o".to_owned())));
    }

    #[test]
    fn parse_model_id_containing_additional_slash_uses_first_slash_as_separator() {
        let result = parse_qualified_model("openai/some/variant");
        assert_eq!(
            result,
            Some(("openai".to_owned(), "some/variant".to_owned()))
        );
    }

    // ---- error / edge cases ----

    #[test]
    fn parse_returns_none_for_unqualified_model_id() {
        assert_eq!(parse_qualified_model("gpt-4o"), None);
    }

    #[test]
    fn parse_returns_none_for_empty_string() {
        assert_eq!(parse_qualified_model(""), None);
    }

    #[test]
    fn parse_returns_none_for_slash_only() {
        assert_eq!(parse_qualified_model("/"), None);
    }

    #[test]
    fn parse_returns_none_when_provider_segment_is_empty() {
        // "/gpt-4o" has an empty provider
        assert_eq!(parse_qualified_model("/gpt-4o"), None);
    }

    #[test]
    fn parse_returns_none_when_model_segment_is_empty() {
        // "openai/" has an empty model id
        assert_eq!(parse_qualified_model("openai/"), None);
    }
}
