//! Slice-6 — `witmcc doctor` CLI smoke tests.
//!
//! These tests don't mock the server; they spin up a real witmcc server
//! against a tempfile DB and probe via the doctor subcommand.

use assert_cmd::Command as AssertCmd;
use std::process::{Command, Stdio};
use std::time::Duration;

fn pick_port() -> u16 {
    portpicker::pick_unused_port().expect("no free port")
}

/// Spawn a witmcc server bound to an ephemeral port + tempfile DB. Returns the
/// child + the URL so the caller can probe and then kill on drop.
fn spawn_server(extra_env: &[(&str, &str)]) -> (std::process::Child, String, tempfile::NamedTempFile) {
    let port = pick_port();
    let db = tempfile::NamedTempFile::new().expect("tempfile");
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
        "3000",
        // slice-7: keep doctor tests deterministic regardless of the test
        // host's real ~/.claude/projects contents.
        "--no-watch-transcripts",
    ])
    .stdout(Stdio::null())
    .stderr(Stdio::null());
    for (k, v) in extra_env {
        cmd.env(k, v);
    }
    let child = cmd.spawn().expect("spawn witmcc serve");
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
    (child, url, db)
}

fn ureq_get(url: &str) -> std::io::Result<bool> {
    // Tiny synchronous HTTP via std::net (no extra dep).
    let url = url.trim_start_matches("http://");
    let (host, rest) = url.split_once('/').unwrap_or((url, ""));
    let stream = std::net::TcpStream::connect_timeout(
        &host.parse().map_err(|_| std::io::Error::other("bad addr"))?,
        Duration::from_millis(200),
    )?;
    stream
        .set_read_timeout(Some(Duration::from_millis(300)))
        .ok();
    use std::io::{Read, Write};
    let mut s = stream;
    write!(s, "GET /{rest} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\n\r\n")?;
    let mut buf = String::new();
    let _ = s.take(256).read_to_string(&mut buf);
    Ok(buf.starts_with("HTTP/1.1 200"))
}

#[test]
fn doctor_pretty_against_live_server_lists_taxonomy_and_exits_0() {
    let (mut child, url, _db) = spawn_server(&[]);
    let out = AssertCmd::cargo_bin("witmcc")
        .unwrap()
        .args(["doctor", "--server", &url])
        .env_remove("CLAUDE_CODE_ENABLE_TELEMETRY")
        .env_remove("OTEL_EXPORTER_OTLP_ENDPOINT")
        .output()
        .expect("doctor");
    let _ = child.kill();
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("witmcc doctor"));
    assert!(stdout.contains("transcript"));
    assert!(stdout.contains("otel-metrics"));
    assert!(stdout.contains("otel-logs"));
    // server reachable but no source has data → exit 1 per spec
    assert_eq!(out.status.code(), Some(1), "no sources → exit 1");
}

#[test]
fn doctor_json_mode_emits_parseable_report() {
    let (mut child, url, _db) = spawn_server(&[]);
    let out = AssertCmd::cargo_bin("witmcc")
        .unwrap()
        .args(["doctor", "--json", "--server", &url])
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
    assert_eq!(sources.len(), 6, "fixed taxonomy of 6 sources");
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
    let (mut child, url, _db) = spawn_server(&[]);
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
    let out = AssertCmd::cargo_bin("witmcc")
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
        .env("WITMCC_DOCTOR_PLUGINS_ROOT", project.path().join("noplugins"))
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
    let (mut child, url, _db) = spawn_server(&[]);
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
    let out = AssertCmd::cargo_bin("witmcc")
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
        .env("WITMCC_DOCTOR_PLUGINS_ROOT", project.path().join("noplugins"))
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
fn doctor_v02_plugin_manifest_hook_picked_up() {
    let (mut child, url, _db) = spawn_server(&[]);
    let tmp = tempfile::tempdir().unwrap();
    let plugins_root = tmp.path().join("plugins");
    let plugin_dir = plugins_root.join("my-plugin");
    std::fs::create_dir_all(&plugin_dir).unwrap();
    write_json(
        &plugin_dir.join("plugin.json"),
        &serde_json::json!({
            "hooks": {
                "PostToolUse": [{
                    "hooks": [{ "type": "command", "command": "curl -d @- http://127.0.0.1:7878/hooks/v1/events" }]
                }]
            }
        }),
    );
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join(".git")).unwrap();
    let out = AssertCmd::cargo_bin("witmcc")
        .unwrap()
        .args([
            "doctor",
            "--json",
            "--server",
            &url,
            "--project",
            project.path().to_str().unwrap(),
        ])
        .env("WITMCC_DOCTOR_PLUGINS_ROOT", &plugins_root)
        .output()
        .expect("doctor");
    let _ = child.kill();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("json");
    let plugin_hooks = v["plugin_hooks"].as_array().expect("plugin_hooks array");
    let any_witmcc = plugin_hooks
        .iter()
        .any(|h| h["forwards_to_witmcc"].as_bool().unwrap_or(false));
    assert!(any_witmcc, "plugin hook forwarding to /hooks/v1/events should be flagged");
    assert!(plugin_hooks
        .iter()
        .any(|h| h["scope"].as_str().unwrap_or("").starts_with("plugin:")));
}

#[test]
fn doctor_v02_env_divergence_when_settings_has_more_than_shell() {
    let (mut child, url, _db) = spawn_server(&[]);
    let project = tempfile::tempdir().unwrap();
    std::fs::create_dir_all(project.path().join(".git")).unwrap();
    write_json(
        &project.path().join(".claude").join("settings.json"),
        &serde_json::json!({ "env": { "OTEL_METRICS_EXPORTER": "otlp" } }),
    );
    let out = AssertCmd::cargo_bin("witmcc")
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
        .env("WITMCC_DOCTOR_PLUGINS_ROOT", project.path().join("noplugins"))
        .output()
        .expect("doctor");
    let _ = child.kill();
    let v: serde_json::Value =
        serde_json::from_str(&String::from_utf8_lossy(&out.stdout)).expect("json");
    let divergence = v["env_divergence"].as_array().expect("env_divergence array");
    assert!(
        divergence
            .iter()
            .any(|d| d["key"] == "OTEL_METRICS_EXPORTER"
                && d["file_value"] == "otlp"
                && d["process_value"].is_null()),
        "should report file vs (unset) divergence; got {divergence:?}",
    );
}

#[test]
fn doctor_unreachable_server_exits_1_with_error() {
    // No spawn; point at an unbound port.
    let port = pick_port();
    let url = format!("http://127.0.0.1:{port}");
    let out = AssertCmd::cargo_bin("witmcc")
        .unwrap()
        .args(["doctor", "--server", &url])
        .output()
        .expect("doctor");
    let stdout = String::from_utf8_lossy(&out.stdout);
    assert!(stdout.contains("unreachable"));
    assert_eq!(out.status.code(), Some(1));
}
