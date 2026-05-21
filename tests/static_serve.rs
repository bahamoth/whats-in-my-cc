use witmcc::api::router;

async fn setup_empty_pool() -> (sqlx::SqlitePool, tempfile::TempDir) {
    let tmp = tempfile::tempdir().unwrap();
    let path = tmp.path().join("test.sqlite");
    let url = format!("sqlite://{}?mode=rwc", path.display());
    let pool = sqlx::sqlite::SqlitePoolOptions::new()
        .connect(&url)
        .await
        .unwrap();
    sqlx::migrate!("./migrations").run(&pool).await.unwrap();
    (pool, tmp)
}

#[tokio::test]
async fn serves_index_html_at_root() {
    let (pool, _tmp) = setup_empty_pool().await;
    let server = axum_test::TestServer::new(router(witmcc::api::AppState::new_for_tests(pool))).unwrap();
    let resp = server.get("/").await;
    resp.assert_status_ok();
    let ct = resp
        .headers()
        .get("content-type")
        .unwrap()
        .to_str()
        .unwrap();
    assert!(ct.starts_with("text/html"));
    let body = resp.text();
    assert!(body.contains("<div id=\"root\""));
}

#[tokio::test]
async fn serves_spa_fallback_for_unknown_route() {
    let (pool, _tmp) = setup_empty_pool().await;
    let server = axum_test::TestServer::new(router(witmcc::api::AppState::new_for_tests(pool))).unwrap();
    let resp = server.get("/sessions/anything").await;
    resp.assert_status_ok();
    assert!(resp.text().contains("<div id=\"root\""));
}

#[tokio::test]
async fn v1_routes_are_not_swallowed_by_fallback() {
    let (pool, _tmp) = setup_empty_pool().await;
    let server = axum_test::TestServer::new(router(witmcc::api::AppState::new_for_tests(pool))).unwrap();
    let resp = server.get("/v1/health").await;
    resp.assert_status_ok();
    let body: serde_json::Value = resp.json();
    assert_eq!(body["status"], "ok");
}
