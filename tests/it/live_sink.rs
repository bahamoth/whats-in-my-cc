use std::sync::Arc;
use tokio::sync::broadcast;
use wimcc::live::{BroadcastSink, CapturingSink, LiveEvent, LiveSink, NoopSink};
use wimcc::model::observed::EventKind;

fn sample(id: &str) -> LiveEvent {
    LiveEvent {
        schema_version: LiveEvent::SCHEMA_VERSION.into(),
        session_id: "s".into(),
        event_id: id.into(),
        kind: EventKind::UserMessage,
        source_type: "transcript".into(),
        observed_at: "2026-05-21T10:00:00Z".into(),
    }
}

#[test]
fn noop_sink_does_not_panic_and_records_nothing() {
    let s = NoopSink;
    s.emit(sample("a"));
    s.emit(sample("b"));
}

#[test]
fn capturing_sink_records_in_order() {
    let s = CapturingSink::new();
    s.emit(sample("a"));
    s.emit(sample("b"));
    let v = s.collected();
    assert_eq!(v.len(), 2);
    assert_eq!(v[0].event_id, "a");
    assert_eq!(v[1].event_id, "b");
}

#[test]
fn livesink_trait_object_usable() {
    let s: &dyn LiveSink = &NoopSink;
    s.emit(sample("a"));
}

#[tokio::test]
async fn broadcast_sink_emits_to_subscriber() {
    let (tx, mut rx) = broadcast::channel::<LiveEvent>(16);
    let sink = BroadcastSink::new(Arc::new(tx));
    sink.emit(sample("a"));
    let got = rx.recv().await.unwrap();
    assert_eq!(got.event_id, "a");
}

#[tokio::test]
async fn broadcast_sink_with_no_subscribers_does_not_panic() {
    let (tx, rx) = broadcast::channel::<LiveEvent>(16);
    drop(rx); // no subscribers
    let sink = BroadcastSink::new(Arc::new(tx));
    sink.emit(sample("a")); // must not panic
}
