//! `witmcc doctor` — read-only diagnostic for collector wiring.
//!
//! Slice-6 v0.1: process env + single user settings.json.
//! Slice-7 v0.2: full Claude Code settings hierarchy walk (managed > local >
//! project > user) + plugin manifests + managed policy detection + scope
//! attribution per env/hook value. Still never mutates anything.

pub mod settings;

use serde::Serialize;
use std::collections::BTreeMap;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use self::settings::{EnvSource, HookEntry, ManagedPolicy, SettingsScope};

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
    // v0.1 (kept for backwards-compat with existing JSON consumers)
    envs: Vec<EnvCheck>,
    endpoint: EnvCheck,
    hook_settings: HookSettingsCheck,
    server: ServerProbe,
    recommendations: Vec<String>,
    exit_code: i32,
    // v0.2 — slice-7 additions
    settings_scopes: Vec<SettingsScope>,
    effective_env: BTreeMap<String, EnvSource>,
    env_divergence: Vec<EnvDivergence>,
    hook_entries: Vec<HookEntry>,
    plugin_hooks: Vec<HookEntry>,
    managed_policy: ManagedPolicy,
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

/// Attempt to load the witmcc token from `WITMCC_CONFIG_DIR/token` or
/// `~/.config/witmcc/token`. Returns `None` if the file doesn't exist or can't
/// be read (doctor stays useful even without credentials).
fn witmcc_token_from_env_or_file() -> Option<String> {
    let dir = if let Ok(v) = std::env::var("WITMCC_CONFIG_DIR") {
        std::path::PathBuf::from(v)
    } else {
        dirs::config_dir()?.join("witmcc")
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
    let token: Option<String> = witmcc_token_from_env_or_file();
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

    // Use the file-effective env (settings hierarchy) as the source of truth
    // for what `claude` will actually see, not process env. v0.1 was wrong
    // here — it printed "Set the following…" even when the user already had
    // the keys in ~/.claude/settings.json.
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
        .map(|v| v.value.trim_end_matches('/').ends_with(EXPECTED_ENDPOINT_SUFFIX))
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

    // Hooks intentionally NOT in the recommendation block.
    //
    // The substring match on `hooks/v1/events` is a heuristic that produces
    // false negatives whenever the user runs a wrapper script, pipes through
    // jq, or targets a non-default endpoint. doctor cannot tell from settings
    // alone whether a hook command will reach witmcc. Whether hook events
    // actually arrived is observable in /v1/health/sources (informational
    // table), which is the source of truth for "is data flowing".
    //
    // Managed policy is still surfaced because those flags really do block
    // hook execution regardless of user intent, but only when present.
    if report.managed_policy.disable_all_hooks {
        out.push(
            "Managed policy `disableAllHooks = true` is set — no hooks will ever fire. \
             Hook collector will receive 0 events regardless of user/project entries."
                .into(),
        );
    } else if report.managed_policy.allow_managed_hooks_only {
        out.push(
            "Managed policy `allowManagedHooksOnly = true` is set — user/project hooks are ignored. \
             Only managed / SDK / force-enabled-plugin hooks load."
                .into(),
        );
    }

    if !report.server.reachable {
        out.push(
            "witmcc server unreachable. Start it with: `witmcc serve --auto-migrate`.".into(),
        );
    } else {
        // Hook is user-configured and intentionally excluded from the
        // "no data, do X" recommendation — doctor doesn't know X.
        // Surface only sources whose absence really is actionable.
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
                "No data yet from: {}. Run a `claude` session against witmcc to populate.",
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
    // Only "actionable" sources affect exit code. hook is a user-configured
    // external (forward script) — its absence is not a failure doctor can
    // diagnose. See the matching block in build_recommendations() for the
    // same rationale.
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
        // hook is user-configured (forward script). doctor cannot diagnose
        // how it should be wired, so it surfaces as informational only — no
        // red ✗ marker that suggests action.
        const INFO_SOURCES: &[&str] = &["hook"];
        for s in &sp.sources {
            let info = INFO_SOURCES.contains(&s.label.as_str());
            let marker = if info {
                if s.status == "no_data" { dim("ℹ") } else { green("✓") }
            } else {
                match s.status {
                    "recent" => green("✓"),
                    "stale" => yellow("~"),
                    _ => red("✗"),
                }
            };
            let last = s.last_ingested_at.as_deref().unwrap_or("—");
            let suffix = if info { dim("  (info-only)") } else { String::new() };
            writeln!(
                w,
                "  {marker} {:<12} {:<28} {:>9} {:>8}{}",
                s.label, last, s.row_count_24h, s.total_rows, suffix
            )?;
        }
        writeln!(
            w,
            "      {}",
            dim("ℹ info-only = depends on external wiring; absence is not a failure")
        )?;
    } else {
        writeln!(w, "  {} unreachable", red("✗"))?;
        if let Some(e) = &sp.error {
            writeln!(w, "      {}", dim(e))?;
        }
    }
    writeln!(w)?;

    // ---- slice-7 v0.2 sections ----
    writeln!(w, "{}", dim("# Settings files probed (Claude Code hierarchy)"))?;
    for s in &report.settings_scopes {
        let marker = if s.present { green("✓") } else { yellow("∅") };
        writeln!(w, "  {marker} {:<10} {}", s.label, s.path.display())?;
        if let Some(n) = &s.note {
            writeln!(w, "      {}", dim(n))?;
        }
    }
    writeln!(w)?;

    writeln!(w, "{}", dim("# Effective OTel env (file scope = what `claude` will see)"))?;
    if report.effective_env.is_empty() {
        writeln!(w, "  {} no env block in any scope", yellow("∅"))?;
    } else {
        for (k, src) in &report.effective_env {
            writeln!(w, "  {} {:<40} = {:<24} ({})", green("✓"), k, src.value, src.scope)?;
        }
    }
    if !report.env_divergence.is_empty() {
        writeln!(w)?;
        writeln!(w, "  {}", dim("env divergence between file scope and current shell:"))?;
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

    writeln!(w, "{}", dim("# Hook forwarding to witmcc"))?;
    let mut all_hooks = report.hook_entries.clone();
    all_hooks.extend(report.plugin_hooks.iter().cloned());
    let witmcc_hooks: Vec<&HookEntry> =
        all_hooks.iter().filter(|h| h.forwards_to_witmcc).collect();
    if witmcc_hooks.is_empty() {
        writeln!(
            w,
            "  {} no hook entry forwards to /hooks/v1/events in any scope",
            yellow("∅"),
        )?;
    } else {
        for h in &witmcc_hooks {
            writeln!(w, "  {} {:<14} → {} ({})", green("✓"), h.event, h.command, h.scope)?;
        }
    }
    // surface non-witmcc hooks too if present
    let other_hooks: Vec<&HookEntry> =
        all_hooks.iter().filter(|h| !h.forwards_to_witmcc).collect();
    if !other_hooks.is_empty() {
        writeln!(w, "  {}", dim("(other hooks observed; not relevant to witmcc):"))?;
        for h in other_hooks.iter().take(5) {
            writeln!(w, "    {:<14} {} ({})", h.event, h.command, h.scope)?;
        }
        if other_hooks.len() > 5 {
            writeln!(w, "    ... +{} more", other_hooks.len() - 5)?;
        }
    }
    writeln!(w)?;

    if report.managed_policy.allow_managed_hooks_only
        || report.managed_policy.disable_all_hooks
    {
        writeln!(w, "{}", dim("# Managed policy (may silence user/project hooks)"))?;
        if report.managed_policy.disable_all_hooks {
            writeln!(w, "  {} disableAllHooks = true — no hooks will fire", red("✗"))?;
        }
        if report.managed_policy.allow_managed_hooks_only {
            writeln!(
                w,
                "  {} allowManagedHooksOnly = true — only managed/SDK/force-enabled plugin hooks load",
                yellow("!"),
            )?;
        }
        writeln!(w)?;
    }

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

    // ---- slice-7 v0.2: settings hierarchy walk ----
    let project_start = opts
        .project
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .unwrap_or_else(|| PathBuf::from("."));
    let settings_scopes = settings::scopes(&project_start);
    let effective_env_map = settings::effective_env(&settings_scopes);
    let plugins_root_override = std::env::var_os("WITMCC_DOCTOR_PLUGINS_ROOT").map(PathBuf::from);
    let plugins_root = plugins_root_override
        .or_else(settings::default_plugins_root)
        .unwrap_or_else(|| PathBuf::from("/nonexistent"));
    let all_hooks = settings::hook_entries(&settings_scopes, &plugins_root);
    let (plugin_hooks, hook_entries): (Vec<HookEntry>, Vec<HookEntry>) =
        all_hooks.into_iter().partition(|h| h.scope.starts_with("plugin:"));
    let managed_policy = settings::managed_policy(&settings_scopes);

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
        hook_settings,
        server,
        recommendations: vec![],
        exit_code: 0,
        settings_scopes,
        effective_env: effective_env_map,
        env_divergence,
        hook_entries,
        plugin_hooks,
        managed_policy,
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
