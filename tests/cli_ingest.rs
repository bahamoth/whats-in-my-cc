use assert_cmd::Command;
use tempfile::tempdir;

#[test]
fn ingest_minimal_fixture_via_cli() {
    let dir = tempdir().unwrap();
    let db = dir.path().join("witmcc.sqlite");
    Command::cargo_bin("witmcc")
        .unwrap()
        .args(["--db-path", db.to_str().unwrap(), "init-db"])
        .assert()
        .success();
    Command::cargo_bin("witmcc")
        .unwrap()
        .args([
            "--db-path",
            db.to_str().unwrap(),
            "ingest",
            "--path",
            "tests/fixtures/transcripts/minimal_session.jsonl",
        ])
        .assert()
        .success();
    // sanity: file exists and has rows
    let n: i64 = rusqlite_count(&db, "SELECT count(*) FROM observed_event");
    assert!(n >= 6, "got {n}");
    let g: i64 = rusqlite_count(&db, "SELECT count(*) FROM graph_node");
    assert!(g >= 4, "got {g}");
}

fn rusqlite_count(path: &std::path::Path, sql: &str) -> i64 {
    // tiny shim: use sqlite3 CLI to avoid adding rusqlite as dep
    let out = std::process::Command::new("sqlite3")
        .arg(path)
        .arg(sql)
        .output()
        .unwrap();
    String::from_utf8(out.stdout)
        .unwrap()
        .trim()
        .parse()
        .unwrap()
}
