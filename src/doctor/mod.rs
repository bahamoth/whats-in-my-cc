//! `wimcc doctor` — read-only diagnostic for OTel collector wiring.
//!
//! Slice-6 v0.1: process env + single user settings.json.
//! Slice-7 v0.2: full Claude Code settings hierarchy walk (managed > local >
//! project > user) + scope attribution per env value. Still never mutates
//! anything.
//!
//! (Hook-forward diagnostics removed 2026-06-19 with the hook collector.)

pub mod settings;

use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use self::settings::{EnvSource, SettingsScope};

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
    /// Slice-7: project root for `.claude/settings.json` walk. Defaults to CWD.
    pub project: Option<PathBuf>,
}

#[derive(Debug, Serialize)]
struct EnvCheck {
    key: String,
    value: Option<String>,
    expected: Option<String>,
    status: &'static str, // "ok" | "wrong" | "unset"
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
    server: ServerProbe,
    recommendations: Vec<String>,
    exit_code: i32,
    // slice-7 settings hierarchy
    settings_scopes: Vec<SettingsScope>,
    effective_env: BTreeMap<String, EnvSource>,
    env_divergence: Vec<EnvDivergence>,
}

#[derive(Debug, Serialize)]
struct EnvDivergence {
    key: String,
    file_value: Option<String>,
    file_scope: Option<String>,
    process_value: Option<String>,
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
        (Some(v), Some(want)) => ("wrong", Some(format!("got {v:?}, expected {want:?}"))),
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

/// Attempt to load the wimcc token from `WIMCC_CONFIG_DIR/token` or
/// `~/.config/wimcc/token`. Returns `None` if the file doesn't exist or can't
/// be read (doctor stays useful even without credentials).
fn wimcc_token_from_env_or_file() -> Option<String> {
    let dir = if let Ok(v) = std::env::var("WIMCC_CONFIG_DIR") {
        std::path::PathBuf::from(v)
    } else {
        dirs::config_dir()?.join("wimcc")
    };
    let path = dir.join("token");
    std::fs::read_to_string(path)
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
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

    // Slice-19: attempt with token from the token file (if available).
    // Fall back to unauthenticated so doctor remains useful even without the
    // token file (e.g., on a different machine than the server).
    let token: Option<String> = wimcc_token_from_env_or_file();
    let health_req = if let Some(ref t) = token {
        client
            .get(&health_url)
            .header("Authorization", format!("Bearer {t}"))
    } else {
        client.get(&health_url)
    };

    match health_req.send().await {
        Ok(r) => {
            // Slice-19: 401 means the server is up but token auth is required.
            // We still mark the server as reachable; the token issue is a config
            // problem, not a connectivity problem.
            let reachable = r.status().is_success() || r.status() == 401;
            probe.reachable = reachable;
            probe.health_status_code = Some(r.status().as_u16());
            if r.status().is_success() {
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
        let sources_req = if let Some(ref t) = token {
            client
                .get(&sources_url)
                .header("Authorization", format!("Bearer {t}"))
        } else {
            client.get(&sources_url)
        };
        match sources_req.send().await {
            Ok(r) if r.status().is_success() => {
                if let Ok(j) = r.json::<serde_json::Value>().await {
                    if let Some(sources) = j["data"]["sources"].as_array() {
                        for s in sources {
                            let label = s["label"].as_str().unwrap_or("?").to_string();
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

    // Use the file-effective env (settings hierarchy) as the source of truth
    // for what `claude` will actually see, not process env.
    let mut missing_env: BTreeMap<&str, String> = BTreeMap::new();
    for (k, expected) in OTEL_ENVS.iter() {
        let present_in_file = report
            .effective_env
            .get(*k)
            .map(|v| v.value == *expected)
            .unwrap_or(false);
        let present_in_process = std::env::var(k).ok().as_deref() == Some(*expected);
        if !present_in_file && !present_in_process {
            missing_env.insert(*k, expected.to_string());
        }
    }
    // endpoint suffix rule
    let endpoint_ok_file = report
        .effective_env
        .get("OTEL_EXPORTER_OTLP_ENDPOINT")
        .map(|v| {
            v.value
                .trim_end_matches('/')
                .ends_with(EXPECTED_ENDPOINT_SUFFIX)
        })
        .unwrap_or(false);
    let endpoint_ok_process = std::env::var("OTEL_EXPORTER_OTLP_ENDPOINT")
        .ok()
        .map(|v| v.trim_end_matches('/').ends_with(EXPECTED_ENDPOINT_SUFFIX))
        .unwrap_or(false);
    if !endpoint_ok_file && !endpoint_ok_process {
        missing_env.insert(
            "OTEL_EXPORTER_OTLP_ENDPOINT",
            "http://localhost:7878/otel".into(),
        );
    }
    if !missing_env.is_empty() {
        let mut block = String::from(
            "Add to ~/.claude/settings.json `env` block (or any higher-precedence scope):\n",
        );
        for (k, v) in missing_env {
            block.push_str(&format!("    \"{k}\": \"{v}\",\n"));
        }
        out.push(block.trim_end().to_string());
    }

    if !report.server.reachable {
        out.push("wimcc server unreachable. Start it with: `wimcc serve --auto-migrate`.".into());
    } else {
        const ACTIONABLE: &[&str] = &["transcript", "otel-traces", "otel-metrics", "otel-logs"];
        let no_data: Vec<&str> = report
            .server
            .sources
            .iter()
            .filter(|s| s.status == "no_data" && ACTIONABLE.contains(&s.label.as_str()))
            .map(|s| s.label.as_str())
            .collect();
        if !no_data.is_empty() {
            out.push(format!(
                "No data yet from: {}. Run a `claude` session against wimcc to populate.",
                no_data.join(", ")
            ));
        }
        // B-11 (2026-07-04): transcript는 있는데 OTLP가 비어 있으면 verification
        // outcome의 구조적 unknown 클래스를 안내한다 — 성공한 명령은 transcript에
        // exit 0 신호를 남기지 않아(2026-07-03 루프 실측) transcript-only로는
        // 영원히 unknown이고, OTLP tool_result 수집이 measured로 해소한다.
        let transcript_has_data = report
            .server
            .sources
            .iter()
            .any(|s| s.label == "transcript" && (s.status == "recent" || s.status == "stale"));
        let otlp_all_empty = report
            .server
            .sources
            .iter()
            .filter(|s| s.label.starts_with("otel-"))
            .all(|s| s.status == "no_data");
        if transcript_has_data && otlp_all_empty {
            out.push(
                "Transcript-only collection leaves successful verification runs at \
                 outcome=unknown (transcripts carry no exit-0 signal). Enable Claude \
                 Code OTLP export (env vars above) so tool results resolve to \
                 measured outcomes."
                    .into(),
            );
        }
    }
    out
}

fn compute_exit_code(report: &DoctorReport) -> i32 {
    if !report.server.reachable {
        return 1;
    }
    const ACTIONABLE: &[&str] = &["transcript", "otel-traces", "otel-metrics", "otel-logs"];
    let any_actionable_source_has_data = report
        .server
        .sources
        .iter()
        .filter(|s| ACTIONABLE.contains(&s.label.as_str()))
        .any(|s| s.status == "recent" || s.status == "stale");
    if any_actionable_source_has_data {
        0
    } else {
        1
    }
}

fn print_pretty<W: Write>(w: &mut W, report: &DoctorReport) -> std::io::Result<()> {
    writeln!(w, "wimcc doctor — read-only diagnostic\n")?;
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

    // ---- slice-7 v0.2 sections ----
    writeln!(
        w,
        "{}",
        dim("# Settings files probed (Claude Code hierarchy)")
    )?;
    for s in &report.settings_scopes {
        let marker = if s.present {
            green("✓")
        } else {
            yellow("∅")
        };
        writeln!(w, "  {marker} {:<10} {}", s.label, s.path.display())?;
        if let Some(n) = &s.note {
            writeln!(w, "      {}", dim(n))?;
        }
    }
    writeln!(w)?;

    writeln!(
        w,
        "{}",
        dim("# Effective OTel env (file scope = what `claude` will see)")
    )?;
    if report.effective_env.is_empty() {
        writeln!(w, "  {} no env block in any scope", yellow("∅"))?;
    } else {
        for (k, src) in &report.effective_env {
            writeln!(
                w,
                "  {} {:<40} = {:<24} ({})",
                green("✓"),
                k,
                src.value,
                src.scope
            )?;
        }
    }
    if !report.env_divergence.is_empty() {
        writeln!(w)?;
        writeln!(
            w,
            "  {}",
            dim("env divergence between file scope and current shell:")
        )?;
        for d in &report.env_divergence {
            let file = d
                .file_value
                .as_deref()
                .map(|v| format!("file:{}={}", d.file_scope.as_deref().unwrap_or("?"), v))
                .unwrap_or_else(|| "file:(none)".into());
            let proc = d
                .process_value
                .as_deref()
                .map(|v| format!("process={v}"))
                .unwrap_or_else(|| "process:(unset)".into());
            writeln!(w, "    {} {:<40}  {file}  {proc}", yellow("~"), d.key)?;
        }
    }
    writeln!(w)?;

    if !report.recommendations.is_empty() {
        writeln!(
            w,
            "{}",
            dim("# Recommendations (none of these will be applied automatically)")
        )?;
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
    let server = probe_server(&opts.server).await;

    // ---- slice-7 v0.2: settings hierarchy walk ----
    let project_start = opts
        .project
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let settings_scopes = settings::scopes(&project_start);
    let effective_env_map = settings::effective_env(&settings_scopes);

    // env divergence: file-effective vs process env, for keys relevant to OTel.
    let mut env_divergence: Vec<EnvDivergence> = Vec::new();
    let mut relevant_keys: Vec<String> = OTEL_ENVS.iter().map(|(k, _)| k.to_string()).collect();
    relevant_keys.push("OTEL_EXPORTER_OTLP_ENDPOINT".into());
    for key in &relevant_keys {
        let file = effective_env_map.get(key);
        let process = std::env::var(key).ok();
        let differs = match (file, &process) {
            (Some(f), Some(p)) => &f.value != p,
            (Some(_), None) => true,
            (None, Some(_)) => true,
            (None, None) => false,
        };
        if differs {
            env_divergence.push(EnvDivergence {
                key: key.clone(),
                file_value: file.map(|f| f.value.clone()),
                file_scope: file.map(|f| f.scope.clone()),
                process_value: process,
            });
        }
    }

    let mut report = DoctorReport {
        envs,
        endpoint,
        server,
        recommendations: vec![],
        exit_code: 0,
        settings_scopes,
        effective_env: effective_env_map,
        env_divergence,
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

#[cfg(test)]
mod tests {
    use super::*;

    fn src(label: &str, status: &'static str) -> SourceFreshness {
        SourceFreshness {
            label: label.into(),
            last_ingested_at: None,
            row_count_24h: 0,
            total_rows: 0,
            status,
        }
    }

    fn report_with(sources: Vec<SourceFreshness>) -> DoctorReport {
        DoctorReport {
            envs: vec![],
            endpoint: EnvCheck {
                key: "OTEL_EXPORTER_OTLP_ENDPOINT".into(),
                value: None,
                expected: None,
                status: "unset",
                note: None,
            },
            server: ServerProbe {
                reachable: true,
                health_status_code: Some(200),
                build_sha: None,
                sources,
                error: None,
            },
            recommendations: vec![],
            exit_code: 0,
            settings_scopes: vec![],
            effective_env: Default::default(),
            env_divergence: vec![],
        }
    }

    /// B-11 (2026-07-04): transcript만 있고 OTLP가 비면 구조적-unknown 안내가 뜬다.
    #[test]
    fn recommends_otlp_when_transcript_only() {
        let r = report_with(vec![
            src("transcript", "recent"),
            src("otel-traces", "no_data"),
            src("otel-metrics", "no_data"),
            src("otel-logs", "no_data"),
        ]);
        let recs = build_recommendations(&r);
        assert!(
            recs.iter().any(|m| m.contains("outcome=unknown")),
            "OTLP 안내가 있어야 한다: {recs:?}"
        );
    }

    /// OTLP가 이미 수집되면 안내가 뜨지 않는다.
    #[test]
    fn no_otlp_hint_when_otlp_flowing() {
        let r = report_with(vec![
            src("transcript", "recent"),
            src("otel-traces", "recent"),
            src("otel-metrics", "no_data"),
            src("otel-logs", "no_data"),
        ]);
        let recs = build_recommendations(&r);
        assert!(
            !recs.iter().any(|m| m.contains("outcome=unknown")),
            "OTLP가 흐르면 안내 불필요: {recs:?}"
        );
    }
}
