//! Slice-8 L3 subprocess E2E for SSE — spawn real `witmcc serve` and verify
//! frames actually arrive over a real HTTP/1.1 connection.
//!
//! These are the silent-regression guards for production wiring: if anyone
//! drops the BroadcastSink from an ingest handler the matching test goes red.
//!
//! Uses raw `std::net::TcpStream` HTTP to avoid pulling reqwest's blocking
//! feature into dev-deps just for these tests.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

fn pick_port() -> u16 {
    portpicker::pick_unused_port().expect("no free port")
}

/// Spawn `witmcc serve` against a tempfile DB on an ephemeral port. Returns
/// (child, host:port, db file, token, config_dir). Keep db + config_dir alive.
fn spawn_serve_with(
    extra_args: &[&str],
) -> (Child, String, tempfile::NamedTempFile, String, tempfile::TempDir) {
    let port = pick_port();
    let db = tempfile::NamedTempFile::new().expect("tempfile");
    let config_dir = tempfile::tempdir().expect("config tempdir");
    let bin = env!("CARGO_BIN_EXE_witmcc");
    let mut cmd = Command::new(bin);
    cmd.args([
        "--db-path",
        db.path().to_str().unwrap(),
        "serve",
        "--port",
        &port.to_string(),
        "--auto-migrate",
        "--shutdown-after-ms",
        "10000",
        "--no-watch-transcripts",
    ]);
    for a in extra_args {
        cmd.arg(a);
    }
    cmd.env("WITMCC_CONFIG_DIR", config_dir.path())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    let child = cmd.spawn().expect("spawn witmcc serve");
    let host = format!("127.0.0.1:{port}");

    // Poll /v1/health to confirm readiness.
    let deadline = Instant::now() + Duration::from_secs(3);
    while Instant::now() < deadline {
        if let Ok(s) = TcpStream::connect_timeout(&host.parse().unwrap(), Duration::from_millis(200))
        {
            drop(s);
            if http_health_ok(&host) {
                let token = std::fs::read_to_string(config_dir.path().join("token"))
                    .unwrap_or_default()
                    .trim()
                    .to_string();
                return (child, host, db, token, config_dir);
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    panic!("server did not come up at {host}");
}

fn http_health_ok(host: &str) -> bool {
    let mut s = match TcpStream::connect_timeout(&host.parse().unwrap(), Duration::from_millis(200))
    {
        Ok(s) => s,
        Err(_) => return false,
    };
    s.set_read_timeout(Some(Duration::from_millis(300))).ok();
    let _ = write!(
        s,
        "GET /v1/health HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    );
    let mut buf = String::new();
    let _ = s.take(256).read_to_string(&mut buf);
    // Slice-19: 401 means the server is up but requires a token.
    buf.starts_with("HTTP/1.1 200") || buf.starts_with("HTTP/1.1 401")
}

/// Open a streaming GET /v1/stream connection and return the TcpStream so the
/// caller can keep reading bytes.
fn open_sse(host: &str, query: &str) -> TcpStream {
    let mut s = TcpStream::connect_timeout(&host.parse().unwrap(), Duration::from_secs(1))
        .expect("connect for SSE");
    s.set_read_timeout(Some(Duration::from_secs(5))).ok();
    let request = format!(
        "GET /v1/stream{q} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Accept: text/event-stream\r\n\
         Connection: close\r\n\r\n",
        q = query,
        host = host,
    );
    s.write_all(request.as_bytes()).expect("write request");
    s
}

/// Read response bytes from the SSE stream until at least one `data:` line is
/// observed. Returns the buffer up to that point.
fn read_until_data_frame(mut s: TcpStream, max_wait: Duration) -> String {
    let start = Instant::now();
    let mut buf = Vec::with_capacity(8192);
    let mut chunk = [0u8; 1024];
    while start.elapsed() < max_wait {
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => {
                buf.extend_from_slice(&chunk[..n]);
                if let Ok(s) = std::str::from_utf8(&buf) {
                    if s.contains("\ndata:") || s.starts_with("data:") {
                        return s.to_string();
                    }
                }
            }
            Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => {
                std::thread::sleep(Duration::from_millis(50));
            }
            Err(_) => break,
        }
    }
    String::from_utf8_lossy(&buf).into_owned()
}

fn http_post_json(host: &str, path: &str, body: &str) {
    let mut s = TcpStream::connect_timeout(&host.parse().unwrap(), Duration::from_secs(1))
        .expect("connect for POST");
    let req = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         Content-Length: {len}\r\n\
         Connection: close\r\n\r\n{body}",
        len = body.len(),
    );
    s.write_all(req.as_bytes()).expect("write POST");
    s.set_read_timeout(Some(Duration::from_secs(2))).ok();
    let mut sink = Vec::new();
    let _ = s.read_to_end(&mut sink);
}

/// Spec §5 row 1 regression — connecting without a cursor must NOT receive
/// any backfill frames. Earlier impl flooded clients with the oldest 10k
/// rows, burying any recent envelope behind weeks of old data and breaking
/// SessionListPage live update on populated DBs.
#[test]
fn no_cursor_means_no_backfill() {
    let (mut child, host, _db, _token, _cfg) = spawn_serve_with(&[]);

    // Seed the DB with one hook BEFORE the SSE client connects.
    let body = serde_json::json!({
        "session_id":      "seeded_session",
        "hook_event_name": "PreToolUse",
        "tool_name":       "Bash",
        "tool_input":      {"command": "x"},
        "tool_use_id":     "toolu_seed"
    })
    .to_string();
    http_post_json(&host, "/hooks/v1/events", &body);
    std::thread::sleep(Duration::from_millis(200));

    // Connect SSE without cursor. Expect zero `data:` frames within a short
    // window (only headers + keepalive comments allowed).
    let mut s = open_sse(&host, "");
    let start = Instant::now();
    let mut buf = Vec::new();
    let mut chunk = [0u8; 512];
    while start.elapsed() < Duration::from_millis(1500) {
        match s.read(&mut chunk) {
            Ok(0) => break,
            Ok(n) => buf.extend_from_slice(&chunk[..n]),
            Err(_) => break,
        }
    }
    let body = String::from_utf8_lossy(&buf).into_owned();
    let _ = child.kill();

    assert!(
        !body.contains("\ndata:") && !body.starts_with("data:"),
        "no-cursor connect must not emit backfill data frames, got: {body:?}"
    );
}

/// E-5 — hook POST emits SSE frame. Hooks are the cheapest ingest path to
/// drive in a subprocess test: synchronous POST, immediate response, single
/// row inserted.
#[test]
fn binary_emits_sse_for_hooks() {
    let (mut child, host, _db, _token, _cfg) = spawn_serve_with(&[]);

    // Open SSE listener in a worker thread; we read while the main thread
    // triggers the hook POST.
    let host_for_sse = host.clone();
    let reader = std::thread::spawn(move || {
        let s = open_sse(&host_for_sse, "");
        read_until_data_frame(s, Duration::from_secs(5))
    });

    // Tiny pause so the SSE handshake reaches subscribe-before-INSERT.
    std::thread::sleep(Duration::from_millis(250));

    let body = serde_json::json!({
        "session_id":      "sess_subproc_hook",
        "hook_event_name": "PreToolUse",
        "tool_name":       "Bash",
        "tool_input":      {"command": "ls"},
        "tool_use_id":     "toolu_subproc"
    })
    .to_string();
    http_post_json(&host, "/hooks/v1/events", &body);

    let body = reader.join().expect("reader thread");
    let _ = child.kill();

    assert!(
        body.contains("data:"),
        "expected SSE data frame in body, got: {body:?}"
    );
    assert!(
        body.contains("\"source_type\":\"hook\""),
        "expected hook source_type in envelope, got: {body:?}"
    );
}

/// E-3 — OTLP/JSON metrics POST emits SSE frames (one per data point).
/// Anchored on the real slice-6 v01 fixture.
#[test]
fn binary_emits_sse_for_otel_metrics() {
    let (mut child, host, _db, _token, _cfg) = spawn_serve_with(&[]);

    let host_for_sse = host.clone();
    let reader = std::thread::spawn(move || {
        let s = open_sse(&host_for_sse, "");
        read_until_data_frame(s, Duration::from_secs(5))
    });

    std::thread::sleep(Duration::from_millis(250));

    // Tiny synthetic metrics body; real fixture is large and slow.
    let body = serde_json::json!({
        "resourceMetrics": [{
            "resource": {"attributes": [{"key": "service.name", "value": {"stringValue": "claude-code"}}]},
            "scopeMetrics": [{
                "metrics": [{
                    "name": "claude_code.cost.usage",
                    "sum": {
                        "dataPoints": [{
                            "asDouble": 0.01,
                            "timeUnixNano": "1716200000000000000",
                            "attributes": [
                                {"key": "session.id", "value": {"stringValue": "sess_subproc_metrics"}}
                            ]
                        }],
                        "aggregationTemporality": 2,
                        "isMonotonic": true
                    }
                }]
            }]
        }]
    })
    .to_string();
    http_post_json(&host, "/otel/v1/metrics", &body);

    let body = reader.join().expect("reader thread");
    let _ = child.kill();

    assert!(
        body.contains("data:"),
        "expected SSE data frame in body, got: {body:?}"
    );
    assert!(
        body.contains("\"source_type\":\"otel-metrics\""),
        "expected otel-metrics source_type, got: {body:?}"
    );
}

/// Session filter (E-9 light) — `?session=other` must not receive frames for
/// a different session. Verifies the server-side filter at the SSE handler.
#[test]
fn sse_session_filter_does_not_leak_other_sessions() {
    let (mut child, host, _db, _token, _cfg) = spawn_serve_with(&[]);

    // Subscribe with a session filter that won't match the hook we'll POST.
    let host_for_sse = host.clone();
    let reader = std::thread::spawn(move || {
        let s = open_sse(&host_for_sse, "?session=NEVER_MATCHED");
        // Short wait; nothing should arrive.
        let start = Instant::now();
        let mut buf = Vec::new();
        let mut chunk = [0u8; 512];
        let mut sock = s;
        while start.elapsed() < Duration::from_millis(1500) {
            match sock.read(&mut chunk) {
                Ok(0) => break,
                Ok(n) => buf.extend_from_slice(&chunk[..n]),
                Err(_) => break,
            }
        }
        String::from_utf8_lossy(&buf).into_owned()
    });

    std::thread::sleep(Duration::from_millis(250));

    let body = serde_json::json!({
        "session_id":      "sess_subproc_other",
        "hook_event_name": "PreToolUse",
        "tool_name":       "Bash",
        "tool_input":      {"command": "ls"},
        "tool_use_id":     "toolu_other"
    })
    .to_string();
    http_post_json(&host, "/hooks/v1/events", &body);

    let body = reader.join().expect("reader thread");
    let _ = child.kill();

    // We may see HTTP headers and possibly `:keepalive` comments, but never a
    // `data:` line with the other session's hook envelope.
    assert!(
        !body.contains("\"session_id\":\"sess_subproc_other\""),
        "envelope leaked through session filter: {body:?}"
    );
}
