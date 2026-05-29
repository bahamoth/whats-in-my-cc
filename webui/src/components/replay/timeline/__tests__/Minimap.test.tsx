// webui/src/components/replay/timeline/__tests__/Minimap.test.tsx
/** R4 RED — Minimap brush drives the viewport. Plan R4 Task 5. */
import { render, screen, fireEvent } from '@testing-library/react';
import { describe, expect, it, vi } from 'vitest';
import { Minimap } from '../Minimap';

describe('Minimap', () => {
  it('renders a track and a brush window', () => {
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 250, t1: 750 }} onChange={() => {}} width={400} />);
    expect(screen.getByTestId('minimap-track')).toBeInTheDocument();
    expect(screen.getByTestId('brush-window')).toBeInTheDocument();
  });

  it('renders a brush window sized to the current viewport fraction', () => {
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 250, t1: 750 }} onChange={() => {}} width={400} />);
    const w = screen.getByTestId('brush-window');
    // window covers 50% of extent => width ~200 of 400
    expect(Number(w.getAttribute('width'))).toBeCloseTo(200, 0);
    expect(Number(w.getAttribute('x'))).toBeCloseTo(100, 0);
  });

  it('calls onChange when the overview is clicked to recenter', () => {
    const onChange = vi.fn();
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 0, t1: 200 }} onChange={onChange} width={400} />);
    fireEvent.mouseDown(screen.getByTestId('minimap-track'), { clientX: 200 });
    expect(onChange).toHaveBeenCalled();
  });

  it('recenters the window on the clicked time (exact inverse mapping)', () => {
    const onChange = vi.fn();
    // extent [0,1000], width 400 → 1px = 2.5 time units. viewport width 200
    // (half = 100). Click at clientX=200 → focusT = (200/400)*1000 = 500 →
    // window centered there = [400, 600], within extent so unclamped.
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 0, t1: 200 }} onChange={onChange} width={400} />);
    fireEvent.mouseDown(screen.getByTestId('minimap-track'), { clientX: 200 });
    const v = onChange.mock.calls[0][0];
    expect(v.t0).toBeCloseTo(400, 0);
    expect(v.t1).toBeCloseTo(600, 0);
  });

  it('onChange receives a Viewport with t0 < t1', () => {
    const onChange = vi.fn();
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 0, t1: 200 }} onChange={onChange} width={400} />);
    fireEvent.mouseDown(screen.getByTestId('minimap-track'), { clientX: 200 });
    const v = onChange.mock.calls[0][0];
    expect(v.t0).toBeLessThan(v.t1);
  });

  it('onChange result is clamped within the extent', () => {
    const onChange = vi.fn();
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 0, t1: 200 }} onChange={onChange} width={400} />);
    // Click at the far right edge
    fireEvent.mouseDown(screen.getByTestId('minimap-track'), { clientX: 399 });
    const v = onChange.mock.calls[0][0];
    expect(v.t0).toBeGreaterThanOrEqual(0);
    expect(v.t1).toBeLessThanOrEqual(1000);
  });

  it('window x=0 when viewport starts at extent start', () => {
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 0, t1: 500 }} onChange={() => {}} width={400} />);
    const w = screen.getByTestId('brush-window');
    expect(Number(w.getAttribute('x'))).toBeCloseTo(0, 0);
    // 50% of extent → width 200
    expect(Number(w.getAttribute('width'))).toBeCloseTo(200, 0);
  });

  it('window fills the entire track when viewport equals extent', () => {
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 0, t1: 1000 }} onChange={() => {}} width={400} />);
    const w = screen.getByTestId('brush-window');
    expect(Number(w.getAttribute('x'))).toBeCloseTo(0, 0);
    expect(Number(w.getAttribute('width'))).toBeCloseTo(400, 0);
  });

  it('drag on track calls onChange multiple times', () => {
    const onChange = vi.fn();
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 250, t1: 750 }} onChange={onChange} width={400} />);
    const track = screen.getByTestId('minimap-track');
    fireEvent.mouseDown(track, { clientX: 100 });
    fireEvent.mouseMove(track, { clientX: 150 });
    fireEvent.mouseMove(track, { clientX: 200 });
    fireEvent.mouseUp(track);
    // mousedown + two drag moves each recenter → 3 calls. (A move after
    // mouseup must NOT fire — see next assertion.)
    expect(onChange.mock.calls.length).toBe(3);
    fireEvent.mouseMove(track, { clientX: 250 });
    expect(onChange.mock.calls.length).toBe(3);
  });

  it('uses the width prop for geometry (not getBoundingClientRect)', () => {
    // width=200; viewport covers 25%-75% → x=50, w=100
    render(<Minimap extent={[0, 1000]} viewport={{ t0: 250, t1: 750 }} onChange={() => {}} width={200} />);
    const w = screen.getByTestId('brush-window');
    expect(Number(w.getAttribute('width'))).toBeCloseTo(100, 0);
    expect(Number(w.getAttribute('x'))).toBeCloseTo(50, 0);
  });
});
