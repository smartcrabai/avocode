/// Match `input` against a glob `pattern`.
///
/// Rules:
/// - `**` matches any sequence of characters including `/`
/// - `*`  matches any sequence except `/`
/// - `?`  matches any single character except `/`
/// - Everything else is a literal match
#[must_use]
pub fn wildcard_match(input: &str, pattern: &str) -> bool {
    matches_bytes(input.as_bytes(), pattern.as_bytes(), true)
}

fn matches_bytes(input: &[u8], pattern: &[u8], at_segment_start: bool) -> bool {
    match (input.first(), pattern.first()) {
        (_, None) => input.is_empty(),
        (None, Some(b'*')) if pattern.get(1) == Some(&b'*') => {
            matches_bytes(input, &pattern[2..], at_segment_start)
                || matches_bytes(input, &pattern[1..], at_segment_start)
        }
        (None, Some(b'*')) => matches_bytes(input, &pattern[1..], at_segment_start),
        (None, _) => false,
        // `**/` may skip zero segments, but only at a path boundary to prevent
        // `**/.env` from matching mid-component like `foo.env`.
        (Some(_), Some(b'*')) if pattern.get(1) == Some(&b'*') && pattern.get(2) == Some(&b'/') => {
            let skip = at_segment_start && matches_bytes(input, &pattern[3..], at_segment_start);
            let ic = input[0];
            let consume_stay = matches_bytes(&input[1..], pattern, ic == b'/');
            skip || consume_stay
        }
        (Some(_), Some(b'*')) if pattern.get(1) == Some(&b'*') => {
            let ic = input[0];
            matches_bytes(&input[1..], pattern, ic == b'/')
                || matches_bytes(input, &pattern[2..], at_segment_start)
        }
        (Some(&ic), Some(b'*')) => {
            if ic == b'/' {
                matches_bytes(input, &pattern[1..], at_segment_start)
            } else {
                matches_bytes(&input[1..], pattern, false)
                    || matches_bytes(input, &pattern[1..], at_segment_start)
            }
        }
        (Some(&ic), Some(b'?')) => ic != b'/' && matches_bytes(&input[1..], &pattern[1..], false),
        (Some(&ic), Some(&pc)) => ic == pc && matches_bytes(&input[1..], &pattern[1..], ic == b'/'),
    }
}

#[cfg(test)]
mod tests {
    use super::wildcard_match;

    #[test]
    fn star_matches_simple_name() {
        assert!(wildcard_match("foo", "*"));
    }

    #[test]
    fn star_does_not_cross_slash() {
        assert!(!wildcard_match("foo/bar", "*"));
    }

    #[test]
    fn double_star_crosses_slash() {
        assert!(wildcard_match("foo/bar", "**"));
    }

    #[test]
    fn double_star_slash_matches_nested() {
        assert!(wildcard_match("foo/bar/baz", "**/baz"));
    }

    #[test]
    fn double_star_env_matches_nested() {
        assert!(wildcard_match("foo/.env", "**/.env"));
    }

    #[test]
    fn double_star_env_matches_top_level() {
        assert!(wildcard_match(".env", "**/.env"));
    }

    #[test]
    fn double_star_env_no_match_wrong_name() {
        assert!(!wildcard_match("foo.env", "**/.env"));
    }

    #[test]
    fn question_mark_matches_single_char() {
        assert!(wildcard_match("foo", "f?o"));
    }

    #[test]
    fn question_mark_does_not_cross_slash() {
        assert!(!wildcard_match("f/o", "f?o"));
    }

    #[test]
    fn literal_match() {
        assert!(wildcard_match("hello", "hello"));
        assert!(!wildcard_match("hello", "world"));
    }

    #[test]
    fn double_star_env_with_extension() {
        assert!(wildcard_match("src/.env.local", "**/.env.*"));
        assert!(!wildcard_match("src/env.local", "**/.env.*"));
    }
}
