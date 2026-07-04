// FilterBar — 스펙 §1.4. 축별 칩 드롭다운(kind/origin/결과/도구·모델) + 텍스트
// 검색(디바운스 ≥300ms) + 활성 조건 제거형 칩 + "N건 매칭" + 전체 해제 +
// 점프-해제 알림(notice). 서버 필터 실행은 useSessionWindow/API가 담당하고
// 이 컴포넌트는 순수 controlled 입력이다(value=filter, onChange).
//
// 드롭다운 구현은 이 디렉터리(stream/)의 기존 관례(CSS module, 서드파티 UI
// 컴포넌트 미사용 — StreamLegend/AutoscrollToggle 참조)를 따라 native
// <details>/<summary>로 구현했다. dash/ 쪽의 shadcn Popover(CohortBoundaries가
// 참조 대상으로 언급됨)는 tailwind 유틸리티 클래스 전제라 이 디렉터리 관례와
// 어긋나 채택하지 않았다 — 값 다중 선택은 CohortBoundaries의 토글 버튼
// 패턴(aria-pressed)을 그대로 재사용한다(2026-07-05 편차 기록, Task 10).
import { useEffect, useState, type KeyboardEvent } from 'react';
import { useT } from '../../../i18n';
import type { FilterState } from './filterState';
import { EMPTY_FILTER, isFilterActive } from './filterState';
import styles from './FilterBar.module.css';

// repo_observed.rs RENDERED 상수와 동일 집합(순서는 UI 표시용, 스펙 §1.4/브리프).
const KIND_OPTIONS = [
  'user_message', 'assistant_message', 'thinking', 'tool_call',
  'tool_result', 'hook_event', 'system_summary', 'diff_hunk',
] as const;

// 스펙 §1.2 표.
const ORIGIN_OPTIONS = [
  'human', 'command', 'command-output', 'skill', 'system', 'notification', 'teammate',
] as const;

const VERIFICATION_OPTIONS = ['passed', 'failed', 'unknown'] as const;

function toggle(list: string[], v: string): string[] {
  return list.includes(v) ? list.filter((x) => x !== v) : [...list, v];
}

export interface FilterBarProps {
  filter: FilterState;
  onChange: (f: FilterState) => void;
  matchedCount: number | null;
  notice: string | null;
}

export function FilterBar({ filter, onChange, matchedCount, notice }: FilterBarProps) {
  const t = useT();
  const [qDraft, setQDraft] = useState(filter.q);

  // 외부에서 filter.q가 바뀌면(예: URL 복원, 전체 해제) 드래프트를 동기화한다.
  useEffect(() => setQDraft(filter.q), [filter.q]);

  // 디바운스 300ms — filter/onChange를 의도적으로 deps에서 제외한다: 포함하면
  // 매 렌더(부모가 onChange를 새 함수로 넘기거나 filter가 바뀔 때)마다 타이머가
  // 재시작되어 사용자가 타이핑을 멈추지 않는 한 절대 발화하지 않는 버그가 된다.
  useEffect(() => {
    if (qDraft === filter.q) return;
    const id = setTimeout(() => onChange({ ...filter, q: qDraft }), 300);
    return () => clearTimeout(id);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [qDraft]);

  const [toolDraft, setToolDraft] = useState('');
  const [modelDraft, setModelDraft] = useState('');

  const addOnEnter =
    (commit: (v: string) => void, reset: () => void) =>
    (e: KeyboardEvent<HTMLInputElement>) => {
      if (e.key !== 'Enter') return;
      const v = e.currentTarget.value.trim();
      if (!v) return;
      commit(v);
      reset();
    };

  // 전체 해제 — roles(이 컴포넌트가 노출하지 않는 축, URL 복원 등 외부 출처일
  // 수 있음)까지 포함해 EMPTY_FILTER로 완전 초기화한다.
  const clearAll = () => {
    setQDraft('');
    onChange(EMPTY_FILTER);
  };

  return (
    <div className={styles.bar}>
      <div className={styles.axes}>
        <span className={styles.title}>{t('filter.title')}</span>

        <details className={styles.dropdown}>
          <summary className={styles.summary}>
            {t('filter.axis.kind')}
            {filter.kinds.length > 0 && <span className={styles.badge}>{filter.kinds.length}</span>}
          </summary>
          <div className={styles.menu}>
            {KIND_OPTIONS.map((k) => (
              <button
                key={k}
                type="button"
                aria-pressed={filter.kinds.includes(k)}
                className={styles.option}
                onClick={() => onChange({ ...filter, kinds: toggle(filter.kinds, k) })}
              >
                {k}
              </button>
            ))}
          </div>
        </details>

        <details className={styles.dropdown}>
          <summary className={styles.summary}>
            {t('filter.axis.origin')}
            {filter.origins.length > 0 && <span className={styles.badge}>{filter.origins.length}</span>}
          </summary>
          <div className={styles.menu}>
            {ORIGIN_OPTIONS.map((o) => (
              <button
                key={o}
                type="button"
                aria-pressed={filter.origins.includes(o)}
                className={styles.option}
                onClick={() => onChange({ ...filter, origins: toggle(filter.origins, o) })}
              >
                {o}
              </button>
            ))}
          </div>
        </details>

        <details className={styles.dropdown}>
          <summary className={styles.summary}>
            {t('filter.axis.outcome')}
            {(filter.error ? 1 : 0) + (filter.signal ? 1 : 0) + filter.verifications.length > 0 && (
              <span className={styles.badge}>
                {(filter.error ? 1 : 0) + (filter.signal ? 1 : 0) + filter.verifications.length}
              </span>
            )}
          </summary>
          <div className={styles.menu}>
            <button
              type="button"
              aria-pressed={filter.error}
              className={styles.option}
              onClick={() => onChange({ ...filter, error: !filter.error })}
            >
              {t('filter.outcome.error')}
            </button>
            <button
              type="button"
              aria-pressed={filter.signal}
              className={styles.option}
              onClick={() => onChange({ ...filter, signal: !filter.signal })}
            >
              {t('filter.outcome.signal')}
            </button>
            <span className={styles.groupLabel}>{t('filter.outcome.verification')}</span>
            {VERIFICATION_OPTIONS.map((v) => (
              <button
                key={v}
                type="button"
                aria-pressed={filter.verifications.includes(v)}
                className={styles.option}
                onClick={() => onChange({ ...filter, verifications: toggle(filter.verifications, v) })}
              >
                {v}
              </button>
            ))}
          </div>
        </details>

        <details className={styles.dropdown}>
          <summary className={styles.summary}>
            {t('filter.axis.content')}
            {filter.tools.length + filter.models.length > 0 && (
              <span className={styles.badge}>{filter.tools.length + filter.models.length}</span>
            )}
          </summary>
          <div className={styles.menu}>
            <input
              type="text"
              className={styles.miniInput}
              placeholder={t('filter.content.toolPlaceholder')}
              aria-label={t('filter.content.toolPlaceholder')}
              value={toolDraft}
              onChange={(e) => setToolDraft(e.target.value)}
              onKeyDown={addOnEnter(
                (v) => onChange({ ...filter, tools: filter.tools.includes(v) ? filter.tools : [...filter.tools, v] }),
                () => setToolDraft(''),
              )}
            />
            <input
              type="text"
              className={styles.miniInput}
              placeholder={t('filter.content.modelPlaceholder')}
              aria-label={t('filter.content.modelPlaceholder')}
              value={modelDraft}
              onChange={(e) => setModelDraft(e.target.value)}
              onKeyDown={addOnEnter(
                (v) => onChange({ ...filter, models: filter.models.includes(v) ? filter.models : [...filter.models, v] }),
                () => setModelDraft(''),
              )}
            />
          </div>
        </details>

        <input
          type="text"
          className={styles.search}
          placeholder={t('filter.qPlaceholder')}
          value={qDraft}
          onChange={(e) => setQDraft(e.target.value)}
        />
      </div>

      <div className={styles.chips}>
        {filter.kinds.map((v) => (
          <button key={`k:${v}`} type="button" className={styles.chip} onClick={() => onChange({ ...filter, kinds: filter.kinds.filter((x) => x !== v) })}>
            {v} ×
          </button>
        ))}
        {filter.origins.map((v) => (
          <button key={`o:${v}`} type="button" className={styles.chip} onClick={() => onChange({ ...filter, origins: filter.origins.filter((x) => x !== v) })}>
            {v} ×
          </button>
        ))}
        {filter.error && (
          <button type="button" className={styles.chip} onClick={() => onChange({ ...filter, error: false })}>
            {t('filter.outcome.error')} ×
          </button>
        )}
        {filter.signal && (
          <button type="button" className={styles.chip} onClick={() => onChange({ ...filter, signal: false })}>
            {t('filter.outcome.signal')} ×
          </button>
        )}
        {filter.verifications.map((v) => (
          <button key={`v:${v}`} type="button" className={styles.chip} onClick={() => onChange({ ...filter, verifications: filter.verifications.filter((x) => x !== v) })}>
            {v} ×
          </button>
        ))}
        {filter.tools.map((v) => (
          <button key={`t:${v}`} type="button" className={styles.chip} onClick={() => onChange({ ...filter, tools: filter.tools.filter((x) => x !== v) })}>
            {v} ×
          </button>
        ))}
        {filter.models.map((v) => (
          <button key={`m:${v}`} type="button" className={styles.chip} onClick={() => onChange({ ...filter, models: filter.models.filter((x) => x !== v) })}>
            {v} ×
          </button>
        ))}
        {filter.q.trim() !== '' && (
          <button
            type="button"
            className={styles.chip}
            onClick={() => {
              setQDraft('');
              onChange({ ...filter, q: '' });
            }}
          >
            q:&quot;{filter.q}&quot; ×
          </button>
        )}

        {matchedCount !== null && <span className={styles.matched}>{t('filter.matched', matchedCount)}</span>}

        {isFilterActive(filter) && (
          <button type="button" className={styles.clearAll} onClick={clearAll}>
            {t('filter.clearAll')}
          </button>
        )}
      </div>

      {notice !== null && (
        <div role="status" className={styles.notice}>
          {notice}
        </div>
      )}
    </div>
  );
}
