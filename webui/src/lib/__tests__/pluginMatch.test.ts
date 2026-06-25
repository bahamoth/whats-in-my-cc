import { describe, expect, it } from 'vitest';
import { findPluginForTool } from '../pluginMatch';
import type { PluginDto } from '../../api/types';

const registry: PluginDto[] = [
  {
    id: 'serena@claude-plugins-official',
    plugin: 'serena',
    marketplace: 'claude-plugins-official',
    provenance: 'official',
    scope: 'user',
    enabled: true,
    mcp_servers: ['serena'],
    description: 'Semantic code analysis MCP server…',
  },
  {
    id: 'dev-tools@cc-marketplace',
    plugin: 'dev-tools',
    marketplace: 'cc-marketplace',
    provenance: 'public',
    scope: 'user',
    enabled: true,
    mcp_servers: [],
    description: null,
  },
];

describe('findPluginForTool', () => {
  it('matches a plugin MCP tool to its owning plugin via mcp_servers', () => {
    const hit = findPluginForTool(registry, 'mcp__plugin_serena_serena__get_symbols_overview');
    expect(hit?.id).toBe('serena@claude-plugins-official');
    expect(hit?.provenance).toBe('official');
  });

  it('returns null for non-MCP tools and unconfigured/directly-configured servers', () => {
    expect(findPluginForTool(registry, 'Read')).toBeNull();
    // a directly-configured (non-plugin) server is not in any plugin's mcp_servers
    expect(findPluginForTool(registry, 'mcp__optiflow-help__search')).toBeNull();
    expect(findPluginForTool(registry, null)).toBeNull();
  });
});
