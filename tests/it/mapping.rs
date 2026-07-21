use futures::StreamExt;
use wimcc::ingest::mapping::map_record;
use wimcc::ingest::transcript::stream_file;
use wimcc::model::observed::{Actor, EventKind};

#[tokio::test]
async fn maps_minimal_fixture_to_six_observed_events() {
    let p = std::path::Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    let mut stream = Box::pin(stream_file(p).await.unwrap());
    let mut events = vec![];
    let mut gen = wimcc::ids::MonotonicUlidGen::new();
    while let Some(item) = stream.next().await {
        let (meta, rec) = item.unwrap();
        let raw_id = gen.generate();
        events.extend(map_record(&meta, &rec, &raw_id, &mut gen).unwrap());
    }
    // Expected breakdown for the 5-line fixture:
    //   user(string)          → 1 user_message
    //   assistant(text+tool)  → 1 assistant_message + 1 tool_call
    //   user(tool_result)     → 1 tool_result
    //   assistant(text)       → 1 assistant_message
    //   permission-mode       → 1 session_state
    //                  total  = 6
    assert_eq!(events.len(), 6, "{events:#?}");
    let kinds: Vec<_> = events.iter().map(|e| e.kind).collect();
    assert_eq!(
        kinds,
        vec![
            EventKind::UserMessage,
            EventKind::AssistantMessage,
            EventKind::ToolCall,
            EventKind::ToolResult,
            EventKind::AssistantMessage,
            EventKind::SessionState,
        ]
    );
    // Spot-check correlation keys.
    let tc = events
        .iter()
        .find(|e| e.kind == EventKind::ToolCall)
        .unwrap();
    assert_eq!(tc.tool_use_id.as_deref(), Some("toolu_x"));
    assert_eq!(tc.tool_name.as_deref(), Some("Bash"));
    assert_eq!(tc.actor, Actor::Assistant);
    let tr = events
        .iter()
        .find(|e| e.kind == EventKind::ToolResult)
        .unwrap();
    assert_eq!(tr.tool_use_id.as_deref(), Some("toolu_x"));
    assert_eq!(tr.source_tool_assistant_uuid.as_deref(), Some("a1"));
}
