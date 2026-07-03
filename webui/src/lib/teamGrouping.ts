// webui/src/lib/teamGrouping.ts
//
// Teammate 세션 그룹핑 (2026-07-03). CC 2.1.198부터 named Agent 스폰은 별도
// 최상위 세션이 되고, 그 레코드에만 team_name("session-<리드 id 앞 8자>")이
// 붙는다 — 리드 세션 레코드에는 없다 (실측 표본 1,
// tests/fixtures/transcripts/real/teammate_v01). 조인은 팀메이트→리드
// 단방향 prefix 매칭뿐이며, 매칭이 실패하거나 모호하면 그룹핑을 포기하고
// 일반 행으로 둔다 — 단정 불가한 매핑을 만들지 않는다.

/** 실측된 team_name 형태에서 리드 세션 id의 8-hex prefix를 뽑는다.
 *  다른 형태는 null — 조인 불가로 취급. */
export function leadPrefixOf(teamName: string): string | null {
  const m = /^session-([0-9a-f]{8})$/.exec(teamName);
  return m ? m[1] : null;
}

export interface TeamRow<T> {
  row: T;
  /** true → 이 행은 바로 위 lead 행의 팀메이트 (들여쓰기 렌더). */
  child: boolean;
}

interface Groupable {
  session_id: string;
  team_name?: string | null;
}

/** 정렬·필터가 끝난 행 배열을 받아, 각 팀메이트 행을 리드 행 바로 아래로
 *  옮기고 child로 표시한다. 리드가 목록에 없거나 prefix가 목록 내에서
 *  모호(비-팀메이트 세션 2개 이상 매칭)하면 원래 자리에 남는다. */
export function groupTeamRows<T extends Groupable>(rows: T[]): TeamRow<T>[] {
  const childrenByLead = new Map<string, T[]>();
  const childIds = new Set<string>();

  for (const r of rows) {
    const prefix = r.team_name ? leadPrefixOf(r.team_name) : null;
    if (!prefix) continue;
    const leads = rows.filter(
      (x) => !x.team_name && x.session_id !== r.session_id && x.session_id.startsWith(prefix),
    );
    if (leads.length !== 1) continue; // 부재 또는 모호 — 그룹핑 포기
    const leadId = leads[0].session_id;
    const list = childrenByLead.get(leadId) ?? [];
    list.push(r);
    childrenByLead.set(leadId, list);
    childIds.add(r.session_id);
  }

  const out: TeamRow<T>[] = [];
  for (const r of rows) {
    if (childIds.has(r.session_id)) continue; // 리드 아래에서 렌더
    out.push({ row: r, child: false });
    for (const c of childrenByLead.get(r.session_id) ?? []) {
      out.push({ row: c, child: true });
    }
  }
  return out;
}
