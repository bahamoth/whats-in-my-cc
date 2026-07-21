/// S1.1: tool_call payload must carry tool_name for node labels.
///
/// Real-data anchoring: assertions use the real minimal_session.jsonl fixture,
/// which contains a tool_use block with name:"Bash" (line 2 of the fixture).
/// The fixture is frozen at tests/fixtures/transcripts/minimal_session.jsonl.
use futures::StreamExt;
use wimcc::ingest::mapping::map_record;
use wimcc::ingest::transcript::stream_file;
use wimcc::model::observed::EventKind;

async fn mapped_events() -> Vec<wimcc::model::observed::ObservedEvent> {
    let p = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    let mut stream = Box::pin(stream_file(p).await.unwrap());
    let mut events = vec![];
    let mut gen = wimcc::ids::MonotonicUlidGen::new();
    while let Some(item) = stream.next().await {
        let (meta, rec) = item.unwrap();
        let raw_id = gen.generate();
        events.extend(map_record(&meta, &rec, &raw_id, &mut gen).unwrap());
    }
    events
}

/// Fixture: assistant line 2 contains a tool_use block with name:"Bash".
/// After enrichment, payload["tool_name"] == "Bash".
#[tokio::test]
async fn tool_call_payload_carries_tool_name() {
    let events = mapped_events().await;
    let tc = events
        .iter()
        .find(|e| e.kind == EventKind::ToolCall)
        .expect("no ToolCall event found");
    assert_eq!(
        tc.payload["tool_name"].as_str(),
        Some("Bash"),
        "tool_call payload must include tool_name; got payload: {}",
        tc.payload
    );
}
