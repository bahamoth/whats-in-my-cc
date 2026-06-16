import { screen } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import { describe, expect, test } from 'vitest';
import { WhatSection } from '../WhatSection';

function ev(kind: string, payload: unknown = {}) {
  return { kind, payload, event_id: 'e1', session_id: 's1' } as any;
}

describe('WhatSection', () => {
  test('tool_call shows command and matched result output', () => {
    const event = ev('tool_call', { tool_name: 'Bash', input: { command: 'cargo test' } });
    const result = { payload: { tool_result: { content: 'test result: ok. 142 passed', is_error: false } } } as any;
    render(<WhatSection event={event} matchedResult={result} />);
    expect(screen.getByText(/cargo test/)).toBeInTheDocument();
    expect(screen.getByText(/142 passed/)).toBeInTheDocument();
  });

  test('user_message shows full prompt', () => {
    const event = ev('user_message', { content: '전체 프롬프트 본문' });
    render(<WhatSection event={event} matchedResult={null} />);
    expect(screen.getByText(/전체 프롬프트 본문/)).toBeInTheDocument();
  });

  test('tool_call with no matchedResult shows command only', () => {
    const event = ev('tool_call', { tool_name: 'Read', input: { file_path: '/foo/bar.ts' } });
    render(<WhatSection event={event} matchedResult={null} />);
    expect(screen.getByText(/bar\.ts/)).toBeInTheDocument();
  });

  test('assistant_message shows text', () => {
    const event = ev('assistant_message', { text: '응답 텍스트 내용' });
    render(<WhatSection event={event} matchedResult={null} />);
    expect(screen.getByText(/응답 텍스트 내용/)).toBeInTheDocument();
  });

  test('thinking shows not-recorded notice', () => {
    const event = ev('thinking', { thinking: '', signature: 'sig123' });
    render(<WhatSection event={event} matchedResult={null} />);
    expect(screen.getByText(/기록되지 않음/)).toBeInTheDocument();
  });

  test('tool_call with is_error=true shows error indicator', () => {
    const event = ev('tool_call', { tool_name: 'Bash', input: { command: 'cat /etc/shadow' } });
    const result = { payload: { tool_result: { content: 'Permission denied', is_error: true } } } as any;
    render(<WhatSection event={event} matchedResult={result} />);
    expect(screen.getByText(/Permission denied/)).toBeInTheDocument();
    expect(screen.getByText(/오류/)).toBeInTheDocument();
  });

  test('diff_hunk shows patch preview and file_path', () => {
    const event = ev('diff_hunk', { patch_preview: '@@ -1,3 +1,4 @@', file_path: 'src/lib.rs' });
    render(<WhatSection event={event} matchedResult={null} />);
    expect(screen.getByText(/@@ -1,3/)).toBeInTheDocument();
    expect(screen.getByText(/src\/lib\.rs/)).toBeInTheDocument();
  });

  test('verification_run shows command and status', () => {
    const event = ev('verification_run', { command: 'cargo test', status: 'failed', failure_summary: 'test_foo failed' });
    render(<WhatSection event={event} matchedResult={null} />);
    expect(screen.getByText(/cargo test/)).toBeInTheDocument();
    expect(screen.getAllByText(/failed/).length).toBeGreaterThanOrEqual(1);
  });

  test('hook_event shows hook name', () => {
    const event = ev('hook_event', { hookName: 'PreToolUse', hook: { hook_event_name: 'PreToolUse' } });
    render(<WhatSection event={event} matchedResult={null} />);
    expect(screen.getByText(/PreToolUse/)).toBeInTheDocument();
  });

  test('unknown kind shows fallback when no scalar fields present', () => {
    // A kind with only nested object payload — no top-level scalar fields to show
    const event = ev('otel_span', { attributes: { nested: 'value' }, resource: { key: 'val' } });
    render(<WhatSection event={event} matchedResult={null} />);
    expect(screen.getByText(/Raw 탭/)).toBeInTheDocument();
  });
});
