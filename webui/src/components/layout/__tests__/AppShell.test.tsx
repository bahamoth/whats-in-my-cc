/**
 * PR-1 RED — AppShell wraps every route with a nav rail, main slot, and
 * right drawer slot. It must not change the rendered children but must
 * surface the correct semantic landmarks for screen readers and let the
 * design tokens drive layout via CSS grid. See plan §10.1 PR-1.
 */
import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { AppShell } from '../AppShell';

function renderShell(children: React.ReactNode = <p>child</p>) {
  return render(
    <MemoryRouter>
      <AppShell>{children}</AppShell>
    </MemoryRouter>,
  );
}

describe('AppShell', () => {
  it('renders children inside a <main role="main"> landmark', () => {
    renderShell(<p>hello-child</p>);
    const main = screen.getByRole('main');
    expect(main.tagName.toLowerCase()).toBe('main');
    expect(within(main).getByText('hello-child')).toBeInTheDocument();
  });

  it('exposes a primary navigation rail labelled for assistive tech', () => {
    renderShell();
    const nav = screen.getByRole('navigation', { name: /primary/i });
    expect(nav).toBeInTheDocument();
  });

  it('navigation rail contains at least one navigable link', () => {
    renderShell();
    const nav = screen.getByRole('navigation', { name: /primary/i });
    const links = within(nav).getAllByRole('link');
    expect(links.length).toBeGreaterThanOrEqual(1);
  });

  it('renders a complementary right slot (drawer container) reachable by aria', () => {
    renderShell();
    // The right slot may be empty by default, but its landmark must exist
    // so feature code (PR-6 WhyPanel, PR-5 BottomDrawer) can portal into it.
    const slot = screen.getByRole('complementary', { name: /side panel/i });
    expect(slot).toBeInTheDocument();
  });

  it('top-level shell element uses CSS grid layout', () => {
    const { container } = renderShell();
    // We assert on the data attribute the implementation must set so the
    // CSS grid contract is testable independent of computed-style flakiness
    // in jsdom (jsdom does not implement layout).
    const shell = container.querySelector('[data-witmcc-shell]') as HTMLElement | null;
    expect(shell).not.toBeNull();
    expect(shell!.dataset.layout).toBe('grid');
  });

  it('skip-to-content link precedes the navigation for keyboard users', () => {
    renderShell();
    const skip = screen.getByRole('link', { name: /skip to content/i });
    const nav = screen.getByRole('navigation', { name: /primary/i });
    // DOM order: skip link before nav rail.
    // node.compareDocumentPosition returns FOLLOWING when arg is after the node.
    expect(skip.compareDocumentPosition(nav) & Node.DOCUMENT_POSITION_FOLLOWING).toBeTruthy();
  });
});
