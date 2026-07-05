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
import { useEffect, useMemo, useRef, useState, type KeyboardEvent } from 'react';
import { useT } from '../../../i18n';
import type { FilterState } from './filterState';
import { EMPTY_FILTER, isFilterActive } from './filterState';
import { mcpServerOf, parseMcpName } from './nodeLabel';
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
  /** 세션에 등장한 도구명(2026-07-05 발견성 개선 — turns.tool_histogram 합산).
   *  MCP 도구(`mcp__<server>__…`)는 서버별 그룹으로 묶어 원클릭 토글을 붙인다. */
  availableTools?: string[];
  /** 세션에서 관측된 모델 id(usage.by_model). */
  availableModels?: string[];
  /** 세션 등장 태그(verb.object — turns.tag_histogram 합산, tag 축 후보). */
  availableTags?: string[];
}

export function FilterBar({
  filter,
  onChange,
  matchedCount,
  notice,
  availableTools,
  availableModels,
  availableTags,
}: FilterBarProps) {
  const t = useT();
  const [qDraft, setQDraft] = useState(filter.q);

  // 외부에서 filter.q가 바뀌면(예: URL 복원, 전체 해제) 드래프트를 동기화한다.
  useEffect(() => setQDraft(filter.q), [filter.q]);

  // 디바운스 콜백이 읽을 최신 filter/onChange를 ref로 잡아둔다 — deps에 넣지
  // 않아도(=타이머 재시작 없이) 항상 최신값을 읽게 한다. 매 렌더 갱신.
  const filterRef = useRef(filter);
  const onChangeRef = useRef(onChange);
  filterRef.current = filter;
  onChangeRef.current = onChange;

  // 디바운스 300ms — deps는 [qDraft]만. filter/onChange를 deps에 넣으면 매
  // 렌더마다 타이머가 재시작돼 타이핑을 멈추기 전엔 발화하지 않는 버그가 된다.
  // 대신 콜백은 ref.current로 최신 filter를 읽는다: 타이머 예약 시점의 filter를
  // 클로저로 캡처하면, 300ms 안에 칩/토글 클릭으로 filter가 바뀌었을 때 stale한
  // pre-click filter로 onChange가 발화해 그 토글을 조용히 되돌리는 데이터 손실이
  // 난다(coordinator 리뷰 Important). ref로 읽으면 최신 filter 위에 q만 얹는다.
  useEffect(() => {
    if (qDraft === filterRef.current.q) return;
    const id = setTimeout(() => onChangeRef.current({ ...filterRef.current, q: qDraft }), 300);
    return () => clearTimeout(id);
  }, [qDraft]);

  const [toolDraft, setToolDraft] = useState('');
  const [modelDraft, setModelDraft] = useState('');

  // 세션 등장 도구를 비-MCP / MCP(서버별 그룹)로 나눈다. 그룹 토글은 축 내
  // CSV OR라 "이 서버(예: serena) 도구 전부 모아보기"가 원클릭이 된다.
  const { plainTools, mcpGroups } = useMemo(() => {
    const plain: string[] = [];
    const groups = new Map<string, string[]>();
    for (const name of availableTools ?? []) {
      const server = mcpServerOf(name);
      if (server) {
        const list = groups.get(server) ?? [];
        list.push(name);
        groups.set(server, list);
      } else {
        plain.push(name);
      }
    }
    return { plainTools: plain, mcpGroups: [...groups.entries()] };
  }, [availableTools]);

  const toggleGroup = (groupTools: string[]) => {
    const allOn = groupTools.every((v) => filter.tools.includes(v));
    const tools = allOn
      ? filter.tools.filter((v) => !groupTools.includes(v))
      : [...filter.tools, ...groupTools.filter((v) => !filter.tools.includes(v))];
    onChange({ ...filter, tools });
  };

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

        {(availableTags ?? []).length > 0 && (
          <details className={styles.dropdown}>
            <summary className={styles.summary}>
              {t('filter.axis.tag')}
              {filter.tags.length > 0 && <span className={styles.badge}>{filter.tags.length}</span>}
            </summary>
            <div className={styles.menu}>
              {(availableTags ?? []).map((v) => (
                <button
                  key={v}
                  type="button"
                  aria-pressed={filter.tags.includes(v)}
                  className={styles.option}
                  onClick={() => onChange({ ...filter, tags: toggle(filter.tags, v) })}
                >
                  {v}
                </button>
              ))}
            </div>
          </details>
        )}

        <details className={styles.dropdown}>
          <summary className={styles.summary}>
            {t('filter.axis.content')}
            {filter.tools.length + filter.models.length > 0 && (
              <span className={styles.badge}>{filter.tools.length + filter.models.length}</span>
            )}
          </summary>
          <div className={styles.menu}>
            {plainTools.length > 0 && (
              <>
                <span className={styles.groupLabel}>{t('filter.content.toolsInSession')}</span>
                {plainTools.map((v) => (
                  <button
                    key={v}
                    type="button"
                    aria-pressed={filter.tools.includes(v)}
                    className={styles.option}
                    onClick={() => onChange({ ...filter, tools: toggle(filter.tools, v) })}
                  >
                    {v}
                  </button>
                ))}
              </>
            )}
            {mcpGroups.map(([server, groupTools]) => (
              <div key={server}>
                <button
                  type="button"
                  data-testid={`mcp-group-${server}`}
                  aria-pressed={groupTools.every((v) => filter.tools.includes(v))}
                  className={styles.groupToggle}
                  onClick={() => toggleGroup(groupTools)}
                  title={t('filter.content.mcpGroupTip', server)}
                >
                  mcp: {server} ({groupTools.length})
                </button>
                {groupTools.map((v) => (
                  <button
                    key={v}
                    type="button"
                    aria-pressed={filter.tools.includes(v)}
                    className={styles.option}
                    onClick={() => onChange({ ...filter, tools: toggle(filter.tools, v) })}
                  >
                    {parseMcpName(v)?.tool ?? v}
                  </button>
                ))}
              </div>
            ))}
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
            {(availableModels ?? []).length > 0 && (
              <>
                <span className={styles.groupLabel}>{t('filter.content.modelsInSession')}</span>
                {(availableModels ?? []).map((v) => (
                  <button
                    key={v}
                    type="button"
                    aria-pressed={filter.models.includes(v)}
                    className={styles.option}
                    onClick={() => onChange({ ...filter, models: toggle(filter.models, v) })}
                  >
                    {v}
                  </button>
                ))}
              </>
            )}
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
        {filter.tags.map((v) => (
          <button key={`tg:${v}`} type="button" className={styles.chip} onClick={() => onChange({ ...filter, tags: filter.tags.filter((x) => x !== v) })}>
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
