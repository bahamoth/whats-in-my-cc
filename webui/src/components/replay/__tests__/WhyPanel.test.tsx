/**
 * PR-6 RED — WhyPanel renders a finding's claim / evidence / confidence /
 * limitation / resource_uri. It opens via the ReplaySelectionContext
 * `whyPanelOpen` flag and reads the active finding id from there.
 *
 * Defensive behaviour: a finding with empty evidence_refs (which the
 * useFindingsQuery already filters out, but client must defend) ⇒ panel
 * shows a "no evidence" warning state instead of pretending the claim is
 * supported.
 */
import { describe, expect, it, vi, beforeEach, afterEach } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { WhyPanel } from '../WhyPanel';
import type { FindingDto, FindingEvidenceResponse } from '../../../api/types';

function finding(extra: Partial<FindingDto> = {}): FindingDto {
  return {
    finding_id: 'f1',
    schema_version: 'v1',
    session_id: 's',
    category: 'risky_action',
    severity: 'high',
    confidence: 0.85,
    summary: 'A risky bash command ran without verification.',
    evidence_refs: [{ kind: 'node', node_id: 'n-bash-1' }],
    evidence_projection: {},
    provenance: {},
    status: 'active',
    created_at: '2026-05-29T00:00:00Z',
    ...extra,
  };
}

beforeEach(() => {
  vi.stubGlobal('fetch', vi.fn());
});
afterEach(() => {
  vi.unstubAllGlobals();
});

describe('WhyPanel', () => {
  it('renders claim, confidence, evidence chips', () => {
    render(
      <WhyPanel
        open
        finding={finding()}
        onClose={vi.fn()}
        onEvidenceHover={vi.fn()}
      />,
    );
    expect(screen.getByText(/risky bash command/i)).toBeInTheDocument();
    expect(screen.getByText(/85%/)).toBeInTheDocument();
    expect(screen.getByTestId('evidence-chip-n-bash-1')).toBeInTheDocument();
  });

  it('renders severity and category badges via data attributes', () => {
    render(
      <WhyPanel
        open
        finding={finding({ severity: 'medium', category: 'context_bloat' })}
        onClose={vi.fn()}
        onEvidenceHover={vi.fn()}
      />,
    );
    const panel = screen.getByTestId('why-panel');
    expect(panel.dataset.severity).toBe('medium');
    expect(panel.dataset.category).toBe('context_bloat');
  });

  it('hovering an evidence chip fires onEvidenceHover with the node id', () => {
    const onEvidenceHover = vi.fn();
    render(
      <WhyPanel
        open
        finding={finding()}
        onClose={vi.fn()}
        onEvidenceHover={onEvidenceHover}
      />,
    );
    fireEvent.mouseEnter(screen.getByTestId('evidence-chip-n-bash-1'));
    expect(onEvidenceHover).toHaveBeenCalledWith('n-bash-1');
    fireEvent.mouseLeave(screen.getByTestId('evidence-chip-n-bash-1'));
    expect(onEvidenceHover).toHaveBeenCalledWith(null);
  });

  it('renders a "Copy resource URI" button that calls navigator.clipboard.writeText', () => {
    const writeText = vi.fn().mockResolvedValue(undefined);
    vi.stubGlobal('navigator', { ...navigator, clipboard: { writeText } });
    render(
      <WhyPanel
        open
        finding={finding()}
        onClose={vi.fn()}
        onEvidenceHover={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByRole('button', { name: /copy resource uri/i }));
    expect(writeText).toHaveBeenCalledWith('whats-in-my-cc://findings/f1');
  });

  it('shows a warning state when evidence_refs is empty', () => {
    render(
      <WhyPanel
        open
        finding={finding({ evidence_refs: [] })}
        onClose={vi.fn()}
        onEvidenceHover={vi.fn()}
      />,
    );
    const panel = screen.getByTestId('why-panel');
    expect(panel.dataset.warning).toBe('no-evidence');
  });

  it('does not render when closed', () => {
    render(
      <WhyPanel
        open={false}
        finding={finding()}
        onClose={vi.fn()}
        onEvidenceHover={vi.fn()}
      />,
    );
    expect(screen.queryByTestId('why-panel')).toBeNull();
  });

  it('does not render when finding is null', () => {
    render(
      <WhyPanel
        open
        finding={null}
        onClose={vi.fn()}
        onEvidenceHover={vi.fn()}
      />,
    );
    expect(screen.queryByTestId('why-panel')).toBeNull();
  });

  it('Esc keypress fires onClose', () => {
    const onClose = vi.fn();
    render(
      <WhyPanel
        open
        finding={finding()}
        onClose={onClose}
        onEvidenceHover={vi.fn()}
      />,
    );
    fireEvent.keyDown(window, { key: 'Escape' });
    expect(onClose).toHaveBeenCalled();
  });

  it('exposes additional evidence content when supplied via evidence prop', () => {
    const evidence: FindingEvidenceResponse = {
      finding: finding(),
      subgraph: { nodes: [], edges: [] },
      raw_source_refs: [
        {
          event_id: 'ev-1',
          source_type: 'transcript',
          source_uri: 'transcript://x.jsonl#42',
          redaction_state: 'none',
        },
      ],
    };
    render(
      <WhyPanel
        open
        finding={finding()}
        evidence={evidence}
        onClose={vi.fn()}
        onEvidenceHover={vi.fn()}
      />,
    );
    expect(screen.getByText('transcript://x.jsonl#42')).toBeInTheDocument();
  });
});
