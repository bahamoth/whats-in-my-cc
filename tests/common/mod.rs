use chrono::{TimeZone, Utc};
use wimcc::model::observed::{EventKind, ObservedEvent};

/// Minimal ObservedEvent for unit tests. All optional fields are None / default;
/// callers can override with struct update syntax. `observed_at` is derived
/// deterministically from `event_id` (no wall-clock) so graph output is stable
/// across runs and distinct event_ids get distinct, reproducible timestamps.
pub fn base_event(kind: EventKind, event_id: &str) -> ObservedEvent {
    let offset = event_id.bytes().fold(0i64, |a, b| a + b as i64);
    ObservedEvent {
        event_id: event_id.into(),
        raw_event_id: format!("raw_{event_id}"),
        schema_version: "observed_event.v1".into(),
        session_id: "sess_t".into(),
        event_uuid: Some(format!("uuid_{event_id}")),
        observed_at: Utc.timestamp_opt(1_700_000_000 + offset, 0).unwrap(),
        kind,
        parser_version: "test".into(),
        ..Default::default()
    }
}
