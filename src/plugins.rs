//! Plugin registry — resolve marketplace-installed plugins and the MCP servers
//! they provide, using the `claude` CLI as the authoritative source:
//!   - `claude plugins list --json`            → id, scope, enabled, mcpServers, installPath
//!   - `claude plugins marketplace list --json` → marketplace → source/repo (provenance)
//!
//! Why the CLI and not the internal `~/.claude/plugins/*.json` caches: the CLI is
//! the supported interface, its `mcpServers` field gives the plugin↔server mapping
//! directly (no `plugin_<plugin>_<server>` underscore-splitting guesswork), and we
//! avoid reading the giant `~/.claude.json`. Pure parsing lives in `build_registry`
//! (unit-tested against frozen real CLI output); the subprocess + cache is wiring.
//!
//! This is read-only local observation: the `claude plugins list` subcommands do
//! not mutate anything.

use serde_json::Value;

/// Where a plugin came from, per Claude Code's marketplace labeling.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provenance {
    /// github marketplace under `anthropics/*`.
    Official,
    /// github marketplace, third-party org.
    Public,
    /// `directory`-source marketplace (developed locally) — "personal".
    Personal,
    /// marketplace not found / unrecognised source.
    Unknown,
}

impl Provenance {
    pub fn as_str(&self) -> &'static str {
        match self {
            Provenance::Official => "official",
            Provenance::Public => "public",
            Provenance::Personal => "personal",
            Provenance::Unknown => "unknown",
        }
    }
}

/// Classify a marketplace by its `claude plugins marketplace list --json` source.
pub fn provenance_of(source: &str, repo: Option<&str>) -> Provenance {
    match source {
        "directory" => Provenance::Personal,
        "github" => match repo {
            Some(r) if r.starts_with("anthropics/") => Provenance::Official,
            Some(_) => Provenance::Public,
            None => Provenance::Unknown,
        },
        _ => Provenance::Unknown,
    }
}

/// One installed plugin, joined with its marketplace provenance.
#[derive(Debug, Clone)]
pub struct PluginEntry {
    /// `plugin@marketplace` (Claude Code's canonical id).
    pub id: String,
    pub plugin: String,
    pub marketplace: String,
    pub provenance: Provenance,
    /// `user` | `local` | `project`.
    pub scope: String,
    pub enabled: bool,
    /// MCP server names this plugin provides (the `mcpServers` keys). A tool call
    /// `mcp__plugin_<plugin>_<server>__<tool>` resolves to the plugin owning `<server>`.
    pub mcp_servers: Vec<String>,
    pub install_path: Option<String>,
    /// From `<installPath>/.claude-plugin/plugin.json` — filled by the loader, not
    /// `build_registry` (which is pure / filesystem-free). None until enriched.
    pub description: Option<String>,
}

/// Build the registry by joining `claude plugins list --json` with
/// `claude plugins marketplace list --json`.
pub fn build_registry(list: &Value, marketplaces: &Value) -> Vec<PluginEntry> {
    // marketplace name → (source, repo) for provenance.
    let mut mk: std::collections::HashMap<&str, (&str, Option<&str>)> =
        std::collections::HashMap::new();
    if let Some(arr) = marketplaces.as_array() {
        for m in arr {
            let Some(name) = m.get("name").and_then(Value::as_str) else {
                continue;
            };
            let source = m.get("source").and_then(Value::as_str).unwrap_or("");
            let repo = m.get("repo").and_then(Value::as_str);
            mk.insert(name, (source, repo));
        }
    }

    let mut out = Vec::new();
    let Some(arr) = list.as_array() else {
        return out;
    };
    for it in arr {
        let id = it.get("id").and_then(Value::as_str).unwrap_or("");
        if id.is_empty() {
            continue;
        }
        let (plugin, marketplace) = match id.rsplit_once('@') {
            Some((p, m)) => (p.to_string(), m.to_string()),
            None => (id.to_string(), String::new()),
        };
        let provenance = mk
            .get(marketplace.as_str())
            .map(|(s, r)| provenance_of(s, *r))
            .unwrap_or(Provenance::Unknown);
        let scope = it
            .get("scope")
            .and_then(Value::as_str)
            .unwrap_or("")
            .to_string();
        let enabled = it.get("enabled").and_then(Value::as_bool).unwrap_or(false);
        let mcp_servers = it
            .get("mcpServers")
            .and_then(Value::as_object)
            .map(|o| o.keys().cloned().collect())
            .unwrap_or_default();
        let install_path = it
            .get("installPath")
            .and_then(Value::as_str)
            .map(str::to_string);
        out.push(PluginEntry {
            id: id.to_string(),
            plugin,
            marketplace,
            provenance,
            scope,
            enabled,
            mcp_servers,
            install_path,
            description: None,
        });
    }
    out
}

/// Read a plugin's description from `<install_path>/.claude-plugin/plugin.json`.
/// Small per-plugin file (not the catalog). None if absent/unreadable.
pub fn read_plugin_description(install_path: &str) -> Option<String> {
    let path = std::path::Path::new(install_path)
        .join(".claude-plugin")
        .join("plugin.json");
    let txt = std::fs::read_to_string(path).ok()?;
    let v: Value = serde_json::from_str(&txt).ok()?;
    v.get("description")
        .and_then(Value::as_str)
        .map(str::to_string)
}

/// Run `claude <args> --json` and parse stdout. None on any failure (binary
/// missing, non-zero exit, bad JSON) — the registry degrades to empty, never panics.
fn run_claude_json(args: &[&str]) -> Option<Value> {
    let out = std::process::Command::new("claude")
        .args(args)
        .output()
        .ok()?;
    if !out.status.success() {
        return None;
    }
    serde_json::from_slice(&out.stdout).ok()
}

/// Load the full registry from the `claude` CLI, enriching each entry with its
/// description. Best-effort: a missing/failing CLI yields an empty registry.
pub fn load_registry() -> Vec<PluginEntry> {
    let list = run_claude_json(&["plugins", "list", "--json"]).unwrap_or(Value::Array(vec![]));
    let markets = run_claude_json(&["plugins", "marketplace", "list", "--json"])
        .unwrap_or(Value::Array(vec![]));
    let mut reg = build_registry(&list, &markets);
    for e in &mut reg {
        if let Some(ip) = e.install_path.as_deref() {
            e.description = read_plugin_description(ip);
        }
    }
    reg
}

/// Process-wide cached registry — loaded once (the `claude` subprocess runs on
/// first access only). Plugins rarely change mid-serve; a restart refreshes.
pub fn cached_registry() -> &'static [PluginEntry] {
    static CACHE: std::sync::OnceLock<Vec<PluginEntry>> = std::sync::OnceLock::new();
    CACHE.get_or_init(load_registry)
}
