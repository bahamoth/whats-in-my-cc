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
        });
    }
    out
}
