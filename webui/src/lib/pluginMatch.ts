import type { PluginDto } from '../api/types';
import { mcpServerOf } from '../components/replay/stream/nodeLabel';

/** Resolve which installed plugin owns the MCP server a tool call belongs to.
 *  Uses the registry's `mcp_servers` mapping (from `claude plugins list --json`),
 *  so no `plugin_<plugin>_<server>` name-splitting is needed. null when the tool
 *  is not an MCP tool, or its server isn't provided by any known plugin (e.g. a
 *  directly-configured server — "configured", not a marketplace plugin). */
export function findPluginForTool(
  registry: PluginDto[],
  toolName: string | null | undefined,
): PluginDto | null {
  if (!toolName) return null;
  const server = mcpServerOf(toolName);
  if (!server) return null;
  return registry.find((p) => p.mcp_servers.includes(server)) ?? null;
}
