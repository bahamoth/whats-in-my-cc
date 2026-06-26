// MCP plugin reference card (Insight tab "②.5 reference"). For a selected MCP
// tool call, resolve which marketplace plugin owns its server (via /v1/plugins)
// and show provenance + description. A server with no owning plugin is a
// directly-configured ("personal") MCP server — shown as such, never tagged.
import { usePluginsQuery } from '../../../lib/queries';
import { findPluginForTool } from '../../../lib/pluginMatch';
import { mcpServerOf, mcpOfficialIntegration } from '../stream/nodeLabel';
import { useT } from '../../../i18n';
import styles from './McpPluginCard.module.css';

export function McpPluginCard({ toolName }: { toolName: string | null }) {
  const t = useT();
  const plugins = usePluginsQuery();
  const server = mcpServerOf(toolName ?? '');
  if (!server) return null; // not an MCP tool

  const plugin = findPluginForTool(plugins.data ?? [], toolName);
  if (!plugin) {
    // Not a marketplace plugin. Distinguish an official Anthropic integration
    // (claude.ai connector / Chrome extension — managed, known semantics, tagged)
    // from a genuinely one-off, directly-configured server.
    const connector = mcpOfficialIntegration(toolName ?? '');
    return (
      <div className={styles.card}>
        <div className={styles.head}>
          <span className={styles.badge} data-prov={connector ? 'connector' : 'configured'}>
            {connector ? 'connector' : 'configured'}
          </span>
          <span className={styles.id}>{server}</span>
        </div>
        <p className={styles.desc}>
          {t(connector ? 'detail.plugin.connector' : 'detail.plugin.configured')}
        </p>
      </div>
    );
  }

  return (
    <div className={styles.card}>
      <div className={styles.head}>
        <span className={styles.badge} data-prov={plugin.provenance}>
          {plugin.provenance}
        </span>
        <span className={styles.id}>{plugin.id}</span>
        <span className={styles.scope}>{plugin.scope}</span>
      </div>
      {plugin.description && <p className={styles.desc}>{plugin.description}</p>}
    </div>
  );
}
