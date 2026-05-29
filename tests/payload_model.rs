/// S1.2: assistant_message payload must carry model for role labels.
///
/// Real-data anchoring: assertions use the real minimal_session.jsonl fixture,
/// which contains assistant messages with model:"claude-opus-4-7" (lines 2 and 4).
/// The fixture is frozen at tests/fixtures/transcripts/minimal_session.jsonl.
use futures::StreamExt;
use witmcc::ingest::mapping::map_record;
use witmcc::ingest::transcript::stream_file;
use witmcc::model::observed::EventKind;

async fn mapped_events() -> Vec<witmcc::model::observed::ObservedEvent> {
    let p = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    let mut stream = Box::pin(stream_file(p).await.unwrap());
    let mut events = vec![];
    let mut gen = witmcc::ids::MonotonicUlidGen::new();
    while let Some(item) = stream.next().await {
        let (meta, rec) = item.unwrap();
        let raw_id = gen.generate();
        events.extend(map_record(&meta, &rec, &raw_id, &mut gen).unwrap());
    }
    events
}

/// Fixture: both assistant messages have model:"claude-opus-4-7".
/// After enrichment, payload["model"] == "claude-opus-4-7" for AssistantMessage events.
#[tokio::test]
async fn assistant_message_payload_carries_model() {
    let events = mapped_events().await;
    let text_msgs: Vec<_> = events
        .iter()
        .filter(|e| e.kind == EventKind::AssistantMessage)
        .collect();
    assert!(
        !text_msgs.is_empty(),
        "expected at least one AssistantMessage event"
    );
    for msg in &text_msgs {
        assert_eq!(
            msg.payload["model"].as_str(),
            Some("claude-opus-4-7"),
            "assistant_message payload must include model; got payload: {}",
            msg.payload
        );
    }
}

/// tool_call payload must NOT carry model (only text blocks get it).
#[tokio::test]
async fn tool_call_payload_does_not_carry_model() {
    let events = mapped_events().await;
    let tc = events
        .iter()
        .find(|e| e.kind == EventKind::ToolCall)
        .expect("no ToolCall event found");
    assert!(
        tc.payload.get("model").is_none(),
        "tool_call payload must not include model; got payload: {}",
        tc.payload
    );
}
