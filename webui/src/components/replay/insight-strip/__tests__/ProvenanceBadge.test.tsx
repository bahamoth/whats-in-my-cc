import { describe, expect, it } from 'vitest';
import { screen } from '@testing-library/react';
import { renderWithI18n as render } from '../../../../test/i18nRender';
import { ProvenanceBadge } from '../ProvenanceBadge';

describe('ProvenanceBadge', () => {
  it('renders the Korean label for each provenance', () => {
    const { rerender } = render(<ProvenanceBadge provenance="measured" />);
    expect(screen.getByTestId('provenance-badge')).toHaveTextContent('측정');
    rerender(<ProvenanceBadge provenance="mixed" />);
    expect(screen.getByTestId('provenance-badge')).toHaveTextContent('혼합');
    rerender(<ProvenanceBadge provenance="estimated" />);
    expect(screen.getByTestId('provenance-badge')).toHaveTextContent('추정');
    rerender(<ProvenanceBadge provenance="uncollected" />);
    expect(screen.getByTestId('provenance-badge')).toHaveTextContent('미수집·예정');
  });

  it('exposes the provenance via data-provenance for token-based colouring', () => {
    render(<ProvenanceBadge provenance="uncollected" />);
    expect(screen.getByTestId('provenance-badge').dataset.provenance).toBe('uncollected');
  });
});
