/** 지시문 카드 (스펙 §2 4차 개정) — 세션에서 전향 관측된 지시문 파일들.
 *  source별 최신 해시 + 세션 중 변경 횟수를 상시 노출하고, 변경 항목을
 *  클릭하면 내용 diff를 렌더한다. 관측이 없는 세션(과거·serve 미가동)은
 *  렌더하지 않는다 — 추정으로 메꾸지 않는다. */
import { useEffect, useMemo, useState } from 'react';
import { getSessionInstructions } from '../../api/client';
import type { InstructionObservationDto } from '../../api/types';
import { InstructionDiff } from '../dash/InstructionDiff';
import { InfoTip } from './insight-strip/InfoTip';
import { useT } from '../../i18n';

export function InstructionCard({ sessionId }: { sessionId: string }) {
  const t = useT();
  const [obs, setObs] = useState<InstructionObservationDto[]>([]);
  const [openKey, setOpenKey] = useState<string | null>(null);
  useEffect(() => {
    let alive = true;
    setObs([]);
    setOpenKey(null);
    getSessionInstructions(sessionId)
      .then((rows) => {
        if (alive) setObs(rows);
      })
      .catch(() => {
        /* 관측 없음/구서버 — 카드 미렌더 */
      });
    return () => {
      alive = false;
    };
  }, [sessionId]);

  /** (source,path)별 시간순 관측 — 연속 항목 쌍이 diff의 전/후가 된다. */
  const groups = useMemo(() => {
    const m = new Map<string, InstructionObservationDto[]>();
    for (const o of obs) {
      const k = `${o.source}|${o.path}`;
      if (!m.has(k)) m.set(k, []);
      m.get(k)!.push(o);
    }
    return [...m.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }, [obs]);

  if (obs.length === 0) return null;
  const changes = groups.reduce((a, [, g]) => a + Math.max(0, g.length - 1), 0);

  return (
    <section className="mt-2 rounded-[13px] border border-(--wimcc-border) bg-(--wimcc-surface-1) px-4 py-3">
      <div className="mb-1.5 flex items-baseline gap-2">
        <span className="flex items-center gap-1 text-[12.5px] font-semibold">
          {t('instr.card.title')}
          <InfoTip label={t('instr.card.title')} text={t('instr.card.tip')} />
        </span>
        {changes > 0 && (
          <span className="rounded-[5px] bg-[#b07dff]/10 px-1.5 py-0.5 font-mono text-[10.5px] text-[#b07dff]">
            {t('instr.card.changes', changes)}
          </span>
        )}
      </div>
      <div className="flex flex-wrap gap-x-5 gap-y-1">
        {groups.map(([key, g]) => {
          const latest = g[g.length - 1];
          const changed = g.length > 1;
          return (
            <button
              key={key}
              type="button"
              disabled={!changed}
              onClick={() => setOpenKey(openKey === key ? null : key)}
              className={`font-mono text-[11px] ${
                changed
                  ? 'cursor-pointer text-(--wimcc-fg) underline decoration-dotted underline-offset-2'
                  : 'cursor-default text-(--wimcc-fg-muted)'
              }`}
              title={`${latest.path} · ${t('instr.card.observedAt', latest.observed_at.slice(5, 16))}`}
            >
              <span className="text-(--wimcc-fg-subtle)">{latest.source}:</span>
              {latest.content_sha256.slice(0, 8)}
              {changed && <span className="ml-1 text-[#b07dff]">×{g.length}</span>}
            </button>
          );
        })}
      </div>
      {openKey &&
        (() => {
          const g = groups.find(([k]) => k === openKey)?.[1];
          if (!g || g.length < 2) return null;
          const prev = g[g.length - 2];
          const cur = g[g.length - 1];
          return (
            <InstructionDiff
              source={cur.source}
              beforeHash={prev.content_sha256}
              afterHash={cur.content_sha256}
            />
          );
        })()}
    </section>
  );
}
