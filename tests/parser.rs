use futures::StreamExt;
use std::path::Path;
use wimcc::ingest::transcript::{stream_file, ParsedRecord};

#[tokio::test]
async fn parses_five_record_types_in_minimal_fixture() {
    let p = Path::new("tests/fixtures/transcripts/minimal_session.jsonl");
    let mut stream = Box::pin(stream_file(p).await.unwrap());
    let mut kinds = vec![];
    while let Some(item) = stream.next().await {
        let (_meta, rec) = item.unwrap();
        kinds.push(match rec {
            ParsedRecord::User(_) => "user",
            ParsedRecord::Assistant(_) => "assistant",
            ParsedRecord::Attachment(_) => "attachment",
            ParsedRecord::SystemMsg(_) => "system",
            ParsedRecord::PermissionMode(_) => "permission-mode",
            ParsedRecord::LastPrompt(_) => "last-prompt",
            ParsedRecord::FileHistorySnapshot(_) => "file-history-snapshot",
            ParsedRecord::Unknown(_) => "unknown",
        });
    }
    assert_eq!(
        kinds,
        vec!["user", "assistant", "user", "assistant", "permission-mode"]
    );
}
