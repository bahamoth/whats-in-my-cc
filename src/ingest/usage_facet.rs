//! usage_facet extractor — parses message.usage from a raw transcript line.

pub const PARSER_VERSION: &str = "usage_facet@v1";
pub const SCHEMA_VERSION: &str = "usage_facet.v1";

/// Token usage parsed from one assistant transcript line.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct UsageParsed {
    pub model: Option<String>,
    pub input_tokens: i64,
    pub cache_creation_input_tokens: i64,
    pub cache_read_input_tokens: i64,
    pub output_tokens: i64,
}

/// Parse `message.usage` + `message.model` from a full raw transcript line
/// (the JSON stored in `raw_event.payload`). Returns `None` when the line is
/// not an assistant message or carries no usage object.
pub fn parse_usage(raw_line: &serde_json::Value) -> Option<UsageParsed> {
    let usage = raw_line.pointer("/message/usage")?;
    let model = raw_line
        .pointer("/message/model")
        .and_then(|v| v.as_str())
        .map(String::from);
    let n = |k: &str| usage.get(k).and_then(|v| v.as_i64()).unwrap_or(0);
    Some(UsageParsed {
        model,
        input_tokens: n("input_tokens"),
        cache_creation_input_tokens: n("cache_creation_input_tokens"),
        cache_read_input_tokens: n("cache_read_input_tokens"),
        output_tokens: n("output_tokens"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn parses_usage_and_model_from_assistant_line() {
        let line = json!({
            "type": "assistant",
            "message": {
                "role": "assistant",
                "model": "claude-opus-4-8",
                "usage": {
                    "input_tokens": 2,
                    "cache_creation_input_tokens": 5836,
                    "cache_read_input_tokens": 94234,
                    "output_tokens": 665
                }
            }
        });
        let u = parse_usage(&line).expect("usage present");
        assert_eq!(u.model.as_deref(), Some("claude-opus-4-8"));
        assert_eq!(u.input_tokens, 2);
        assert_eq!(u.cache_creation_input_tokens, 5836);
        assert_eq!(u.cache_read_input_tokens, 94234);
        assert_eq!(u.output_tokens, 665);
    }

    #[test]
    fn returns_none_when_no_usage() {
        let line = json!({ "type": "user", "message": { "role": "user" } });
        assert_eq!(parse_usage(&line), None);
    }

    #[test]
    fn missing_token_fields_default_to_zero() {
        let line = json!({
            "message": { "model": "claude-haiku-4-5-20251001", "usage": { "output_tokens": 10 } }
        });
        let u = parse_usage(&line).expect("usage present");
        assert_eq!(u.output_tokens, 10);
        assert_eq!(u.input_tokens, 0);
        assert_eq!(u.cache_read_input_tokens, 0);
    }
}
