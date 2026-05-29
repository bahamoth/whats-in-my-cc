// webui/src/components/replay/detail/__tests__/InsightTab.test.tsx
/**
 * R3 RED — InsightTab lists findings linked to the selected node. Absorbs the
 * WhyPanel. Plan R3 Task 4 / spec §4.
 */
import { render, screen } from '@testing-library/react';
import { describe, expect, it } from 'vitest';
import { InsightTab } from '../InsightTab';
import type { FindingDto } from '../../../api/types';

function finding(p: Partial<FindingDto>): FindingDto {
  return {
    finding_id: 'f1', schema_version: '1', session_id: 's', category: 'risky_action',
    severity: 'high', confidence: 0.8, summary: 'risky rm -rf', evidence_refs: [],
    evidence_projection: {}, provenance: {}, status: 'open', created_at: '', ...p,
  };
}

describe('InsightTab', () => {
  it('renders each finding with summary, category, and severity', () => {
    render(<InsightTab findings={[finding({})]} />);
    expect(screen.getByText('risky rm -rf')).toBeInTheDocument();
    expect(screen.getByText(/risky_action/)).toBeInTheDocument();
    expect(screen.getByText(/high/i)).toBeInTheDocument();
  });

  it('renders confidence as a percentage', () => {
    render(<InsightTab findings={[finding({ confidence: 0.8 })]} />);
    expect(screen.getByText('80%')).toBeInTheDocument();
  });

  it('shows an empty hint when the node has no findings', () => {
    render(<InsightTab findings={[]} />);
    expect(screen.getByText(/no insights|no findings/i)).toBeInTheDocument();
  });
});
