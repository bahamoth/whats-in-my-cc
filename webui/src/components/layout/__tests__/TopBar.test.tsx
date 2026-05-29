/**
 * R1 RED — TopBar is the single breadcrumb at the top of the session page.
 * It replaces the in-page "← Sessions" header that duplicated the nav rail.
 * See plan R1 Task 1 / spec §2 (#8 top overlap).
 */
import { render, screen, within } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { MemoryRouter } from 'react-router-dom';
import { TopBar } from '../TopBar';

function renderBar(sessionId = 'aac68973') {
  return render(
    <MemoryRouter>
      <TopBar sessionId={sessionId} />
    </MemoryRouter>,
  );
}

describe('TopBar', () => {
  it('renders a breadcrumb navigation landmark', () => {
    renderBar();
    expect(screen.getByRole('navigation', { name: /breadcrumb/i })).toBeInTheDocument();
  });

  it('links "Sessions" back to the list', () => {
    renderBar();
    const nav = screen.getByRole('navigation', { name: /breadcrumb/i });
    const link = within(nav).getByRole('link', { name: /sessions/i });
    expect(link).toHaveAttribute('href', '/sessions');
  });

  it('shows the current session id as the trailing crumb (not a link)', () => {
    renderBar('aac68973');
    const nav = screen.getByRole('navigation', { name: /breadcrumb/i });
    const current = within(nav).getByText('aac68973');
    expect(current.closest('a')).toBeNull();
    expect(current).toHaveAttribute('aria-current', 'page');
  });
});
