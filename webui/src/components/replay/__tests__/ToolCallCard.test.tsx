/**
 * PR-5 RED — ToolCallCard collapses a tool_call (and optionally its
 * tool_result) into one structured tile. Locked behaviour:
 *  - tool name + icon slot
 *  - input summary chip; > 120 chars ⇒ ellipsis ("…")
 *  - output preview; renders only first N chars / first 3 lines
 *  - latency badge (formatMs)
 *  - status badge: ok / error / pending
 *  - "raw" toggle ⇒ onOpenRaw
 *  - raw JSON payload NEVER inlined into the card
 */
import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ToolCallCard } from '../ToolCallCard';

describe('ToolCallCard', () => {
  it('renders the tool name', () => {
    render(<ToolCallCard toolName="Bash" status="ok" />);
    expect(screen.getByText('Bash')).toBeInTheDocument();
  });

  it('renders input summary when supplied', () => {
    render(<ToolCallCard toolName="Bash" status="ok" inputSummary="ls -la /tmp" />);
    expect(screen.getByText('ls -la /tmp')).toBeInTheDocument();
  });

  it('truncates input summary longer than 120 chars with an ellipsis', () => {
    const long = 'x'.repeat(200);
    render(<ToolCallCard toolName="Bash" status="ok" inputSummary={long} />);
    const chip = screen.getByTestId('toolcall-input');
    expect((chip.textContent ?? '').length).toBeLessThanOrEqual(125); // 120 + ellipsis
    expect(chip.textContent ?? '').toMatch(/…$/);
  });

  it('renders output preview as plain text (first 3 lines)', () => {
    const out = 'line1\nline2\nline3\nline4\nline5';
    render(<ToolCallCard toolName="Bash" status="ok" outputPreview={out} />);
    const pre = screen.getByTestId('toolcall-output');
    expect(pre.textContent ?? '').toContain('line1');
    expect(pre.textContent ?? '').toContain('line3');
    // line4 must be truncated out so the card stays compact.
    expect(pre.textContent ?? '').not.toContain('line4');
  });

  it('renders a latency badge using formatMs', () => {
    render(<ToolCallCard toolName="Bash" status="ok" latencyMs={1500} />);
    expect(screen.getByTestId('toolcall-latency').textContent).toMatch(/1\.5s/);
  });

  it('encodes status in data-state attribute', () => {
    const { rerender } = render(<ToolCallCard toolName="Bash" status="ok" />);
    expect(screen.getByTestId('toolcall-card').dataset.state).toBe('ok');
    rerender(<ToolCallCard toolName="Bash" status="error" />);
    expect(screen.getByTestId('toolcall-card').dataset.state).toBe('error');
    rerender(<ToolCallCard toolName="Bash" status="pending" />);
    expect(screen.getByTestId('toolcall-card').dataset.state).toBe('pending');
  });

  it('"raw" toggle invokes onOpenRaw', () => {
    const onOpenRaw = vi.fn();
    render(<ToolCallCard toolName="Bash" status="ok" onOpenRaw={onOpenRaw} />);
    fireEvent.click(screen.getByRole('button', { name: /raw/i }));
    expect(onOpenRaw).toHaveBeenCalled();
  });

  it('does not dump arbitrary raw payload into the DOM', () => {
    // Card receives a payload prop (used for the raw drawer) but must NOT
    // render its content directly. We confirm by passing a recognisable
    // sentinel and asserting it never appears in the DOM text.
    const sentinel = 'SECRET_TOKEN_DEADBEEF';
    render(
      <ToolCallCard
        toolName="Bash"
        status="ok"
        inputSummary="ls"
        rawPayload={{ secret: sentinel }}
      />,
    );
    expect(document.body.textContent ?? '').not.toContain(sentinel);
  });
});
