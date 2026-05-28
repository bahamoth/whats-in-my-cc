/**
 * PR-8 RED — WhyPanel responsive mode. The panel is `inline` when there
 * is room for a third column (≥1400 px) and switches to `floating`
 * (overlay sheet) below that. The component publishes `data-layout` so
 * the test does not depend on layout math.
 */
import { describe, expect, it, beforeEach, vi } from 'vitest';
import { render, screen } from '@testing-library/react';
import { WhyPanel } from '../WhyPanel';
import type { FindingDto } from '../../../api/types';

let mediaListeners: Array<(ev: MediaQueryListEvent) => void> = [];
let inlineMatches = false;

function installMatchMedia() {
  mediaListeners = [];
  window.matchMedia = ((query: string) => ({
    matches: query.includes('1400') ? inlineMatches : false,
    media: query,
    addEventListener: (_: string, fn: (ev: MediaQueryListEvent) => void) => {
      mediaListeners.push(fn);
    },
    removeEventListener: (_: string, fn: (ev: MediaQueryListEvent) => void) => {
      const i = mediaListeners.indexOf(fn);
      if (i >= 0) mediaListeners.splice(i, 1);
    },
    addListener: vi.fn(),
    removeListener: vi.fn(),
    dispatchEvent: () => false,
    onchange: null,
  })) as unknown as typeof window.matchMedia;
}

const finding: FindingDto = {
  finding_id: 'f1',
  schema_version: 'v1',
  session_id: 's',
  category: 'risky_action',
  severity: 'high',
  confidence: 0.85,
  summary: 'x',
  evidence_refs: ['ev1'],
  evidence_projection: {},
  provenance: {},
  status: 'active',
  created_at: '2026-05-29T00:00:00Z',
};

beforeEach(() => {
  installMatchMedia();
});

describe('WhyPanel responsive layout', () => {
  it('wide viewport (≥1400px) ⇒ data-layout="inline"', () => {
    inlineMatches = true;
    render(
      <WhyPanel open finding={finding} onClose={() => {}} onEvidenceHover={() => {}} />,
    );
    expect(screen.getByTestId('why-panel').dataset.layout).toBe('inline');
  });

  it('narrow viewport (<1400px) ⇒ data-layout="floating"', () => {
    inlineMatches = false;
    render(
      <WhyPanel open finding={finding} onClose={() => {}} onEvidenceHover={() => {}} />,
    );
    expect(screen.getByTestId('why-panel').dataset.layout).toBe('floating');
  });
});
