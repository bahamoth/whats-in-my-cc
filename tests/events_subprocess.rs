//! Slice-9 L3 subprocess E2E for the windowed events endpoint. Seeds a
//! 1200-event session into a file-backed DB, then spawns a real `wimcc
//! serve` against that DB and pages backwards through `/v1/sessions/:id/events`
//! using only raw TCP HTTP/1.1.
//!
//! Pass criteria:
//!   - Each page returns up to `limit` events, ordered ASC by
//!     `(observed_at, event_id)`.
//!   - Following `prev_cursor` walks strictly backwards without overlap or
//!     gap, until the union of all pages reconstructs the full seed set.
//!   - After paging back, a `?after=<initial_newest>&limit=200` request
//!     returns 0 rows (we've already consumed the live tip via the initial
//!     page-1 fetch).

use chrono::{DateTime, TimeZone, Utc};
use serde_json::Value;
use std::collections::BTreeSet;
use std::io::{Read, Write};
use std::net::TcpStream;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

use wimcc::db::{connect, migrate, repo_observed, repo_raw, repo_runs};
use wimcc::model::observed::{Actor, EventKind, ObservedEvent};

const SESS: &str = "sess-paged";
const SEED_N: usize = 1200;
const PAGE_LIMIT: usize = 200;

fn pick_port() -> u16 {
    portpicker::pick_unused_port().expect("no free port")
}

async fn seed_db(path: &Path) {
    let url = format!("sqlite://{}", path.display());
    let pool = connect(&url).await.unwrap();
    migrate(&pool).await.unwrap();
    let run_id = repo_runs::start(&pool).await.unwrap();
    let base: DateTime<Utc> = Utc.with_ymd_and_hms(2026, 5, 21, 0, 0, 0).unwrap();
    for i in 0..SEED_N {
        let event_id = format!("01J{i:023}");
        let raw_id = format!("raw_{i:06}");
        repo_raw::insert_dedup(
            &pool,
            &repo_raw::NewRaw {
                raw_event_id: raw_id.clone(),
                ingest_run_id: run_id.clone(),
                source_type: "test".into(),
                source_uri: format!("test://{i}"),
                source_line_no: i as i64,
                source_byte_offset: 0,
                payload_sha256: format!("sha_{i:06}"),
                payload: b"{}".to_vec(),
                parse_error: None,
                captured_at: chrono::Utc::now(),
                redaction_state: "not_applicable".into(),
                redaction_manifest: None,
            },
        )
        .await
        .unwrap();
        let ev = ObservedEvent {
            event_id,
            raw_event_id: raw_id,
            schema_version: "0.5.0".into(),
            session_id: SESS.into(),
            event_uuid: Some(format!("uuid-{i:06}")),
            observed_at: base + chrono::Duration::seconds(i as i64),
            actor: Actor::User,
            kind: EventKind::UserMessage,
            parser_version: "test".into(),
            ..Default::default()
        };
        repo_observed::insert(&pool, &ev).await.unwrap();
    }
    pool.close().await;
}

/// Spawn a wimcc server. Returns (child, host, token, config_dir).
/// `config_dir` must be kept alive while the server runs.
fn spawn_serve(db_path: &Path) -> (Child, String, String, tempfile::TempDir) {
    let port = pick_port();
    let bin = env!("CARGO_BIN_EXE_wimcc");
    // Slice-19: isolated config dir so tests don't touch ~/.config/wimcc.
    let config_dir = tempfile::tempdir().expect("config tempdir");
    let mut child = Command::new(bin)
        .args([
            "--db-path",
            db_path.to_str().unwrap(),
            "serve",
            "--port",
            &port.to_string(),
            "--auto-migrate",
            "--shutdown-after-ms",
            "15000",
            "--no-watch-transcripts",
        ])
        .env("WIMCC_CONFIG_DIR", config_dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("spawn wimcc serve");
    let host = format!("127.0.0.1:{port}");
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        let resp = http_get(&host, "/v1/health", None);
        if resp.starts_with("HTTP/1.1 200") || resp.starts_with("HTTP/1.1 401") {
            // Read the token that the server wrote.
            let token = std::fs::read_to_string(config_dir.path().join("token"))
                .unwrap_or_default()
                .trim()
                .to_string();
            return (child, host, token, config_dir);
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    let _ = child.kill();
    let _ = child.wait();
    panic!("server did not come up at {host}");
}

fn http_get(host: &str, path: &str, token: Option<&str>) -> String {
    let addr: std::net::SocketAddr = host.parse().expect("host:port");
    let mut s = match TcpStream::connect_timeout(&addr, Duration::from_secs(1)) {
        Ok(s) => s,
        Err(_) => return String::new(),
    };
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let auth_header = token
        .map(|t| format!("Authorization: Bearer {t}\r\n"))
        .unwrap_or_default();
    let req =
        format!("GET {path} HTTP/1.1\r\nHost: {host}\r\n{auth_header}Connection: close\r\n\r\n",);
    s.write_all(req.as_bytes()).expect("write");
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).ok();
    String::from_utf8_lossy(&buf).into_owned()
}

fn json_body(resp: &str) -> Value {
    let body_start = resp.find("\r\n\r\n").map(|i| i + 4).unwrap_or(0);
    let body = &resp[body_start..];
    // chunked transfer encoding — strip hex-size lines if present.
    if resp.to_lowercase().contains("transfer-encoding: chunked") {
        let mut out = String::new();
        let mut lines = body.split("\r\n");
        while let Some(size_line) = lines.next() {
            let size = match usize::from_str_radix(size_line.trim(), 16) {
                Ok(n) => n,
                Err(_) => break,
            };
            if size == 0 {
                break;
            }
            let data_line = lines.next().unwrap_or("");
            out.push_str(&data_line[..size.min(data_line.len())]);
        }
        return serde_json::from_str(&out).unwrap_or(Value::Null);
    }
    serde_json::from_str(body).unwrap_or(Value::Null)
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paginate_backwards_reconstructs_full_session() {
    // 1. Seed.
    let db = tempfile::NamedTempFile::new().expect("tempfile");
    seed_db(db.path()).await;

    // 2. Spawn server.
    let (mut child, host, token, _cfg) = spawn_serve(db.path());
    let tok = Some(token.as_str());

    // 3. Page-1 fetch (no cursors → newest PAGE_LIMIT).
    let resp = http_get(
        &host,
        &format!("/v1/sessions/{SESS}/events?limit={PAGE_LIMIT}"),
        tok,
    );
    let mut data = json_body(&resp);
    let mut collected: BTreeSet<String> = BTreeSet::new();
    let mut prev_first_id: Option<String>;
    {
        let events = data["data"]["events"].as_array().expect("events array");
        assert_eq!(
            events.len(),
            PAGE_LIMIT,
            "page-1 should be full when seed > limit"
        );
        for e in events {
            collected.insert(e["event_id"].as_str().unwrap().to_string());
        }
        prev_first_id = Some(events[0]["event_id"].as_str().unwrap().to_string());
    }
    let initial_newest_cursor = data["data"]["events"].as_array().unwrap().last().map(|e| {
        format!(
            "{}|{}",
            e["observed_at"].as_str().unwrap(),
            e["event_id"].as_str().unwrap()
        )
    });

    // 4. Page backwards using prev_cursor until the response shrinks below
    //    PAGE_LIMIT or we hit the start.
    let mut prev_cursor = data["data"]["prev_cursor"].as_str().map(str::to_string);
    let mut pages = 1;
    while let Some(cur) = prev_cursor.take() {
        let url = format!(
            "/v1/sessions/{SESS}/events?before={}&limit={PAGE_LIMIT}",
            urlencoding::encode(&cur)
        );
        let r = http_get(&host, &url, tok);
        data = json_body(&r);
        let events = data["data"]["events"].as_array().expect("events array");
        if events.is_empty() {
            break;
        }
        let last_id = events.last().unwrap()["event_id"]
            .as_str()
            .unwrap()
            .to_string();
        if let Some(first) = &prev_first_id {
            assert!(
                last_id.as_str() < first.as_str(),
                "page boundary overlap: this page's last_id={last_id} >= prev page's first_id={first}",
            );
        }
        for e in events {
            let id = e["event_id"].as_str().unwrap().to_string();
            assert!(
                collected.insert(id.clone()),
                "duplicate event_id across pages: {id}"
            );
        }
        prev_first_id = Some(events[0]["event_id"].as_str().unwrap().to_string());
        if events.len() < PAGE_LIMIT {
            // last page
            break;
        }
        prev_cursor = data["data"]["prev_cursor"].as_str().map(str::to_string);
        pages += 1;
        assert!(
            pages < 20,
            "runaway paging (test seed only has {SEED_N} events)"
        );
    }

    // 5. Union must equal the seed set.
    assert_eq!(
        collected.len(),
        SEED_N,
        "page union should reconstruct all {SEED_N} events, got {}",
        collected.len()
    );

    // 6. Reaching the live tip via `?after=` returns zero rows.
    if let Some(cur) = initial_newest_cursor {
        let r = http_get(
            &host,
            &format!(
                "/v1/sessions/{SESS}/events?after={}&limit={PAGE_LIMIT}",
                urlencoding::encode(&cur)
            ),
            tok,
        );
        let v = json_body(&r);
        assert_eq!(
            v["data"]["events"].as_array().unwrap().len(),
            0,
            "after-live-tip cursor should return no rows; got: {}",
            v["data"]["events"]
        );
    }

    let _ = child.kill();
    let _ = child.wait();
}

fn http_post_json(host: &str, path: &str, body: &str) -> String {
    let addr: std::net::SocketAddr = host.parse().expect("host:port");
    let mut s = TcpStream::connect_timeout(&addr, Duration::from_secs(1)).expect("connect");
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let req = format!(
        "POST {path} HTTP/1.1\r\nHost: {host}\r\nContent-Type: application/json\r\n\
         Content-Length: {len}\r\nConnection: close\r\n\r\n{body}",
        path = path,
        host = host,
        len = body.len(),
        body = body,
    );
    s.write_all(req.as_bytes()).expect("write");
    let mut buf = Vec::new();
    s.read_to_end(&mut buf).ok();
    String::from_utf8_lossy(&buf).into_owned()
}

/// Slice-9 — live activity + paging coexistence. Verifies that after
/// fetching page-1 (which captures `initial_newest` cursor), a burst of
/// new events landing into the same session is fully recovered by
/// `?after=initial_newest` and that the original page-1 cursor still
/// paginates older history without overlap with the new tail.
///
/// This is the "SSE + paging" integration gap I called out — the slice-9
/// PR description claims the two coexist; this test locks it.
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn paging_remains_consistent_through_live_activity() {
    let db = tempfile::NamedTempFile::new().expect("tempfile");
    seed_db(db.path()).await;
    let (mut child, host, token, _cfg) = spawn_serve(db.path());
    let tok = Some(token.as_str());

    // 1. Page-1 capture
    let r1 = http_get(
        &host,
        &format!("/v1/sessions/{SESS}/events?limit={PAGE_LIMIT}"),
        tok,
    );
    let v1 = json_body(&r1);
    let initial_first_cursor = {
        let e = &v1["data"]["events"].as_array().unwrap()[0];
        format!(
            "{}|{}",
            e["observed_at"].as_str().unwrap(),
            e["event_id"].as_str().unwrap()
        )
    };
    let initial_newest = v1["data"]["events"]
        .as_array()
        .unwrap()
        .last()
        .map(|e| {
            format!(
                "{}|{}",
                e["observed_at"].as_str().unwrap(),
                e["event_id"].as_str().unwrap()
            )
        })
        .expect("initial newest");

    // 2. Burst of live activity — 50 hook POSTs against the same session.
    //    Hook ingest stamps `captured_at = now()`, which is later than any
    //    seed row (base 2026-05-21T00:00:00Z + ≤1199s). Hooks land at the
    //    live tip and only the live tip.
    const BURST: usize = 50;
    for i in 0..BURST {
        let body = format!(
            r#"{{"session_id":"{SESS}","hook_event_name":"PreToolUse","tool_name":"Bash","tool_input":{{"command":"echo {i}"}},"tool_use_id":"tu_burst_{i}"}}"#
        );
        let resp = http_post_json(&host, "/hooks/v1/events", &body);
        assert!(
            resp.starts_with("HTTP/1.1 200"),
            "hook POST {i} did not 200: {}",
            &resp[..80.min(resp.len())]
        );
    }

    // 3. ?after=initial_newest must surface exactly the BURST new rows.
    let r_after = http_get(
        &host,
        &format!(
            "/v1/sessions/{SESS}/events?after={}&limit=500",
            urlencoding::encode(&initial_newest)
        ),
        tok,
    );
    let v_after = json_body(&r_after);
    let after_events = v_after["data"]["events"].as_array().expect("events array");
    assert_eq!(
        after_events.len(),
        BURST,
        "after-cursor should surface exactly {BURST} new hook rows; got {}",
        after_events.len()
    );
    // All BURST events must be kind=hook_event.
    for e in after_events {
        assert_eq!(
            e["kind"].as_str().unwrap(),
            "hook_event",
            "after-window must be hook-only — leak from seed?"
        );
    }

    // 4. The original page-1's `?before=` cursor still paginates older
    //    history (unaffected by the live tail).
    let r_before = http_get(
        &host,
        &format!(
            "/v1/sessions/{SESS}/events?before={}&limit={PAGE_LIMIT}",
            urlencoding::encode(&initial_first_cursor)
        ),
        tok,
    );
    let v_before = json_body(&r_before);
    let before_events = v_before["data"]["events"].as_array().expect("events array");
    assert_eq!(
        before_events.len(),
        PAGE_LIMIT,
        "older window unchanged by live activity"
    );
    let before_last_id = before_events.last().unwrap()["event_id"].as_str().unwrap();
    assert!(
        before_last_id.starts_with("01J"),
        "older window must contain only ULID seed ids, not hook ids; got {before_last_id}"
    );

    // 5. A fresh page-1 fetch now ends at the latest hook row.
    let r_p1_after = http_get(
        &host,
        &format!("/v1/sessions/{SESS}/events?limit={PAGE_LIMIT}"),
        tok,
    );
    let v_p1_after = json_body(&r_p1_after);
    let new_p1 = v_p1_after["data"]["events"].as_array().unwrap();
    let new_p1_last_kind = new_p1.last().unwrap()["kind"].as_str().unwrap();
    assert_eq!(
        new_p1_last_kind, "hook_event",
        "after burst, page-1's newest row should be a hook_event"
    );
    // next_cursor stays null (we're at the live tip).
    assert!(v_p1_after["data"]["next_cursor"].is_null());

    let _ = child.kill();
    let _ = child.wait();
}
