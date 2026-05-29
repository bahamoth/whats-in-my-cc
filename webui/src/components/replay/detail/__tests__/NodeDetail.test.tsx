import { render, screen } from '@testing-library/react';
import { describe, it, expect } from 'vitest';
import { NodeDetail } from '../NodeDetail';

const node = (kind: string, payload: unknown, id = 'nd_1') => ({ node_id: id, schema_version: '1', session_id: 's', node_kind: kind, started_at: '2026-05-15T05:25:39Z', ended_at: null, merge_keys: {}, source_event_ids: [], source_uris: [], payload }) as any;

describe('NodeDetail', () => {
  it('tool_call: shows tool name + parameters + result', () => {
    render(<NodeDetail node={node('tool_call', { tool_name: 'Bash', input: { command: 'rm -f x', description: '정리' } })}
      record={{ tool_result: { is_error: false } }} episodePhase="action" findings={[]} />);
    expect(screen.getByText('Bash')).toBeInTheDocument();
    expect(screen.getByText('rm -f x')).toBeInTheDocument();
    expect(screen.getByText('정리')).toBeInTheDocument();
    expect(screen.getByText(/ok/i)).toBeInTheDocument();
  });
  it('assistant_message: shows full text + token usage from record', () => {
    render(<NodeDetail node={node('assistant_message', { model: 'claude-opus-4-8', text: '전체 답변' })}
      record={{ message: { usage: { output_tokens: 451, input_tokens: 3 } } }} episodePhase={null} findings={[]} />);
    expect(screen.getByText('전체 답변')).toBeInTheDocument();
    expect(screen.getByText(/451/)).toBeInTheDocument();
  });
  it('renders findings for the node', () => {
    render(<NodeDetail node={node('tool_call', { tool_name: 'Read', input: {} })} record={null} episodePhase={null}
      findings={[{ finding_id: 'f1', severity: 'medium', category: 'missing_verification', confidence: 0.8, summary: '검증 없음' } as any]} />);
    expect(screen.getByText('missing_verification')).toBeInTheDocument();
    expect(screen.getByText(/검증 없음/)).toBeInTheDocument();
  });
});
