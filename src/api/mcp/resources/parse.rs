//! Slice-17 — URI parser for whats-in-my-cc:// resource scheme.

/// Parsed form of a resource URI.
#[derive(Debug, PartialEq, Eq)]
pub enum ResourceUri {
    /// whats-in-my-cc://sessions/{session_id}
    Session(String),
    /// whats-in-my-cc://sessions/{session_id}/findings
    SessionFindings(String),
    /// whats-in-my-cc://findings/{finding_id}
    Finding(String),
    /// whats-in-my-cc://file-lineage/{session_id}
    FileLineage(String),
    /// whats-in-my-cc://otel/traces/{trace_id}
    OtelTrace(String),
}

/// Parse a `whats-in-my-cc://` URI. Returns `None` for unrecognised patterns.
pub fn parse(uri: &str) -> Option<ResourceUri> {
    let rest = uri.strip_prefix("whats-in-my-cc://")?;
    // Split on '/' and match patterns.
    let parts: Vec<&str> = rest.splitn(4, '/').collect();
    match parts.as_slice() {
        // sessions/{session_id}
        ["sessions", session_id] if !session_id.is_empty() => {
            Some(ResourceUri::Session(session_id.to_string()))
        }
        // sessions/{session_id}/findings
        ["sessions", session_id, "findings"] => {
            Some(ResourceUri::SessionFindings(session_id.to_string()))
        }
        // findings/{finding_id}
        ["findings", finding_id] if !finding_id.is_empty() => {
            Some(ResourceUri::Finding(finding_id.to_string()))
        }
        // file-lineage/{session_id}
        ["file-lineage", session_id] if !session_id.is_empty() => {
            Some(ResourceUri::FileLineage(session_id.to_string()))
        }
        // otel/traces/{trace_id}
        ["otel", "traces", trace_id] | ["otel", "traces", trace_id, ""] => {
            Some(ResourceUri::OtelTrace(trace_id.to_string()))
        }
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_session() {
        assert_eq!(
            parse("whats-in-my-cc://sessions/sess-A"),
            Some(ResourceUri::Session("sess-A".into()))
        );
    }

    #[test]
    fn parses_findings() {
        assert_eq!(
            parse("whats-in-my-cc://sessions/sess-A/findings"),
            Some(ResourceUri::SessionFindings("sess-A".into()))
        );
    }

    #[test]
    fn parses_finding() {
        assert_eq!(
            parse("whats-in-my-cc://findings/find_001"),
            Some(ResourceUri::Finding("find_001".into()))
        );
    }

    #[test]
    fn parses_file_lineage() {
        assert_eq!(
            parse("whats-in-my-cc://file-lineage/sess-A"),
            Some(ResourceUri::FileLineage("sess-A".into()))
        );
    }

    #[test]
    fn parses_otel_trace() {
        assert_eq!(
            parse("whats-in-my-cc://otel/traces/abc123"),
            Some(ResourceUri::OtelTrace("abc123".into()))
        );
    }

    #[test]
    fn unknown_returns_none() {
        assert_eq!(parse("whats-in-my-cc://unknown/xyz"), None);
        assert_eq!(parse("https://example.com/foo"), None);
        assert_eq!(parse("whats-in-my-cc://sessions/"), None);
    }
}
