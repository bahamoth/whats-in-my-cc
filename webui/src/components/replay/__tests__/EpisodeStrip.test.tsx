/**
 * PR-3 RED — EpisodeStrip. Horizontal stacked bar of episode phases. We
 * test the *behaviour* not the chart rendering (ECharts canvas in jsdom
 * is unreliable). The component exposes a data-testid grid the test can
 * inspect for each phase segment with its data-state, data-width-pct, and
 * a click handler on each segment that fires `onZoomTo(startISO, endISO)`.
 *
 * Visual regression of the rendered chart is covered by the browser smoke
 * (plan §10.3 PR-3) and by reading `ECharts option` from the DOM data
 * attribute the component publishes.
 *
 * See plan §10.1 PR-3.
 */
import { describe, expect, it, vi } from 'vitest';
import { render, screen, fireEvent } from '@testing-library/react';
import { EpisodeStrip } from '../EpisodeStrip';
import type { EpisodeDto } from '../../../api/types';

function ep(
  id: string,
  phase: EpisodeDto['phase'],
  startMs: number,
  endMs: number,
): EpisodeDto {
  return {
    episode_id: id,
    schema_version: 'v1',
    session_id: 's1',
    phase,
    start_event_id: `${id}-s`,
    end_event_id: `${id}-e`,
    started_at: new Date(startMs).toISOString(),
    ended_at: new Date(endMs).toISOString(),
    evidence_node_ids: [],
    classification_basis: [],
    confidence: 0.8,
    summary: null,
    classifier_version: 'v1',
    created_at: new Date(startMs).toISOString(),
  };
}

describe('EpisodeStrip', () => {
  it('renders one segment per episode', () => {
    const episodes = [
      ep('a', 'intake', 0, 1000),
      ep('b', 'action', 1000, 4000),
      ep('c', 'verification', 4000, 5000),
    ];
    render(<EpisodeStrip episodes={episodes} />);
    expect(screen.getAllByTestId(/^episode-segment-/)).toHaveLength(3);
  });

  it('normalises segment widths to sum 100%', () => {
    const episodes = [
      ep('a', 'intake', 0, 1000),
      ep('b', 'action', 1000, 4000),
      ep('c', 'verification', 4000, 5000),
    ];
    render(<EpisodeStrip episodes={episodes} />);
    const segs = screen.getAllByTestId(/^episode-segment-/);
    const widths = segs.map((el) => parseFloat(el.dataset.widthPct ?? '0'));
    const sum = widths.reduce((acc, w) => acc + w, 0);
    expect(sum).toBeGreaterThan(99.9);
    expect(sum).toBeLessThan(100.1);
  });

  it('single episode segment is always 100%', () => {
    render(<EpisodeStrip episodes={[ep('only', 'intake', 0, 5_000)]} />);
    const seg = screen.getByTestId('episode-segment-only');
    expect(parseFloat(seg.dataset.widthPct ?? '0')).toBeCloseTo(100, 1);
  });

  it('hovering a segment exposes phase summary in tooltip role', () => {
    const episodes = [ep('a', 'action', 0, 1000)];
    render(<EpisodeStrip episodes={episodes} />);
    const seg = screen.getByTestId('episode-segment-a');
    fireEvent.mouseEnter(seg);
    const tip = screen.getByRole('tooltip');
    expect(tip.textContent ?? '').toMatch(/action/i);
  });

  it('clicking a segment invokes onZoomTo with the episode time range', () => {
    const onZoomTo = vi.fn();
    const e = ep('b', 'verification', 5_000, 7_000);
    render(<EpisodeStrip episodes={[e]} onZoomTo={onZoomTo} />);
    fireEvent.click(screen.getByTestId('episode-segment-b'));
    expect(onZoomTo).toHaveBeenCalledWith(e.started_at, e.ended_at);
  });

  it('empty episodes array renders nothing (no chart placeholder collapse)', () => {
    const { container } = render(<EpisodeStrip episodes={[]} />);
    expect(container.querySelectorAll('[data-testid^="episode-segment-"]').length).toBe(0);
  });

  it('phase is encoded in data-state for token-based colouring', () => {
    const e = ep('x', 'verification', 0, 100);
    render(<EpisodeStrip episodes={[e]} />);
    const seg = screen.getByTestId('episode-segment-x');
    expect(seg.dataset.state).toBe('verification');
  });
});
