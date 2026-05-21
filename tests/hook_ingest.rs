use axum_test::TestServer;
use serde_json::Value;
use sqlx::sqlite::SqlitePoolOptions;
use witmcc::db::migrate;

async fn setup() -> TestServer {
    let pool = SqlitePoolOptions::new()
        .max_connections(2)
        .connect("sqlite::memory:")
        .await
        .unwrap();
    migrate(&pool).await.unwrap();
    let app = witmcc::api::router(witmcc::api::AppState::new_for_tests(pool));
    TestServer::new(app).unwrap()
}

fn load(path: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(path).unwrap()).unwrap()
}

#[tokio::test]
async fn batch_three_ingests_all_and_graph_has_three_hook_nodes() {
    let s = setup().await;
    let body = load("tests/fixtures/hook/batch_three.json");
    let resp = s.post("/hooks/v1/events").json(&body).await;
    resp.assert_status_ok();
    let v: Value = resp.json();
    assert_eq!(v["data"]["accepted_events"], 3);
    assert_eq!(v["data"]["rejected_events"], 0);
    assert_eq!(v["data"]["sessions_touched"][0], "sess_fix_B");

    let graph: Value = s.get("/v1/sessions/sess_fix_B/graph").await.json();
    let hook_count = graph["data"]["nodes"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|n| n["node_kind"] == "hook_event")
        .count();
    assert_eq!(hook_count, 3);
}

#[tokio::test]
async fn duplicate_post_increments_duplicate_events_and_keeps_one_row() {
    let s = setup().await;
    let body = load("tests/fixtures/hook/pre_tool_use.json");

    let r1: Value = s.post("/hooks/v1/events").json(&body).await.json();
    assert_eq!(r1["data"]["accepted_events"], 1);
    assert_eq!(r1["data"]["duplicate_events"], 0);

    let r2: Value = s.post("/hooks/v1/events").json(&body).await.json();
    assert_eq!(r2["data"]["accepted_events"], 0);
    assert_eq!(r2["data"]["duplicate_events"], 1);
    // Self-heal: session still marked touched on full dup.
    assert_eq!(r2["data"]["sessions_touched"][0], "sess_fix_A");

    // Slice-9 — session_detail no longer ships events. Use the windowed
    // /events endpoint instead.
    let events: Value = s.get("/v1/sessions/sess_fix_A/events").await.json();
    let cnt = events["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["kind"] == "hook_event")
        .count();
    assert_eq!(cnt, 1);
}

#[tokio::test]
async fn unknown_hook_event_name_accepts_with_unknown_subkind() {
    let s = setup().await;
    let body = load("tests/fixtures/hook/unknown_event.json");
    let r: Value = s.post("/hooks/v1/events").json(&body).await.json();
    assert_eq!(r["data"]["accepted_events"], 1);

    let events: Value = s.get("/v1/sessions/sess_fix_A/events").await.json();
    let unknown = events["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .any(|e| e["kind"] == "hook_event" && e["subkind"] == "unknown");
    assert!(unknown);
}

#[tokio::test]
async fn raw_endpoint_returns_original_hook_json() {
    let s = setup().await;
    let body = load("tests/fixtures/hook/notification.json");
    s.post("/hooks/v1/events").json(&body).await;

    let events: Value = s.get("/v1/sessions/sess_fix_A/events").await.json();
    let event_id = events["data"]["events"]
        .as_array()
        .unwrap()
        .iter()
        .find(|e| e["kind"] == "hook_event" && e["subkind"] == "notification")
        .unwrap()["event_id"]
        .as_str()
        .unwrap()
        .to_string();

    let raw: Value = s.get(&format!("/v1/events/{event_id}/raw")).await.json();
    assert_eq!(raw["data"]["source"]["kind"], "hook");
    assert_eq!(raw["data"]["record"]["hook_event_name"], "Notification");
    assert_eq!(
        raw["data"]["record"]["message"],
        "Claude wants to run a Bash command — approve?"
    );
}

#[tokio::test]
async fn session_appears_in_sessions_list_after_hook_post() {
    let s = setup().await;
    let body = load("tests/fixtures/hook/pre_tool_use.json");
    s.post("/hooks/v1/events").json(&body).await;

    let list: Value = s.get("/v1/sessions").await.json();
    let found = list["data"]
        .as_array()
        .unwrap()
        .iter()
        .any(|sess| sess["session_id"] == "sess_fix_A");
    assert!(found);
}

#[tokio::test]
async fn rejected_event_in_batch_does_not_block_valid_events() {
    let s = setup().await;
    let body = serde_json::json!([
        {"hook_event_name": "PreToolUse"},
        {"session_id": "sess_fix_X", "hook_event_name": "Stop"},
    ]);
    let r: Value = s.post("/hooks/v1/events").json(&body).await.json();
    assert_eq!(r["data"]["accepted_events"], 1);
    assert_eq!(r["data"]["rejected_events"], 1);
    assert_eq!(r["data"]["sessions_touched"][0], "sess_fix_X");
}
