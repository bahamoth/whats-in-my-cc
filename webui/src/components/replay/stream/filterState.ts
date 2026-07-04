// filterState.ts — 스펙 §1.2/§1.4. 축끼리 AND, 축 내 CSV OR. URL 키는 f_ 접두.
export interface FilterState {
  kinds: string[]; roles: string[]; origins: string[];
  error: boolean; signal: boolean; verifications: string[];
  tools: string[]; models: string[]; q: string;
}

export const EMPTY_FILTER: FilterState = {
  kinds: [], roles: [], origins: [], error: false, signal: false,
  verifications: [], tools: [], models: [], q: '',
};

export function isFilterActive(f: FilterState): boolean {
  return (
    f.kinds.length > 0 || f.roles.length > 0 || f.origins.length > 0 ||
    f.error || f.signal || f.verifications.length > 0 ||
    f.tools.length > 0 || f.models.length > 0 || f.q.trim() !== ''
  );
}

/** 서버 쿼리 파라미터(스펙 §1.2 이름). 비활성 축은 키 자체를 생략. */
export interface EventFilterParams {
  kind?: string; role?: string; origin?: string; error?: 'true'; signal?: 'true';
  verification?: string; tool?: string; model?: string; q?: string;
}

export function toEventFilterParams(f: FilterState): EventFilterParams {
  const p: EventFilterParams = {};
  if (f.kinds.length) p.kind = f.kinds.join(',');
  if (f.roles.length) p.role = f.roles.join(',');
  if (f.origins.length) p.origin = f.origins.join(',');
  if (f.error) p.error = 'true';
  if (f.signal) p.signal = 'true';
  if (f.verifications.length) p.verification = f.verifications.join(',');
  if (f.tools.length) p.tool = f.tools.join(',');
  if (f.models.length) p.model = f.models.join(',');
  if (f.q.trim()) p.q = f.q.trim();
  return p;
}

const LIST_KEYS = [
  ['f_kind', 'kinds'], ['f_role', 'roles'], ['f_origin', 'origins'],
  ['f_verification', 'verifications'], ['f_tool', 'tools'], ['f_model', 'models'],
] as const;

/** URL 동기화 — 접두 f_ (react-router searchParams). q는 trim 정규화 후 왕복하며,
 *  그 외 축은 무손실. CSV 직렬화는 콤마를 이스케이프하지 않는다 — tools/models 등
 *  축 값에 콤마가 없다는 가정(kind·role·model id·도구명 모두 콤마 미포함). */
export function filterToSearch(f: FilterState, sp: URLSearchParams): void {
  for (const [key, prop] of LIST_KEYS) {
    const v = f[prop];
    if (v.length) sp.set(key, v.join(','));
    else sp.delete(key);
  }
  if (f.error) sp.set('f_error', 'true'); else sp.delete('f_error');
  if (f.signal) sp.set('f_signal', 'true'); else sp.delete('f_signal');
  if (f.q.trim()) sp.set('f_q', f.q.trim()); else sp.delete('f_q');
}

export function filterFromSearch(sp: URLSearchParams): FilterState {
  const list = (k: string) => sp.get(k)?.split(',').map((s) => s.trim()).filter(Boolean) ?? [];
  return {
    kinds: list('f_kind'), roles: list('f_role'), origins: list('f_origin'),
    error: sp.get('f_error') === 'true', signal: sp.get('f_signal') === 'true',
    verifications: list('f_verification'), tools: list('f_tool'), models: list('f_model'),
    q: sp.get('f_q') ?? '',
  };
}

/** 필터 정체성 비교(윈도우 리셋 트리거) — 직렬화 키.
 *  축 내 값 순서는 정체성과 무관하므로(같은 선택 집합) 정렬 후 직렬화한다 —
 *  Set 이터레이션·토글 순서 차이로 인한 useSessionWindow 버퍼 스퓨리어스 리셋 방지.
 *  서버 전송 순서(toEventFilterParams/filterToSearch)는 바꾸지 않는다. */
export function filterKey(f: FilterState): string {
  const p = toEventFilterParams(f);
  const sortCsv = (v?: string) =>
    v === undefined ? undefined : v.split(',').sort().join(',');
  return JSON.stringify({
    ...p,
    kind: sortCsv(p.kind), role: sortCsv(p.role), origin: sortCsv(p.origin),
    verification: sortCsv(p.verification), tool: sortCsv(p.tool), model: sortCsv(p.model),
  });
}
