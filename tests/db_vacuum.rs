//! growth-2026-07-18 — `wimcc vacuum`: 기존 DB의 1회성 압축.
//!
//! auto_vacuum=INCREMENTAL은 신규 생성 DB에만 적용된다(기존 파일은 header가
//! 우선, 전환에는 전체 VACUUM 필요). 도그푸딩 DB(1.2GB)처럼 NONE으로 만들어진
//! 파일은 sweep의 incremental_vacuum이 no-op라, 수동 변환+회수 경로가 필요하다.

use sqlx::sqlite::{SqliteConnectOptions, SqlitePoolOptions};
use std::str::FromStr;

#[tokio::test]
async fn vacuum_converts_legacy_db_and_reclaims_disk() {
    let dir = tempfile::tempdir().unwrap();
    let path = dir.path().join("legacy.sqlite");
    let url = format!("sqlite://{}?mode=rwc", path.display());

    // auto_vacuum 옵션 없이 생성 — 2026-07-18 이전 배포본이 만든 파일 재현.
    {
        let opts = SqliteConnectOptions::from_str(&url)
            .unwrap()
            .create_if_missing(true);
        let pool = SqlitePoolOptions::new()
            .max_connections(1)
            .connect_with(opts)
            .await
            .unwrap();
        wimcc::db::migrate(&pool).await.unwrap();
        sqlx::query(
            "INSERT INTO ingest_run (run_id, started_at, status) VALUES ('r1', datetime('now'), 'ok')",
        )
        .execute(&pool)
        .await
        .unwrap();
        let blob = "x".repeat(20_000);
        for i in 0..100 {
            sqlx::query(
                "INSERT INTO raw_event (raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, source_byte_offset, payload_sha256, payload, captured_at)
                 VALUES (?, 'r1', 'claude_transcript', 'big.jsonl', ?, 0, ?, ?, datetime('now'))",
            )
            .bind(format!("raw_{i}"))
            .bind(i as i64)
            .bind(format!("sha_{i}"))
            .bind(&blob)
            .execute(&pool)
            .await
            .unwrap();
        }
        // 공간을 만든 뒤 지워 free page를 남긴다.
        sqlx::query("DELETE FROM raw_event")
            .execute(&pool)
            .await
            .unwrap();
        sqlx::query("PRAGMA wal_checkpoint(TRUNCATE)")
            .execute(&pool)
            .await
            .unwrap();
        pool.close().await;
    }

    {
        let pool = wimcc::db::connect(&url).await.unwrap();
        let av: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
            .fetch_one(&pool)
            .await
            .unwrap();
        assert_eq!(
            av, 0,
            "legacy file keeps auto_vacuum=NONE until a full VACUUM"
        );
        // vacuum_cmd 실사용 조건: serve 정지 = 다른 연결 없음.
        pool.close().await;
    }

    let before_file = std::fs::metadata(&path).unwrap().len();
    let (before, after) = wimcc::db::vacuum_db(&url).await.unwrap();
    assert!(
        before > after,
        "page math must show the shrink: {before} -> {after}"
    );

    let pool = wimcc::db::connect(&url).await.unwrap();
    let av: i64 = sqlx::query_scalar("PRAGMA auto_vacuum")
        .fetch_one(&pool)
        .await
        .unwrap();
    assert_eq!(av, 2, "vacuum converts the header to INCREMENTAL");

    let after_file = std::fs::metadata(&path).unwrap().len();
    assert!(
        after_file < before_file,
        "file on disk must shrink: {before_file} -> {after_file}"
    );
}
