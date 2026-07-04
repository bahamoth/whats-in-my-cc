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

/** URL 동기화 — 접두 f_ (react-router searchParams). 왕복 무손실. */
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

/** 필터 정체성 비교(윈도우 리셋 트리거) — 직렬화 키 */
export function filterKey(f: FilterState): string {
  return JSON.stringify(toEventFilterParams(f));
}
