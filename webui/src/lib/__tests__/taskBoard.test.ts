import { describe, expect, it } from 'vitest';
import { buildTaskBoard } from '../taskBoard';
import type { ObservedEventDto } from '../../api/types';

/** Minimal ObservedEventDto factory — fills the required fields so each test
 *  only specifies what it cares about (kind/tool_name/payload/observed_at). */
function ev(p: Partial<ObservedEventDto> & { event_id: string }): ObservedEventDto {
  return {
    raw_event_id: p.event_id,
    session_id: 's1',
    event_uuid: null,
    parent_uuid: null,
    observed_at: '2026-06-23T04:00:00.000+00:00',
    actor: 'assistant',
    kind: 'tool_call',
    subkind: null,
    tool_use_id: null,
    tool_name: null,
    turn_id: null,
    is_sidechain: false,
    is_meta: false,
    payload: {},
    ...p,
  };
}

function create(id: string, useId: string, subject: string, at: string): ObservedEventDto {
  return ev({
    event_id: id,
    tool_use_id: useId,
    tool_name: 'TaskCreate',
    observed_at: at,
    payload: { content_ordinal: 0, tool_name: 'TaskCreate', input: { subject } },
  });
}

function createResult(id: string, useId: string, text: string): ObservedEventDto {
  return ev({
    event_id: id,
    kind: 'tool_result',
    tool_use_id: useId,
    tool_name: null,
    payload: { content_ordinal: 0, tool_result: { tool_use_id: useId, type: 'tool_result', content: text } },
  });
}

function update(id: string, taskId: string, status: string, at: string): ObservedEventDto {
  return ev({
    event_id: id,
    tool_use_id: id,
    tool_name: 'TaskUpdate',
    observed_at: at,
    payload: { content_ordinal: 0, tool_name: 'TaskUpdate', input: { taskId, status } },
  });
}

describe('buildTaskBoard', () => {
  // Real session 4dc1b1bb shape: 3 TaskCreate, task#1 transitions through
  // in_progress→completed, task#2/#3 jump straight to completed.
  const events: ObservedEventDto[] = [
    create('c1', 'u1', '테스트 위생', '2026-06-23T04:01:34.000+00:00'),
    createResult('r1', 'u1', 'Task #1 created successfully: 테스트 위생'),
    create('c2', 'u2', 'README 최신화', '2026-06-23T04:01:40.000+00:00'),
    createResult('r2', 'u2', 'Task #2 created successfully: README 최신화'),
    create('c3', 'u3', '스펙 문서 최신화', '2026-06-23T04:01:46.000+00:00'),
    createResult('r3', 'u3', 'Task #3 created successfully: 스펙 문서 최신화'),
    update('up1a', '1', 'in_progress', '2026-06-23T04:06:35.000+00:00'),
    update('up1b', '1', 'completed', '2026-06-23T04:16:23.000+00:00'),
    update('up2', '2', 'completed', '2026-06-23T04:16:26.000+00:00'),
    update('up3', '3', 'completed', '2026-06-23T04:16:29.000+00:00'),
    // TaskStop targets a background task (alphanumeric id) — must be ignored.
    ev({
      event_id: 'stop1',
      tool_use_id: 'us1',
      tool_name: 'TaskStop',
      observed_at: '2026-06-23T04:43:30.000+00:00',
      payload: { content_ordinal: 0, tool_name: 'TaskStop', input: {} },
    }),
  ];

  it('correlates create→update by taskId from the result line, sorted by numeric id', () => {
    const board = buildTaskBoard(events);
    expect(board.map((t) => t.taskId)).toEqual(['1', '2', '3']);
    expect(board[0].subject).toBe('테스트 위생');
    expect(board[1].subject).toBe('README 최신화');
    // carries the TaskCreate event_id so the board can jump into the replay
    expect(board[0].eventId).toBe('c1');
    expect(board[0].transitions.map((x) => x.eventId)).toEqual(['c1', 'up1a', 'up1b']);
  });

  it('derives latest status and the created→…→final transition timeline', () => {
    const board = buildTaskBoard(events);
    const t1 = board.find((t) => t.taskId === '1')!;
    expect(t1.status).toBe('completed');
    expect(t1.transitions.map((x) => x.status)).toEqual(['created', 'in_progress', 'completed']);
    expect(t1.createdAt).toBe('2026-06-23T04:01:34.000+00:00');
    // 04:01:34 → 04:16:23 = 14m49s = 889_000 ms
    expect(t1.durationMs).toBe(889_000);
    expect(t1.sawInProgress).toBe(true);
  });

  it('flags tasks that reached completion without an observed in_progress transition', () => {
    const board = buildTaskBoard(events);
    const t2 = board.find((t) => t.taskId === '2')!;
    expect(t2.status).toBe('completed');
    expect(t2.sawInProgress).toBe(false);
    expect(t2.transitions.map((x) => x.status)).toEqual(['created', 'completed']);
  });

  it('ignores TaskStop (background-task lifecycle, not a numeric todo)', () => {
    const board = buildTaskBoard(events);
    expect(board).toHaveLength(3);
    expect(board.some((t) => t.taskId.startsWith('us') || t.subject.includes('stop'))).toBe(false);
  });

  it('returns an empty board when there are no task events', () => {
    expect(buildTaskBoard([])).toEqual([]);
    expect(buildTaskBoard([ev({ event_id: 'x', tool_name: 'Bash' })])).toEqual([]);
  });
});
