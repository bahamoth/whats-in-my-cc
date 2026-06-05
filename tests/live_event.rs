use strum::IntoEnumIterator;
use wimcc::live::LiveEvent;
use wimcc::model::observed::EventKind;

#[test]
fn liveevent_envelope_v1_roundtrip() {
    let env = LiveEvent {
        schema_version: LiveEvent::SCHEMA_VERSION.to_string(),
        session_id: "s1".into(),
        event_id: "01HZZZ000000000000000000A".into(),
        kind: EventKind::UserMessage,
        source_type: "transcript".into(),
        observed_at: "2026-05-21T10:00:00Z".into(),
    };
    let json = serde_json::to_string(&env).unwrap();
    let back: LiveEvent = serde_json::from_str(&json).unwrap();
    assert_eq!(back.schema_version, "1");
    assert_eq!(back.event_id, env.event_id);
    assert_eq!(back.kind, EventKind::UserMessage);
    assert!(json.contains("\"schema_version\":\"1\""));
    assert!(json.contains("\"kind\":\"user_message\""));
}

#[test]
fn liveevent_from_each_event_kind_compiles_and_serializes() {
    for k in EventKind::iter() {
        let env = LiveEvent {
            schema_version: LiveEvent::SCHEMA_VERSION.to_string(),
            session_id: "s".into(),
            event_id: "01HZZZ000000000000000000A".into(),
            kind: k,
            source_type: "transcript".into(),
            observed_at: "2026-05-21T10:00:00Z".into(),
        };
        let s = serde_json::to_string(&env).unwrap();
        assert!(s.contains("\"kind\""), "missing kind for {:?}", k);
    }
}
