// webui/src/components/replay/stream/__tests__/streamModel.test.ts
/**
 * R2 RED — buildStreamCards maps normalized ObservedEventDto[] into the chat
 * view model. Shapes anchored to the live API (plan R2 real-data anchor).
 */
import { describe, expect, it } from 'vitest';
import { buildStreamCards } from '../streamModel';
import type { ObservedEventDto } from '../../../../api/types';

function ev(partial: Partial<ObservedEventDto>): ObservedEventDto {
  return {
    event_id: 'e', raw_event_id: '', session_id: 's', event_uuid: null,
    parent_uuid: null, observed_at: '2026-05-28T00:00:00Z', actor: 'user',
    kind: 'user_message', subkind: null, tool_use_id: null, tool_name: null,
    turn_id: null, is_sidechain: false, is_meta: false, payload: {},
    ...partial,
  };
}

describe('buildStreamCards', () => {
  it('keeps only conversation kinds, dropping session_state/hook/otel/etc', () => {
    const cards = buildStreamCards([
      ev({ event_id: 'u', kind: 'user_message', actor: 'user', payload: { content: 'hi' } }),
      ev({ event_id: 'noise', kind: 'session_state', actor: 'system', payload: { permissionMode: 'x' } }),
      ev({ event_id: 'm', kind: 'metric_sample', actor: 'system', payload: {} }),
    ]);
    expect(cards.map((c) => c.id)).toEqual(['u']);
  });

  it('maps user_message to a user card with the prompt text', () => {
    const [c] = buildStreamCards([ev({ event_id: 'u', kind: 'user_message', actor: 'user', payload: { content: 'fix build.rs' } })]);
    expect(c.kind).toBe('user');
    expect(c.preview).toBe('fix build.rs');
  });

  it('maps assistant_message text and thinking to their own cards', () => {
    const cards = buildStreamCards([
      ev({ event_id: 'a', kind: 'assistant_message', actor: 'assistant', payload: { content_ordinal: 0, text: 'on it' } }),
      ev({ event_id: 't', kind: 'thinking', actor: 'assistant', payload: { content_ordinal: 0, thinking: 'reasoning…', signature: 'x' } }),
    ]);
    expect(cards.find((c) => c.id === 'a')?.kind).toBe('assistant');
    expect(cards.find((c) => c.id === 'a')?.preview).toBe('on it');
    expect(cards.find((c) => c.id === 't')?.kind).toBe('thinking');
    expect(cards.find((c) => c.id === 't')?.preview).toBe('reasoning…');
  });

  it('represents empty/redacted thinking with a placeholder preview', () => {
    const [c] = buildStreamCards([ev({ event_id: 't', kind: 'thinking', actor: 'assistant', payload: { content_ordinal: 0, thinking: '', signature: 'x' } })]);
    expect(c.kind).toBe('thinking');
    expect(c.preview).toMatch(/redacted|hidden/i);
  });

  it('merges tool_result into its tool_call by tool_use_id (no standalone result card)', () => {
    const cards = buildStreamCards([
      ev({ event_id: 'tc', kind: 'tool_call', actor: 'assistant', tool_name: 'Bash', tool_use_id: 'tu1', payload: { input: { command: 'cargo test', description: 'run', timeout: 60 } } }),
      ev({ event_id: 'tr', kind: 'tool_result', actor: 'system', tool_use_id: 'tu1', payload: { tool_result: { type: 'tool_result', tool_use_id: 'tu1', content: 'ok' } } }),
    ]);
    expect(cards).toHaveLength(1);
    const c = cards[0];
    expect(c.kind).toBe('tool');
    expect(c.id).toBe('tc');
    expect(c.tool?.toolName).toBe('Bash');
    expect(c.tool?.inputSummary).toBe('cargo test');
    expect(c.tool?.result?.isError).toBe(false);
  });

  it('flags is_error on the merged tool result', () => {
    const cards = buildStreamCards([
      ev({ event_id: 'tc', kind: 'tool_call', actor: 'assistant', tool_name: 'Edit', tool_use_id: 'tu2', payload: { input: { file_path: 'src/graph/build.rs' } } }),
      ev({ event_id: 'tr', kind: 'tool_result', actor: 'system', tool_use_id: 'tu2', payload: { tool_result: { type: 'tool_result', tool_use_id: 'tu2', content: 'boom', is_error: true } } }),
    ]);
    expect(cards[0].tool?.inputSummary).toBe('src/graph/build.rs');
    expect(cards[0].tool?.result?.isError).toBe(true);
  });

  it('keeps a tool_call with no matching result (result null)', () => {
    const cards = buildStreamCards([
      ev({ event_id: 'tc', kind: 'tool_call', actor: 'assistant', tool_name: 'Read', tool_use_id: 'tu3', payload: { input: { file_path: 'a.ts' } } }),
    ]);
    expect(cards).toHaveLength(1);
    expect(cards[0].tool?.result == null).toBe(true);
  });
});
