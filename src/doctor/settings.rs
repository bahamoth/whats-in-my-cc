//! Slice-7 doctor v0.2 — Claude Code settings hierarchy walk.
//!
//! Per https://code.claude.com/docs/en/settings the active configuration
//! merges in this order (highest → lowest precedence):
//!
//!   1. Managed       — `/Library/Application Support/ClaudeCode/managed-settings.json`
//!      + `managed-settings.d/*.json` (alphabetical merge)
//!   2. CLI flags     — not visible from outside Claude Code; doctor doesn't see them.
//!   3. Local         — `<project>/.claude/settings.local.json`
//!   4. Project       — `<project>/.claude/settings.json`
//!   5. User          — `~/.claude/settings.json`
//!
//! Both `env` and `hooks` may live in any of these scopes. Plugin manifests
//! under `~/.claude/plugins/<plugin>/{plugin.json,manifest.json}` can also
//! contribute hook entries.
//!
//! All operations here are read-only. Doctor never writes to settings files.

use serde::Serialize;
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ScopeKind {
    Managed,
    User,
    ProjectShared,
    ProjectLocal,
    Plugin,
}

impl ScopeKind {
    pub fn precedence(&self) -> u8 {
        // higher wins
        match self {
            ScopeKind::Managed => 4,
            ScopeKind::ProjectLocal => 3,
            ScopeKind::ProjectShared => 2,
            ScopeKind::User => 1,
            ScopeKind::Plugin => 0, // hooks-only contributor; env is not picked from plugins
        }
    }
    pub fn label(&self) -> &'static str {
        match self {
            ScopeKind::Managed => "managed",
            ScopeKind::User => "user",
            ScopeKind::ProjectShared => "project",
            ScopeKind::ProjectLocal => "local",
            ScopeKind::Plugin => "plugin",
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct SettingsScope {
    pub kind: ScopeKind,
    pub label: String, // e.g. "project" or "plugin:my-plugin"
    pub path: PathBuf,
    pub present: bool,
    #[serde(skip)]
    pub parsed: Option<Value>,
    /// Soft warning (e.g. "managed file present but JSON parse error: ...").
    pub note: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct EnvSource {
    pub value: String,
    pub scope: String, // ScopeKind::label() of winner
}

#[derive(Debug, Clone, Serialize)]
pub struct HookEntry {
    pub event: String,
    pub command: String,
    pub scope: String,           // scope label
    pub forwards_to_wimcc: bool, // command contains "hooks/v1/events"
}

#[derive(Debug, Clone, Default, Serialize)]
pub struct ManagedPolicy {
    pub allow_managed_hooks_only: bool,
    pub disable_all_hooks: bool,
    pub allowed_http_hook_urls: Vec<String>,
}

// -- Scope discovery --------------------------------------------------------

#[cfg(target_os = "macos")]
fn managed_root() -> PathBuf {
    PathBuf::from("/Library/Application Support/ClaudeCode")
}
#[cfg(target_os = "linux")]
fn managed_root() -> PathBuf {
    PathBuf::from("/etc/claude-code")
}
#[cfg(target_os = "windows")]
fn managed_root() -> PathBuf {
    PathBuf::from(r"C:\Program Files\ClaudeCode")
}
#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn managed_root() -> PathBuf {
    PathBuf::from("/etc/claude-code")
}

fn managed_paths() -> Vec<PathBuf> {
    let base = managed_root();
    let mut out = vec![base.join("managed-settings.json")];
    let drop_in = base.join("managed-settings.d");
    if let Ok(entries) = std::fs::read_dir(&drop_in) {
        let mut paths: Vec<PathBuf> = entries
            .flatten()
            .map(|e| e.path())
            .filter(|p| p.extension().and_then(|x| x.to_str()) == Some("json"))
            .collect();
        paths.sort();
        out.extend(paths);
    }
    out
}

fn user_path() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("settings.json"))
}

/// Walk upward from `start` looking for a `.claude` directory. Stops at the
/// filesystem root or when a directory contains `.git` (the project root).
/// Returns (shared, local) paths if a project was located.
fn project_paths(start: &Path) -> Option<(PathBuf, PathBuf)> {
    let mut cur = if start.is_absolute() {
        start.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(start)
    };
    loop {
        let claude_dir = cur.join(".claude");
        if claude_dir.is_dir() {
            return Some((
                claude_dir.join("settings.json"),
                claude_dir.join("settings.local.json"),
            ));
        }
        if cur.join(".git").exists() {
            // project root without .claude/ — record the path so doctor reports
            // both files as absent at the right place.
            let claude_dir = cur.join(".claude");
            return Some((
                claude_dir.join("settings.json"),
                claude_dir.join("settings.local.json"),
            ));
        }
        match cur.parent() {
            Some(p) if p != cur => cur = p.to_path_buf(),
            _ => return None,
        }
    }
}

fn read_scope(kind: ScopeKind, path: PathBuf, scope_label: Option<String>) -> SettingsScope {
    let label = scope_label.unwrap_or_else(|| kind.label().to_string());
    let (present, parsed, note) = match std::fs::read_to_string(&path) {
        Ok(s) => match serde_json::from_str::<Value>(&s) {
            Ok(v) => (true, Some(v), None),
            Err(e) => (true, None, Some(format!("JSON parse error: {e}"))),
        },
        Err(_) => (false, None, None),
    };
    SettingsScope {
        kind,
        label,
        path,
        present,
        parsed,
        note,
    }
}

/// Return all settings scopes in precedence order (lowest first so callers
/// can fold left-to-right with higher overriding lower).
pub fn scopes(project_start: &Path) -> Vec<SettingsScope> {
    let mut out: Vec<SettingsScope> = Vec::new();
    // user
    if let Some(p) = user_path() {
        out.push(read_scope(ScopeKind::User, p, None));
    }
    // project shared + local
    if let Some((shared, local)) = project_paths(project_start) {
        out.push(read_scope(ScopeKind::ProjectShared, shared, None));
        out.push(read_scope(ScopeKind::ProjectLocal, local, None));
    }
    // managed (last to be appended, highest precedence — for drop-ins we
    // generate one SettingsScope per file so attribution stays precise).
    for p in managed_paths() {
        let label = if p.file_name().and_then(|x| x.to_str()) == Some("managed-settings.json") {
            "managed".to_string()
        } else {
            format!(
                "managed:{}",
                p.file_name().and_then(|x| x.to_str()).unwrap_or("?")
            )
        };
        out.push(read_scope(ScopeKind::Managed, p, Some(label)));
    }
    out
}

// -- Env merge -------------------------------------------------------------

/// Merge `env` blocks across scopes following precedence rules
/// (higher precedence overwrites lower).
pub fn effective_env(scopes: &[SettingsScope]) -> BTreeMap<String, EnvSource> {
    let mut out: BTreeMap<String, EnvSource> = BTreeMap::new();
    let mut ranked: Vec<&SettingsScope> = scopes.iter().collect();
    ranked.sort_by_key(|s| s.kind.precedence()); // ascending
    for s in ranked {
        let parsed = match &s.parsed {
            Some(v) => v,
            None => continue,
        };
        let env = match parsed.get("env").and_then(|v| v.as_object()) {
            Some(m) => m,
            None => continue,
        };
        for (k, v) in env {
            let value = match v {
                Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            out.insert(
                k.clone(),
                EnvSource {
                    value,
                    scope: s.label.clone(),
                },
            );
        }
    }
    out
}

// -- Hook discovery --------------------------------------------------------

const WIMCC_FORWARD_SUBSTR: &str = "hooks/v1/events";

fn collect_hooks_into(scope: &SettingsScope, out: &mut Vec<HookEntry>) {
    let parsed = match &scope.parsed {
        Some(v) => v,
        None => return,
    };
    let hooks = match parsed.get("hooks").and_then(|v| v.as_object()) {
        Some(m) => m,
        None => return,
    };
    for (event_name, entries) in hooks {
        let arr = match entries.as_array() {
            Some(a) => a,
            None => continue,
        };
        for entry in arr {
            // settings.json shape: hooks.<Event>[].hooks[].command  OR
            // settings.json shape: hooks.<Event>[].command (older docs variant)
            let inner_candidates: Vec<&Value> =
                if let Some(inner) = entry.get("hooks").and_then(|v| v.as_array()) {
                    inner.iter().collect()
                } else {
                    vec![entry]
                };
            for h in inner_candidates {
                let cmd = h
                    .get("command")
                    .and_then(|v| v.as_str())
                    .or_else(|| h.get("url").and_then(|v| v.as_str())) // HTTP variant
                    .unwrap_or("");
                if cmd.is_empty() {
                    continue;
                }
                out.push(HookEntry {
                    event: event_name.clone(),
                    command: cmd.to_string(),
                    scope: scope.label.clone(),
                    forwards_to_wimcc: cmd.contains(WIMCC_FORWARD_SUBSTR),
                });
            }
        }
    }
}

fn plugin_manifest_paths(plugins_root: &Path) -> Vec<(String, PathBuf)> {
    let mut out = Vec::new();
    let entries = match std::fs::read_dir(plugins_root) {
        Ok(e) => e,
        Err(_) => return out,
    };
    for e in entries.flatten() {
        let p = e.path();
        if !p.is_dir() {
            continue;
        }
        let name = p
            .file_name()
            .and_then(|x| x.to_str())
            .unwrap_or("?")
            .to_string();
        for candidate in ["plugin.json", "manifest.json", "hooks.json"] {
            let mp = p.join(candidate);
            if mp.is_file() {
                out.push((name.clone(), mp));
                break;
            }
        }
    }
    out
}

/// Gather all hook entries from settings scopes plus plugin manifests.
pub fn hook_entries(scopes: &[SettingsScope], plugins_root: &Path) -> Vec<HookEntry> {
    let mut out = Vec::new();
    for s in scopes {
        collect_hooks_into(s, &mut out);
    }
    for (name, path) in plugin_manifest_paths(plugins_root) {
        let plugin_scope = read_scope(ScopeKind::Plugin, path, Some(format!("plugin:{name}")));
        collect_hooks_into(&plugin_scope, &mut out);
    }
    out
}

// -- Managed policy --------------------------------------------------------

pub fn managed_policy(scopes: &[SettingsScope]) -> ManagedPolicy {
    let mut policy = ManagedPolicy::default();
    for s in scopes {
        if s.kind != ScopeKind::Managed {
            continue;
        }
        let parsed = match &s.parsed {
            Some(v) => v,
            None => continue,
        };
        if let Some(b) = parsed
            .get("allowManagedHooksOnly")
            .and_then(|v| v.as_bool())
        {
            policy.allow_managed_hooks_only = policy.allow_managed_hooks_only || b;
        }
        if let Some(b) = parsed.get("disableAllHooks").and_then(|v| v.as_bool()) {
            policy.disable_all_hooks = policy.disable_all_hooks || b;
        }
        if let Some(arr) = parsed.get("allowedHttpHookUrls").and_then(|v| v.as_array()) {
            for u in arr {
                if let Some(s) = u.as_str() {
                    policy.allowed_http_hook_urls.push(s.to_string());
                }
            }
        }
    }
    policy
}

pub fn default_plugins_root() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("plugins"))
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;
    use std::io::Write;

    fn write(path: &Path, body: &Value) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        let mut f = std::fs::File::create(path).unwrap();
        write!(f, "{}", serde_json::to_string_pretty(body).unwrap()).unwrap();
    }

    #[test]
    fn project_walk_finds_dot_claude_settings() {
        let tmp = tempfile::tempdir().unwrap();
        let claude = tmp.path().join("a").join("b").join(".claude");
        std::fs::create_dir_all(&claude).unwrap();
        write(&claude.join("settings.json"), &json!({"env":{"X":"1"}}));
        let start = tmp.path().join("a").join("b").join("c");
        std::fs::create_dir_all(&start).unwrap();
        let (shared, local) = project_paths(&start).unwrap();
        assert_eq!(shared, claude.join("settings.json"));
        assert_eq!(local, claude.join("settings.local.json"));
    }

    #[test]
    fn effective_env_local_overrides_project_overrides_user() {
        let tmp = tempfile::tempdir().unwrap();
        let user_path = tmp.path().join("user.json");
        write(&user_path, &json!({"env":{"X":"user","Y":"user-only"}}));
        let proj_path = tmp.path().join("proj.json");
        write(&proj_path, &json!({"env":{"X":"project"}}));
        let local_path = tmp.path().join("local.json");
        write(&local_path, &json!({"env":{"X":"local","Z":"local-only"}}));

        let scopes = vec![
            read_scope(ScopeKind::User, user_path, None),
            read_scope(ScopeKind::ProjectShared, proj_path, None),
            read_scope(ScopeKind::ProjectLocal, local_path, None),
        ];
        let env = effective_env(&scopes);
        assert_eq!(env.get("X").unwrap().value, "local");
        assert_eq!(env.get("X").unwrap().scope, "local");
        assert_eq!(env.get("Y").unwrap().value, "user-only");
        assert_eq!(env.get("Y").unwrap().scope, "user");
        assert_eq!(env.get("Z").unwrap().value, "local-only");
    }

    #[test]
    fn hook_entries_match_wimcc_substring_and_attribute_scope() {
        let tmp = tempfile::tempdir().unwrap();
        let user_path = tmp.path().join("user.json");
        write(
            &user_path,
            &json!({"hooks":{"PostToolUse":[{"hooks":[{"type":"command","command":"/usr/local/bin/wimcc-forward.sh /hooks/v1/events"}]}]}}),
        );
        let scopes = vec![read_scope(ScopeKind::User, user_path, None)];
        let plugins_root = tmp.path().join("nonexistent-plugins");
        let hooks = hook_entries(&scopes, &plugins_root);
        assert_eq!(hooks.len(), 1);
        assert!(hooks[0].forwards_to_wimcc);
        assert_eq!(hooks[0].event, "PostToolUse");
        assert_eq!(hooks[0].scope, "user");
    }

    #[test]
    fn plugin_manifest_hooks_picked_up() {
        let tmp = tempfile::tempdir().unwrap();
        let plugin_dir = tmp.path().join("my-plugin");
        std::fs::create_dir_all(&plugin_dir).unwrap();
        write(
            &plugin_dir.join("plugin.json"),
            &json!({"hooks":{"PreToolUse":[{"hooks":[{"command":"curl …/hooks/v1/events"}]}]}}),
        );
        let hooks = hook_entries(&[], tmp.path());
        assert_eq!(hooks.len(), 1);
        assert_eq!(hooks[0].scope, "plugin:my-plugin");
        assert!(hooks[0].forwards_to_wimcc);
    }

    #[test]
    fn managed_policy_or_aggregates_flags() {
        let tmp = tempfile::tempdir().unwrap();
        let p1 = tmp.path().join("m1.json");
        write(&p1, &json!({"allowManagedHooksOnly": true}));
        let p2 = tmp.path().join("m2.json");
        write(
            &p2,
            &json!({"allowedHttpHookUrls": ["https://allow.example/*"]}),
        );
        let scopes = vec![
            read_scope(ScopeKind::Managed, p1, Some("managed".into())),
            read_scope(ScopeKind::Managed, p2, Some("managed:m2".into())),
        ];
        let pol = managed_policy(&scopes);
        assert!(pol.allow_managed_hooks_only);
        assert!(!pol.disable_all_hooks);
        assert_eq!(pol.allowed_http_hook_urls.len(), 1);
    }
}
