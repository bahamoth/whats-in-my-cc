pub mod diff_hunk;
pub mod mapping;
pub mod otel;
pub mod otel_logs;
pub mod otel_metrics;
pub mod store;
pub mod subagent_meta;
pub mod transcript;
pub mod usage_facet;
pub mod verification_run;

/// transcripts root 아래에서 ingest 대상 파일을 걷는다: `*.jsonl`(메인 +
/// subagent transcript) + subagent 사이드카 `agent-*.meta.json`. CLI의
/// `ingest --all/--path <dir>`와 serve의 초기 스캔이 공유한다.
pub fn discover_files(root: &std::path::Path) -> Vec<std::path::PathBuf> {
    walkdir::WalkDir::new(root)
        .into_iter()
        .filter_map(|r| r.ok())
        .filter(|e| {
            e.file_type().is_file()
                && (e.path().extension().and_then(|x| x.to_str()) == Some("jsonl")
                    || subagent_meta::sidecar_path_parts(e.path()).is_some())
        })
        .map(|e| e.into_path())
        .collect()
}
