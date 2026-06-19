//! Slice-6 — `wimcc doctor` CLI smoke tests.
//!
//! These tests don't mock the server; they spin up a real wimcc server
//! against a tempfile DB and probe via the doctor subcommand.

use assert_cmd::Command as AssertCmd;
use std::process::{Command, Stdio};
use std::time::Duration;

fn pick_port() -> u16 {
    portpicker::pick_unused_port().expect("no free port")
}

/// Spawn a wimcc server bound to an ephemeral port + tempfile DB. Returns the
/// child + the URL + the config dir tempdir (held to keep the dir alive).
fn spawn_server(
    extra_env: &[(&str, &str)],
) -> (
    std::process::Child,
    String,
    tempfile::NamedTempFile,
    tempfile::TempDir,
) {
    let port = pick_port();
    let db = tempfile::NamedTempFile::new().expect("tempfile");
    // Slice-19: give the server its own config dir so the token file is isolated
    // from the real ~/.config/wimcc. We keep the TempDir alive until the caller
    // drops it so the token file persists for the doctor subcommand.
    let config_dir = tempfile::tempdir().expect("config tempdir");
    let bin = env!("CARGO_BIN_EXE_wimcc");
    let mut cmd = Command::new(bin);
    cmd.args([
        "--db-path",
        db.path().to_str().unwrap(),
        "serve",
        "--port",
        &port.to_string(),
        "--auto-migrate",
        "--shutdown-after-ms",
        "3000",
        // slice-7: keep doctor tests deterministic regardless of the test
        // host's real ~/.claude/projects contents.
        "--no-watch-transcripts",
    ])
    .env("WIMCC_CONFIG_DIR", config_dir.path())
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let child = cmd.spawn().expect("spawn wimcc serve");
    // Poll /v1/health to confirm readiness.
    let url = format!("http://127.0.0.1:{port}");
    let deadline = std::time::Instant::now() + Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if let Ok(r) = std::thread::spawn({
            let url = url.clone();
            move || ureq_get(&format!("{url}/v1/health"))
        })
        .join()
        .unwrap()
        {
            if r {
                break;
            }
        }
        std::thread::sleep(Duration::from_millis(50));
    }
    (child, url, db, config_dir)
}

fn ureq_get(url: &str) -> std::io::Result<bool> {
    // Tiny synchronous HTTP via std::net (no extra dep).
    let url = url.trim_start_matches("http://");
    let (host, rest) = url.split_once('/').unwrap_or((url, ""));
    let stream = std::net::TcpStream::connect_timeout(
        &host
            .parse()
            .map_err(|_| std::io::Error::other("bad addr"))?,
        Duration::from_millis(200),
    )?;
    stream
        .set_read_timeout(Some(Duration::from_millis(300)))
        .ok();
    use std::io::{Read, Write};
    let mut s = stream;
    write!(
        s,
        "GET /{rest} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n"
    )?;
    let mut buf = String::new();
    let _ = s.take(256).read_to_string(&mut buf);
    // Slice-19: 401 means the server is up but token auth is required.
    // Both 200 and 401 mean the server is ready.
    Ok(buf.starts_with("HTTP/1.1 200") || buf.starts_with("HTTP/1.1 401"))
}

#[test]
fn doctor_pretty_against_live_server_lists_taxonomy_and_exits_0() {
    let (mut child, url, _db, config_dir) = spawn_server(&[]);
    let out = AssertCmd::cargo_bin("wimcc")
        .unwrap()
        .args(["doctor", "--server", &url])
        .env("WIMCC_CONFIG_DIR", config_dir.path())
        .env_remove("CLAUDE_CODE_ENABLE_TELEMETRY")
        .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
        .output()
        .expect("doctor");
    let _ = child.kill();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("wimcc doctor"));
    assert!(stdout.contains("transcript"));
    assert!(stdout.contains("otel-metrics"));
    assert!(stdout.contains("otel-logs"));
    // server reachable but no source has data → exit 1 per spec
    assert_eq!(out.status.code(), Some(1), "no sources → exit 1");
}

#[test]
fn doctor_json_mode_emits_parseable_report() {
    let (mut child, url, _db, config_dir) = spawn_server(&[]);
    let out = AssertCmd::cargo_bin("wimcc")
        .unwrap()
        .args(["doctor", "--json", "--server", &url])
        .env("WIMCC_CONFIG_DIR", config_dir.path())
        .env_remove("CLAUDE_CODE_ENABLE_TELEMETRY")
        .output()
        .expect("doctor");
    let _ = child.kill();
    let stdout = String::from_utf8_lossy(&out.stdout);
    let v: serde_json::Value =
        serde_json::from_str(&stdout).expect("doctor --json must be valid JSON");
    assert!(v["envs"].is_array());
    assert!(v["server"]["reachable"].as_bool().unwrap());
    let sources = v["server"]["sources"].as_array().unwrap();
    // Slice-10a — file-git removed. 2026-06-19 — hook removed with the hook
    // collector. Remaining taxonomy: transcript + 3 OTel signals.
    assert_eq!(sources.len(), 4, "fixed taxonomy of 4 sources");
    // --json always exits 0
    assert_eq!(out.status.code(), Some(0));
}

// ---- slice-7 v0.2 — multi-scope settings hierarchy + plugin manifests ----

use std::io::Write;

fn write_json(path: &std::path::Path, body: &serde_json::Value) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    let mut f = std::fs::File::create(path).unwrap();
    write!(f, "{}", serde_json::to_string_pretty(body).unwrap()).unwrap();
}

#[test]
fn doctor_v02_project_scope_env_attribution() {
    let (mut child, url, _db, _cfg) = spawn_server(&[]);
    let project = tempfile::tempdir().unwrap();
    // .git so the walk stops here; .claude/settings.json with OTel env.
    std::fs::create_dir_all(project.path().join(".git")).unwrap();
    write_json(
        &project.path().join(".claude").join("settings.json"),
        &serde_json::json!({
            "env": {
                "OTEL_METRICS_EXPORTER": "otlp",
                "OTEL_EXPORTER_OTLP_ENDPOINT": "http://localhost:7878/otel"
            }
        }),
    );
    let out = AssertCmd::cargo_bin("wimcc")
        .unwrap()
        .args([
            "doctor",
            "--json",
            "--server",
            &url,
            "--project",
            project.path().to_str().unwrap(),
        ])
        .env_remove("OTEL_METRICS_EXPORTER")
        .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
        .env(
            "WIMCC_DOCTOR_PLUGINS_ROOT",
            project.path().join("noplugins"),
        )
        .output()
        .expect("doctor");
    let _ = child.kill();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("json");
    let env = &v["effective_env"];
    assert_eq!(env["OTEL_METRICS_EXPORTER"]["value"], "otlp");
    assert_eq!(env["OTEL_METRICS_EXPORTER"]["scope"], "project");
    assert_eq!(
        env["OTEL_EXPORTER_OTLP_ENDPOINT"]["value"],
        "http://localhost:7878/otel"
    );
    assert_eq!(env["OTEL_EXPORTER_OTLP_ENDPOINT"]["scope"], "project");
}

#[test]
fn doctor_v02_local_overrides_project_scope() {
    let (mut child, url, _db, _cfg) = spawn_server(&[]);
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join(".git")).unwrap();
    write_json(
        &project.path().join(".claude").join("settings.json"),
        &serde_json::json!({ "env": { "OTEL_EXPORTER_OTLP_ENDPOINT": "http://project:7878/otel" } }),
    );
    write_json(
        &project.path().join(".claude").join("settings.local.json"),
        &serde_json::json!({ "env": { "OTEL_EXPORTER_OTLP_ENDPOINT": "http://local:7878/otel" } }),
    );
    let out = AssertCmd::cargo_bin("wimcc")
        .unwrap()
        .args([
            "doctor",
            "--json",
            "--server",
            &url,
            "--project",
            project.path().to_str().unwrap(),
        ])
        .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
        .env(
            "WIMCC_DOCTOR_PLUGINS_ROOT",
            project.path().join("noplugins"),
        )
        .output()
        .expect("doctor");
    let _ = child.kill();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("json");
    let entry = &v["effective_env"]["OTEL_EXPORTER_OTLP_ENDPOINT"];
    assert_eq!(entry["value"], "http://local:7878/otel");
    assert_eq!(entry["scope"], "local");
}

#[test]
fn doctor_v02_env_divergence_when_settings_has_more_than_shell() {
    let (mut child, url, _db, _cfg) = spawn_server(&[]);
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join(".git")).unwrap();
    write_json(
        &project.path().join(".claude").join("settings.json"),
        &serde_json::json!({ "env": { "OTEL_METRICS_EXPORTER": "otlp" } }),
    );
    let out = AssertCmd::cargo_bin("wimcc")
        .unwrap()
        .args([
            "doctor",
            "--json",
            "--server",
            &url,
            "--project",
            project.path().to_str().unwrap(),
        ])
        .env_remove("OTEL_METRICS_EXPORTER")
        .env(
            "WIMCC_DOCTOR_PLUGINS_ROOT",
            project.path().join("noplugins"),
        )
        .output()
        .expect("doctor");
    let _ = child.kill();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("json");
    let divergence = v["env_divergence"]
        .as_array()
        .expect("env_divergence array");
    assert!(
        divergence
            .iter()
            .any(|d| d["key"] == "OTEL_METRICS_EXPORTER"
                && d["file_value"] == "otlp"
                && d["process_value"].is_null()),
        "should report file vs (unset) divergence; got {divergence:?}",
    );
}

/// Invariant from the PR-5 doctor scope discussion: hook + file-git are
/// user-configured externals (forward script / `serve --watch`) so doctor
/// cannot diagnose how they should be wired. They must not appear in the
/// "no data, do X" recommendation block.
#[test]
fn doctor_recommendations_omit_hook_and_file_git() {
    let (mut child, url, _db, _cfg) = spawn_server(&[]);
    let out = AssertCmd::cargo_bin("wimcc")
        .unwrap()
        .args(["doctor", "--server", &url])
        .env_remove("CLAUDE_CODE_ENABLE_TELEMETRY")
        .output()
        .expect("doctor");
    let _ = child.kill();
    let stdout = String::from_utf8_lossy(&out.stdout);
    // Recommendation block must NOT mention hook substring matching or
    // file-git path advice — those are not actionable from doctor's vantage.
    let recs_start = stdout.find("# Recommendations").unwrap_or(stdout.len());
    let recs = &stdout[recs_start..];
    assert!(
        !recs.to_lowercase().contains("forward to /hooks/v1/events"),
        "hook forward substring should not be in recommendations; got:\n{recs}"
    );
    assert!(
        !recs.to_lowercase().contains("no data yet from")
            || !recs.contains("hook") && !recs.contains("file-git"),
        "'no data yet from' should never list hook or file-git; got:\n{recs}"
    );
}

/// Exit code must be driven only by actionable sources (transcript + the
/// three OTel signals). A DB that has hook rows but no transcript/OTel must
/// still exit 1 — hook arrival alone does not prove the wiring is correct.
#[test]
fn doctor_exit_ignores_hook_only_data() {
    let port = pick_port();
    let db = tempfile::NamedTempFile::new().expect("tempfile");
    let bin = env!("CARGO_BIN_EXE_wimcc");

    // Migrate then insert a single hook raw row.
    let init = std::process::Command::new(bin)
        .args(["--db-path", db.path().to_str().unwrap(), "init-db"])
        .output()
        .expect("init-db");
    assert!(init.status.success());

    let conn = rusqlite_open(db.path());
    conn.execute(
        "INSERT INTO ingest_run(run_id, started_at, status, stats) VALUES('run','2026-05-21T00:00:00Z','ok','{}')",
        [],
    ).unwrap();
    conn.execute(
        "INSERT INTO raw_event(raw_event_id, ingest_run_id, source_type, source_uri, source_line_no, source_byte_offset, payload_sha256, payload, parse_error, captured_at) \
         VALUES('r1','run','hook','hook://t/1',0,0,'sha',X'7B7D',NULL,datetime('now'))",
        [],
    ).unwrap();
    drop(conn);

    // Spawn wimcc serve against this DB.
    let mut child = std::process::Command::new(bin)
        .args([
            "--db-path",
            db.path().to_str().unwrap(),
            "serve",
            "--port",
            &port.to_string(),
            "--auto-migrate",
            "--shutdown-after-ms",
            "3000",
            "--no-watch-transcripts",
        ])
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .expect("spawn");
    // Wait for ready.
    let url = format!("http://127.0.0.1:{port}");
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(2);
    while std::time::Instant::now() < deadline {
        if let Ok(r) = std::thread::spawn({
            let url = url.clone();
            move || ureq_get(&format!("{url}/v1/health"))
        })
        .join()
        .unwrap()
        {
            if r {
                break;
            }
        }
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    let out = AssertCmd::cargo_bin("wimcc")
        .unwrap()
        .args(["doctor", "--server", &url])
        .output()
        .expect("doctor");
    let _ = child.kill();
    let _ = child.wait();
    assert_eq!(
        out.status.code(),
        Some(1),
        "hook-only data must still exit 1 (hook is informational)"
    );
}

fn rusqlite_open(_path: &std::path::Path) -> RusqliteShim {
    // We don't want to depend on rusqlite for one test. Use sqlx blocking via
    // a tiny tokio runtime instead.
    RusqliteShim::new(_path.to_path_buf())
}

struct RusqliteShim {
    rt: tokio::runtime::Runtime,
    pool: sqlx::SqlitePool,
}

impl RusqliteShim {
    fn new(path: std::path::PathBuf) -> Self {
        let rt = tokio::runtime::Runtime::new().unwrap();
        let url = format!("sqlite://{}?mode=rwc", path.display());
        let pool = rt
            .block_on(async { sqlx::SqlitePool::connect(&url).await })
            .unwrap();
        Self { rt, pool }
    }
    fn execute(&self, sql: &str, _: [(); 0]) -> Result<u64, sqlx::Error> {
        self.rt.block_on(async {
            sqlx::query(sql)
                .execute(&self.pool)
                .await
                .map(|r| r.rows_affected())
        })
    }
}

#[test]
fn doctor_unreachable_server_exits_1_with_error() {
    // No spawn; point at an unbound port.
    let port = pick_port();
    let url = format!("http://127.0.0.1:{port}");
    let out = AssertCmd::cargo_bin("wimcc")
        .unwrap()
        .args(["doctor", "--server", &url])
        .output()
        .expect("doctor");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("unreachable"));
    assert_eq!(out.status.code(), Some(1));
}
