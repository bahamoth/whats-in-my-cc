/**
 * PR-5 RED — ChatMessageNode renders a user / assistant chat turn as a
 * compact bubble with a role pill, the text content, and (optionally) a
 * token chip. It MUST NOT render the raw record payload into the DOM —
 * raw goes to BottomDrawer only on `onOpenRaw`.
 *
 * No markdown library dependency yet (would be a separate PR); for now
 * the component renders text in a <p> and asserts XSS escape via React's
 * default escaping behaviour.
 */
import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { ChatMessageNode } from '../ChatMessageNode';

describe('ChatMessageNode', () => {
  it('renders the role pill', () => {
    render(<ChatMessageNode role="user" text="hello" />);
    const pill = screen.getByTestId('chat-role-pill');
    expect(pill.textContent).toMatch(/user/i);
  });

  it('renders the text content', () => {
    render(<ChatMessageNode role="assistant" text="some reply text" />);
    expect(screen.getByText('some reply text')).toBeInTheDocument();
  });

  it('renders a token chip when tokenCount is supplied', () => {
    render(<ChatMessageNode role="assistant" text="hi" tokenCount={1500} />);
    expect(screen.getByTestId('chat-token-chip')).toBeInTheDocument();
    expect(screen.getByTestId('chat-token-chip').textContent).toMatch(/1\.5k/);
  });

  it('omits the token chip when tokenCount is missing or zero', () => {
    const { rerender } = render(<ChatMessageNode role="user" text="x" />);
    expect(screen.queryByTestId('chat-token-chip')).toBeNull();
    rerender(<ChatMessageNode role="user" text="x" tokenCount={0} />);
    expect(screen.queryByTestId('chat-token-chip')).toBeNull();
  });

  it('escapes raw HTML in text (no <script> in DOM)', () => {
    const malicious = '<img src=x onerror="alert(1)" />';
    const { container } = render(<ChatMessageNode role="user" text={malicious} />);
    // React text node escaping means the string appears as-is in textContent
    // but no <img> element exists in the DOM.
    expect(container.querySelector('img')).toBeNull();
    expect(container.textContent ?? '').toContain('<img');
  });

  it('exposes a "raw" toggle that calls onOpenRaw', () => {
    const onOpenRaw = vi.fn();
    render(<ChatMessageNode role="user" text="x" onOpenRaw={onOpenRaw} />);
    fireEvent.click(screen.getByRole('button', { name: /raw/i }));
    expect(onOpenRaw).toHaveBeenCalledTimes(1);
  });

  it('omits the "raw" toggle when onOpenRaw is not provided', () => {
    render(<ChatMessageNode role="user" text="x" />);
    expect(screen.queryByRole('button', { name: /raw/i })).toBeNull();
  });

  it('role is reflected in data-role attribute for token styling', () => {
    const { container } = render(<ChatMessageNode role="assistant" text="x" />);
    expect(container.querySelector('[data-role="assistant"]')).not.toBeNull();
  });
});
