//! Slice-6 — `witmcc doctor`.
//!
//! Read-only diagnostic: dumps OTel env vars, peeks at `~/.claude/settings.json`
//! for hook wiring, probes the running witmcc server for per-source freshness.
//! No file mutation (CLAUDE.md non-goal). Prints copy-pastable recommendations
//! when items are missing; never auto-applies anything.

use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::time::Duration;

const EXPECTED_ENDPOINT_SUFFIX: &str = "/otel";

const OTEL_ENVS: &[(&str, &str)] = &[
    ("CLAUDE_CODE_ENABLE_TELEMETRY", "1"),
    ("CLAUDE_CODE_ENHANCED_TELEMETRY_BETA", "1"),
    ("OTEL_METRICS_EXPORTER", "otlp"),
    ("OTEL_LOGS_EXPORTER", "otlp"),
    ("OTEL_TRACES_EXPORTER", "otlp"),
    ("OTEL_EXPORTER_OTLP_PROTOCOL", "http/json"),
    // OTEL_EXPORTER_OTLP_ENDPOINT is checked separately (suffix rule).
];

pub struct DoctorOpts {
    pub json: bool,
    pub server: String,
}

#[derive(Debug, Serialize)]
struct EnvCheck {
    key: String,
    value: Option<String>,
    expected: Option<String>,
    status: &'static str, // "ok" | "wrong" | "unset"
    note: Option<String>,
}

#[derive(Debug, Serialize)]
struct HookSettingsCheck {
    settings_path: String,
    settings_present: bool,
    wired_for_witmcc: bool,
    wired_hook_events: Vec<String>,
    note: Option<String>,
}

#[derive(Debug, Serialize, Default)]
struct ServerProbe {
    reachable: bool,
    health_status_code: Option<u16>,
    build_sha: Option<String>,
    sources: Vec<SourceFreshness>,
    error: Option<String>,
}

#[derive(Debug, Serialize)]
struct SourceFreshness {
    label: String,
    last_ingested_at: Option<String>,
    row_count_24h: i64,
    total_rows: i64,
    status: &'static str, // "recent" | "stale" | "no_data"
}

#[derive(Debug, Serialize)]
struct DoctorReport {
    envs: Vec<EnvCheck>,
    endpoint: EnvCheck,
    hook_settings: HookSettingsCheck,
    server: ServerProbe,
    recommendations: Vec<String>,
    exit_code: i32,
}

fn ansi(code: &str, s: impl AsRef<str>) -> String {
    format!("\x1b[{}m{}\x1b[0m", code, s.as_ref())
}
fn green(s: impl AsRef<str>) -> String {
    ansi("32", s)
}
fn yellow(s: impl AsRef<str>) -> String {
    ansi("33", s)
}
fn red(s: impl AsRef<str>) -> String {
    ansi("31", s)
}
fn dim(s: impl AsRef<str>) -> String {
    ansi("2", s)
}

fn check_env(key: &str, expected: Option<&str>) -> EnvCheck {
    let value = std::env::var(key).ok();
    let (status, note) = match (&value, expected) {
        (Some(v), Some(want)) if v == want => ("ok", None),
        (Some(v), Some(want)) => (
            "wrong",
            Some(format!("got {v:?}, expected {want:?}")),
        ),
        (Some(_), None) => ("ok", None),
        (None, _) => ("unset", None),
    };
    EnvCheck {
        key: key.to_string(),
        value,
        expected: expected.map(String::from),
        status,
        note,
    }
}

fn check_endpoint() -> EnvCheck {
    let key = "OTEL_EXPORTER_OTLP_ENDPOINT";
    let value = std::env::var(key).ok();
    let (status, note) = match &value {
        None => ("unset", None),
        Some(v) if v.trim_end_matches('/').ends_with(EXPECTED_ENDPOINT_SUFFIX) => ("ok", None),
        Some(v) => (
            "wrong",
            Some(format!(
                "must end with {EXPECTED_ENDPOINT_SUFFIX} so OTel SDK posts to .../otel/v1/<signal> — got {v:?}"
            )),
        ),
    };
    EnvCheck {
        key: key.to_string(),
        value,
        expected: Some(format!("…{EXPECTED_ENDPOINT_SUFFIX}")),
        status,
        note,
    }
}

fn read_hook_settings() -> HookSettingsCheck {
    let path = match dirs::home_dir() {
        Some(h) => h.join(".claude").join("settings.json"),
        None => {
            return HookSettingsCheck {
                settings_path: "~/.claude/settings.json".into(),
                settings_present: false,
                wired_for_witmcc: false,
                wired_hook_events: vec![],
                note: Some("HOME not set".into()),
            }
        }
    };
    let raw = match std::fs::read_to_string(&path) {
        Ok(s) => s,
        Err(_) => {
            return HookSettingsCheck {
                settings_path: path.display().to_string(),
                settings_present: false,
                wired_for_witmcc: false,
                wired_hook_events: vec![],
                note: Some("file missing — hook collector will receive nothing".into()),
            }
        }
    };
    let parsed: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(e) => {
            return HookSettingsCheck {
                settings_path: path.display().to_string(),
                settings_present: true,
                wired_for_witmcc: false,
                wired_hook_events: vec![],
                note: Some(format!("settings.json parse error: {e}")),
            }
        }
    };
    let mut wired_events = Vec::new();
    if let Some(hooks) = parsed.get("hooks").and_then(|v| v.as_object()) {
        for (event_name, entries) in hooks {
            let mut event_wired = false;
            if let Some(arr) = entries.as_array() {
                for entry in arr {
                    if let Some(inner) = entry.get("hooks").and_then(|v| v.as_array()) {
                        for h in inner {
                            if let Some(cmd) = h.get("command").and_then(|v| v.as_str()) {
                                if cmd.contains("hooks/v1/events") {
                                    event_wired = true;
                                }
                            }
                        }
                    }
                }
            }
            if event_wired {
                wired_events.push(event_name.clone());
            }
        }
    }
    let wired = !wired_events.is_empty();
    wired_events.sort();
    HookSettingsCheck {
        settings_path: path.display().to_string(),
        settings_present: true,
        wired_for_witmcc: wired,
        wired_hook_events: wired_events,
        note: if wired {
            None
        } else {
            Some("no hook entries forward to witmcc — see README".into())
        },
    }
}

async fn probe_server(server: &str) -> ServerProbe {
    let client = match reqwest::Client::builder()
        .timeout(Duration::from_secs(2))
        .build()
    {
        Ok(c) => c,
        Err(e) => {
            return ServerProbe {
                error: Some(format!("client build failed: {e}")),
                ..Default::default()
            }
        }
    };
    let health_url = format!("{}/v1/health", server.trim_end_matches('/'));
    let sources_url = format!("{}/v1/health/sources", server.trim_end_matches('/'));
    let mut probe = ServerProbe::default();

    match client.get(&health_url).send().await {
        Ok(r) => {
            probe.reachable = r.status().is_success();
            probe.health_status_code = Some(r.status().as_u16());
            if probe.reachable {
                if let Ok(j) = r.json::<serde_json::Value>().await {
                    probe.build_sha = j
                        .get("build_sha")
                        .and_then(|v| v.as_str())
                        .map(String::from);
                }
            }
        }
        Err(e) => {
            probe.error = Some(format!("{health_url}: {e}"));
            return probe;
        }
    }

    if probe.reachable {
        match client.get(&sources_url).send().await {
            Ok(r) if r.status().is_success() => {
                if let Ok(j) = r.json::<serde_json::Value>().await {
                    if let Some(sources) = j["data"]["sources"].as_array() {
                        for s in sources {
                            let label =
                                s["label"].as_str().unwrap_or("?").to_string();
                            let last = s["last_ingested_at"].as_str().map(String::from);
                            let r24 = s["row_count_24h"].as_i64().unwrap_or(0);
                            let tot = s["total_rows"].as_i64().unwrap_or(0);
                            let status = if tot == 0 {
                                "no_data"
                            } else if r24 > 0 {
                                "recent"
                            } else {
                                "stale"
                            };
                            probe.sources.push(SourceFreshness {
                                label,
                                last_ingested_at: last,
                                row_count_24h: r24,
                                total_rows: tot,
                                status,
                            });
                        }
                    }
                }
            }
            Ok(r) => {
                probe.error = Some(format!("{sources_url}: HTTP {}", r.status().as_u16()));
            }
            Err(e) => {
                probe.error = Some(format!("{sources_url}: {e}"));
            }
        }
    }
    probe
}

fn build_recommendations(report: &DoctorReport) -> Vec<String> {
    let mut out = Vec::new();
    let mut env_block: BTreeMap<&str, String> = BTreeMap::new();
    for e in &report.envs {
        if e.status != "ok" {
            if let Some(want) = &e.expected {
                env_block.insert(e.key.as_str(), want.clone());
            }
        }
    }
    if report.endpoint.status != "ok" {
        env_block.insert(
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            "http://localhost:7878/otel".into(),
        );
    }
    if !env_block.is_empty() {
        let mut block = String::from(
            "Set the following in ~/.claude/settings.json env (or as shell exports):\n",
        );
        for (k, v) in env_block {
            block.push_str(&format!("    {k}={v}\n"));
        }
        out.push(block.trim_end().to_string());
    }
    if !report.hook_settings.wired_for_witmcc {
        out.push(
            "Register at least one hook forwarder in ~/.claude/settings.json. \
             See README 'Hook Collector (slice-4)' for the snippet."
                .into(),
        );
    }
    if !report.server.reachable {
        out.push(format!(
            "witmcc server unreachable. Start it with: `witmcc serve --auto-migrate`."
        ));
    } else {
        let no_data: Vec<&str> = report
            .server
            .sources
            .iter()
            .filter(|s| s.status == "no_data")
            .map(|s| s.label.as_str())
            .collect();
        if !no_data.is_empty() {
            out.push(format!(
                "No data yet from: {}. Run a `claude` session with telemetry on and try again.",
                no_data.join(", ")
            ));
        }
    }
    out
}

fn compute_exit_code(report: &DoctorReport) -> i32 {
    if !report.server.reachable {
        return 1;
    }
    let any_recent_source = report
        .server
        .sources
        .iter()
        .any(|s| s.status == "recent" || s.status == "stale");
    if any_recent_source {
        0
    } else {
        1
    }
}

fn print_pretty<W: Write>(w: &mut W, report: &DoctorReport) -> std::io::Result<()> {
    writeln!(w, "witmcc doctor — read-only diagnostic\n")?;
    writeln!(w, "{}", dim("# Environment (Claude Code OTel)"))?;
    for e in &report.envs {
        let marker = match e.status {
            "ok" => green("✓"),
            "wrong" => red("✗"),
            _ => yellow("∅"),
        };
        let val = e.value.clone().unwrap_or_else(|| "(unset)".into());
        writeln!(w, "  {marker} {:<40} = {val}", e.key)?;
        if let Some(note) = &e.note {
            writeln!(w, "      {}", dim(note))?;
        }
    }
    {
        let e = &report.endpoint;
        let marker = match e.status {
            "ok" => green("✓"),
            "wrong" => red("✗"),
            _ => yellow("∅"),
        };
        let val = e.value.clone().unwrap_or_else(|| "(unset)".into());
        writeln!(w, "  {marker} {:<40} = {val}", e.key)?;
        if let Some(note) = &e.note {
            writeln!(w, "      {}", dim(note))?;
        }
    }
    writeln!(w)?;

    writeln!(w, "{}", dim("# Hook settings (~/.claude/settings.json)"))?;
    let hs = &report.hook_settings;
    let marker = if hs.wired_for_witmcc {
        green("✓")
    } else {
        yellow("∅")
    };
    writeln!(w, "  {marker} {}", hs.settings_path)?;
    if hs.wired_for_witmcc {
        writeln!(
            w,
            "      wired hook events: {}",
            hs.wired_hook_events.join(", ")
        )?;
    } else if let Some(n) = &hs.note {
        writeln!(w, "      {}", dim(n))?;
    }
    writeln!(w)?;

    writeln!(w, "{}", dim("# Server (loopback Pull API)"))?;
    let sp = &report.server;
    if sp.reachable {
        writeln!(
            w,
            "  {} reachable (build_sha = {})",
            green("✓"),
            sp.build_sha.as_deref().unwrap_or("?")
        )?;
        writeln!(w)?;
        writeln!(
            w,
            "  {:<14} {:<28} {:>9} {:>8}",
            "source", "last_ingested_at", "rows/24h", "total"
        )?;
        for s in &sp.sources {
            let marker = match s.status {
                "recent" => green("✓"),
                "stale" => yellow("~"),
                _ => red("✗"),
            };
            let last = s.last_ingested_at.as_deref().unwrap_or("—");
            writeln!(
                w,
                "  {marker} {:<12} {:<28} {:>9} {:>8}",
                s.label, last, s.row_count_24h, s.total_rows
            )?;
        }
    } else {
        writeln!(w, "  {} unreachable", red("✗"))?;
        if let Some(e) = &sp.error {
            writeln!(w, "      {}", dim(e))?;
        }
    }
    writeln!(w)?;

    if !report.recommendations.is_empty() {
        writeln!(w, "{}", dim("# Recommendations (none of these will be applied automatically)"))?;
        for r in &report.recommendations {
            for line in r.lines() {
                writeln!(w, "  {line}")?;
            }
            writeln!(w)?;
        }
    }
    writeln!(w, "exit_code = {}", report.exit_code)?;
    Ok(())
}

pub async fn run(opts: DoctorOpts) -> std::io::Result<i32> {
    let envs: Vec<EnvCheck> = OTEL_ENVS
        .iter()
        .map(|(k, expected)| check_env(k, Some(*expected)))
        .collect();
    let endpoint = check_endpoint();
    let hook_settings = read_hook_settings();
    let server = probe_server(&opts.server).await;
    let mut report = DoctorReport {
        envs,
        endpoint,
        hook_settings,
        server,
        recommendations: vec![],
        exit_code: 0,
    };
    report.recommendations = build_recommendations(&report);
    report.exit_code = compute_exit_code(&report);

    let mut stdout = std::io::stdout().lock();
    if opts.json {
        let json = serde_json::to_string_pretty(&report).unwrap_or_else(|_| "{}".into());
        writeln!(stdout, "{json}")?;
        Ok(0)
    } else {
        print_pretty(&mut stdout, &report)?;
        Ok(report.exit_code)
    }
}
