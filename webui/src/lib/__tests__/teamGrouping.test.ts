import { describe, expect, it } from 'vitest';
import { groupTeamRows, leadPrefixOf, leadOf, teammatesOf } from '../teamGrouping';

// team_name 실측 형태: "session-" + 리드 세션 id 앞 8자 — 표본 1
// (tests/fixtures/transcripts/real/teammate_v01, CC 2.1.198). 형태가 바뀌면
// 조인만 무산되고(팀메이트는 일반 행 유지) 오분류는 없어야 한다.

const lead = { session_id: 'bebd8197-894f-4ed8-95ed-7e9b6ed3a0e5', team_name: null };
const mateA = { session_id: 'e8b4a11e-541d-4d64-9aae-52663c01c5cc', team_name: 'session-bebd8197', agent_name: 'explore-api' };
const mateB = { session_id: 'fb3f080e-904f-4d5f-9a27-017b6e4194f1', team_name: 'session-bebd8197', agent_name: 'explore-insight' };
const other = { session_id: '673313c6-e375-4e4b-ad53-98144cdd4d5f', team_name: null };

describe('leadPrefixOf', () => {
  it('parses the observed team_name shape only', () => {
    expect(leadPrefixOf('session-bebd8197')).toBe('bebd8197');
    expect(leadPrefixOf('my-team')).toBeNull();
    expect(leadPrefixOf('session-XYZ')).toBeNull();
  });
});

describe('groupTeamRows', () => {
  it('moves teammates directly under their lead, flagged as children', () => {
    const out = groupTeamRows([mateA, other, lead, mateB]);
    expect(out.map((r) => r.row.session_id)).toEqual([
      other.session_id,
      lead.session_id,
      mateA.session_id,
      mateB.session_id,
    ]);
    expect(out.map((r) => r.child)).toEqual([false, false, true, true]);
  });

  it('keeps a teammate in place when its lead is not listed', () => {
    const out = groupTeamRows([mateA, other]);
    expect(out.map((r) => r.row.session_id)).toEqual([mateA.session_id, other.session_id]);
    expect(out.every((r) => !r.child)).toBe(true);
  });

  it('never groups when the lead prefix is ambiguous', () => {
    const clash = { session_id: 'bebd8197-0000-0000-0000-000000000000', team_name: null };
    const out = groupTeamRows([lead, clash, mateA]);
    expect(out.every((r) => !r.child)).toBe(true);
  });
});

// 세션 상세(리드/팀메이트 배지)용 방향별 조회 — groupTeamRows와 같은 조인
// 규칙(표본 1 형태, 모호 시 포기)을 공유해야 한다.
describe('teammatesOf / leadOf', () => {
  const rows = [lead, mateA, mateB, other];

  it('teammatesOf finds the sessions pointing at this lead', () => {
    expect(teammatesOf(rows, lead.session_id).map((r) => r.session_id)).toEqual([
      mateA.session_id,
      mateB.session_id,
    ]);
    expect(teammatesOf(rows, other.session_id)).toEqual([]);
  });

  it('leadOf resolves a teammate back to its unique lead', () => {
    expect(leadOf(rows, mateA.session_id)?.session_id).toBe(lead.session_id);
    expect(leadOf(rows, lead.session_id)).toBeNull();
    expect(leadOf(rows, other.session_id)).toBeNull();
  });

  it('leadOf gives up on ambiguity', () => {
    const clash = { session_id: 'bebd8197-0000-0000-0000-000000000000', team_name: null };
    expect(leadOf([...rows, clash], mateA.session_id)).toBeNull();
  });
});
