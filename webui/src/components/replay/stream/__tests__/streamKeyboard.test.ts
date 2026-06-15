import { describe, expect, it } from 'vitest';
import type { StreamItem } from '../streamModel';
import { primaryEventId, spineEventIds, stepEventId, nextErrorEventId } from '../streamKeyboard';

// Minimal hand-built items — only the fields the keyboard helpers read.
const msg = (id: string): StreamItem =>
  ({ type: 'message', id, eventId: id, role: 'assistant' } as unknown as StreamItem);
const thinking = (id: string): StreamItem =>
  ({ type: 'thinking', id: `t-${id}`, events: [{ eventId: id, timestamp: '', sigLen: 1, requestId: null, metrics: null }] } as unknown as StreamItem);
const run = (firstId: string, isError: boolean): StreamItem =>
  ({
    type: 'activity-run',
    id: `r-${firstId}`,
    events: [{ event: { event_id: firstId }, result: { isError } }],
  } as unknown as StreamItem);
const endCard = (notifId: string, status: string): StreamItem =>
  ({ type: 'subagent-end', id: `e-${notifId}`, agentId: 'a', color: '#000', conclusion: '', durationMs: 0, messageCount: 0, toolCount: 0, endTimestamp: '', status, notificationEventId: notifId } as unknown as StreamItem);

describe('streamKeyboard — primaryEventId', () => {
  it('extracts the representative event id per item type', () => {
    expect(primaryEventId(msg('m1'))).toBe('m1');
    expect(primaryEventId(thinking('th1'))).toBe('th1');
    expect(primaryEventId(run('r1', false))).toBe('r1');
    expect(primaryEventId(endCard('n1', 'completed'))).toBe('n1');
  });
});

describe('streamKeyboard — spine + step (j/k)', () => {
  const items = [msg('m1'), thinking('th1'), run('r1', false), msg('m2')];

  it('lists selectable ids in order', () => {
    expect(spineEventIds(items)).toEqual(['m1', 'th1', 'r1', 'm2']);
  });
  it('j (down) moves to the next id, k (up) to the previous', () => {
    expect(stepEventId(items, 'th1', 'down')).toBe('r1');
    expect(stepEventId(items, 'th1', 'up')).toBe('m1');
  });
  it('clamps at the ends (no wrap)', () => {
    expect(stepEventId(items, 'm2', 'down')).toBe('m2');
    expect(stepEventId(items, 'm1', 'up')).toBe('m1');
  });
  it('selects the first id when nothing is selected yet', () => {
    expect(stepEventId(items, null, 'down')).toBe('m1');
  });
});

describe('streamKeyboard — nextErrorEventId (e)', () => {
  const items = [msg('m1'), run('ok1', false), run('bad1', true), msg('m2'), run('bad2', true)];

  it('jumps to the next error item after the current selection', () => {
    expect(nextErrorEventId(items, 'm1')).toBe('bad1');
    expect(nextErrorEventId(items, 'bad1')).toBe('bad2');
  });
  it('treats a failed/killed end card as an error', () => {
    const withFail = [msg('m1'), endCard('n1', 'failed')];
    expect(nextErrorEventId(withFail, 'm1')).toBe('n1');
  });
  it('returns null when there is no error after the current position', () => {
    expect(nextErrorEventId([msg('m1'), run('ok', false)], 'm1')).toBeNull();
  });
});
