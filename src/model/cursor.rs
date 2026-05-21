//! Slice-9 — `Cursor` type for windowed event paging.
//!
//! Wire format: `<observed_at_rfc3339>|<event_id>`. Same ordering key the
//! slice-8 SSE backfill uses (DEV-S8-10): `(observed_at, event_id)` so that
//! cross-source event_ids (ULID vs `metric:...`) sort by time first.
//!
//! Round-trip is canonicalising: parse normalises observed_at to UTC, format
//! emits `+00:00`. Client-supplied `Z` is accepted because
//! `chrono::DateTime::parse_from_rfc3339` accepts both.
use chrono::{DateTime, Utc};
use std::str::FromStr;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub observed_at: DateTime<Utc>,
    pub event_id: String,
}

#[derive(Debug, thiserror::Error)]
pub enum CursorParseError {
    #[error("cursor missing '|' separator")]
    NoSeparator,
    #[error("invalid observed_at: {0}")]
    InvalidObservedAt(#[from] chrono::ParseError),
    #[error("invalid event_id: {0}")]
    InvalidEventId(String),
}

impl FromStr for Cursor {
    type Err = CursorParseError;
    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let (ts, eid) = s.split_once('|').ok_or(CursorParseError::NoSeparator)?;
        let observed_at = DateTime::parse_from_rfc3339(ts)?.with_timezone(&Utc);
        if eid.is_empty() {
            return Err(CursorParseError::InvalidEventId("empty".into()));
        }
        if eid.len() > 200 {
            return Err(CursorParseError::InvalidEventId("too long".into()));
        }
        if eid.chars().any(|c| (c as u32) < 0x20 || (c as u32) == 0x7f) {
            return Err(CursorParseError::InvalidEventId(
                "control characters".into(),
            ));
        }
        Ok(Cursor {
            observed_at,
            event_id: eid.to_string(),
        })
    }
}

impl std::fmt::Display for Cursor {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}|{}", self.observed_at.to_rfc3339(), self.event_id)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parse_round_trip() {
        let c: Cursor = "2026-05-21T11:42:33.012Z|01J123ABC".parse().unwrap();
        assert_eq!(c.event_id, "01J123ABC");
        let s = c.to_string();
        let c2: Cursor = s.parse().unwrap();
        assert_eq!(c, c2);
    }

    #[test]
    fn accepts_metric_format_event_id() {
        let c: Cursor = "2026-05-21T11:42:33.012Z|metric:foo:bar:1234:5"
            .parse()
            .unwrap();
        assert_eq!(c.event_id, "metric:foo:bar:1234:5");
    }

    #[test]
    fn parse_missing_separator() {
        assert!("no-separator".parse::<Cursor>().is_err());
    }

    #[test]
    fn parse_invalid_timestamp() {
        assert!("not-a-time|01J123".parse::<Cursor>().is_err());
    }

    #[test]
    fn parse_empty_event_id() {
        assert!("2026-05-21T11:42:33.012Z|".parse::<Cursor>().is_err());
    }

    #[test]
    fn parse_control_chars_rejected() {
        assert!("2026-05-21T11:42:33.012Z|\x01abc".parse::<Cursor>().is_err());
    }

    #[test]
    fn parse_oversize_rejected() {
        let huge = format!("2026-05-21T11:42:33.012Z|{}", "a".repeat(201));
        assert!(huge.parse::<Cursor>().is_err());
    }
}
