//! Plugin registry — resolve marketplace-installed plugins (and their MCP
//! servers) from the `claude` CLI's JSON output. Provenance is derived from the
//! marketplace source. Real-data anchored: fixtures are frozen verbatim from
//! `claude plugins list --json` + `claude plugins marketplace list --json`.

use serde_json::Value;
use wimcc::plugins::{build_registry, provenance_of, Provenance};

fn load(p: &str) -> Value {
    serde_json::from_str(&std::fs::read_to_string(p).unwrap()).unwrap()
}
fn list() -> Value {
    load("tests/fixtures/plugins/real/plugins_list.json")
}
fn marketplaces() -> Value {
    load("tests/fixtures/plugins/real/marketplace_list.json")
}

#[test]
fn provenance_from_marketplace_source() {
    // CC labeling: github anthropics/* = official, other github = public,
    // directory (local path) = personal.
    assert_eq!(
        provenance_of("github", Some("anthropics/claude-plugins-official")),
        Provenance::Official
    );
    assert_eq!(
        provenance_of("github", Some("bahamoth/claude-marketplace")),
        Provenance::Public
    );
    assert_eq!(provenance_of("directory", None), Provenance::Personal);
    assert_eq!(provenance_of("mystery", None), Provenance::Unknown);
}

#[test]
fn registry_resolves_id_scope_provenance_and_mcp_servers() {
    let reg = build_registry(&list(), &marketplaces());

    let serena = reg.iter().find(|e| e.plugin == "serena").expect("serena");
    assert_eq!(serena.id, "serena@claude-plugins-official");
    assert_eq!(serena.marketplace, "claude-plugins-official");
    assert_eq!(serena.provenance, Provenance::Official);
    assert_eq!(serena.scope, "user");
    assert!(serena.enabled);
    assert_eq!(serena.mcp_servers, vec!["serena".to_string()]);

    let public = reg
        .iter()
        .find(|e| e.marketplace == "cc-marketplace")
        .expect("a cc-marketplace plugin");
    assert_eq!(public.provenance, Provenance::Public);

    let personal = reg
        .iter()
        .find(|e| e.marketplace == "mcc-llm-wiki-dev")
        .expect("a directory-marketplace plugin");
    assert_eq!(personal.provenance, Provenance::Personal);
    assert_eq!(personal.scope, "local");
}

#[test]
fn mcp_server_name_resolves_to_its_plugin() {
    // A tool call mcp__plugin_serena_serena__X → server "serena" → serena plugin.
    // The CLI's mcpServers field gives this mapping directly (no name-splitting).
    let reg = build_registry(&list(), &marketplaces());
    let hit = reg
        .iter()
        .find(|e| e.mcp_servers.iter().any(|s| s == "serena"))
        .expect("serena server owner");
    assert_eq!(hit.id, "serena@claude-plugins-official");
    assert_eq!(hit.provenance.as_str(), "official");
}
