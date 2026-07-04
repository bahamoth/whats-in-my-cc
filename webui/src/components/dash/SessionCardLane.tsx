/** 세션 타임라인 — 실제 날짜 위치의 세션 카드 레인(스펙 §1 모듈 5).
 *  역할: 일별 집계의 귀속("그날 누가")과 작업 리듬. hover 없이 핵심 값
 *  (이름·모델 전체명·비용·신호·통과율)을 상시 노출한다. 좌표계가 없는
 *  텍스트 배치라 DOM으로 그린다(스펙 원칙 6). */
import { useMemo } from 'react';
import type { SessionSeriesRowDto } from '../../api/types';
import { laneLayout, modelColors, signalsOf, MODEL_OVERFLOW_COLOR } from '../../lib/dashDerive';
import type { CohortMarker } from './dailyOptions';
import { cohortModels, displayModel, usageRatios } from '../../lib/seriesView';
import { useT } from '../../i18n';
import { InfoTip } from '../replay/insight-strip/InfoTip';

const CARD_W_PCT = 14.7;
const LANE_H = 78;

const dayOrd = (iso: string) => Math.floor(Date.parse(iso.slice(0, 10)) / 86_400_000);
const trim1 = (v: number) => String(Math.round(v * 10) / 10);

export function SessionCardLane({
  rows,
  nameOf,
  onOpen,
  markers = [],
}: {
  rows: SessionSeriesRowDto[];
  nameOf: (sid: string) => string;
  onOpen: (sid: string) => void;
  /** 코호트 전환 마커 — dayIdx는 buildDaily와 같은 기준(첫 세션 날짜 = 0). */
  markers?: CohortMarker[];
}) {
  const t = useT();
  const colors = useMemo(() => modelColors(rows), [rows]);
  const span = useMemo(() => {
    if (rows.length === 0) return 1;
    return Math.max(
      1,
      dayOrd(rows[rows.length - 1].first_observed_at) - dayOrd(rows[0].first_observed_at),
    );
  }, [rows]);
  const cards = useMemo(() => {
    if (rows.length === 0) return [];
    const ord0 = dayOrd(rows[0].first_observed_at);
    const items = rows.map((r) => ({
      r,
      x: Math.min(((dayOrd(r.first_observed_at) - ord0) / span) * 100, 100 - CARD_W_PCT),
    }));
    const lanes = laneLayout(items, CARD_W_PCT + 0.6);
    // 신호 밀도 중앙값 — 앰버 강조 임계(중앙값×2)의 기준. 측정 세션만.
    const densities = rows
      .filter((r) => r.event_count > 0)
      .map((r) => (signalsOf(r.metrics) / r.event_count) * 100)
      .sort((a, b) => a - b);
    const median = densities.length ? densities[Math.floor(densities.length / 2)] : 0;
    return items.map(({ r, x }, i) => {
      const ratios = usageRatios(r.metrics);
      const sig = signalsOf(r.metrics);
      const dens = r.event_count > 0 ? (sig / r.event_count) * 100 : 0;
      const t2 = r.metrics.verification_passed + r.metrics.verification_failed;
      const models = cohortModels(r.fingerprint);
      // 무활동 세션(usage 미측정·신호 0·가드 0)은 고스트 — 0 나열이 "고장"처럼
      // 읽히는 것을 막는다. 프로브/synthetic 마이크로 세션이 여기 속한다.
      const ghost = !ratios.measured && sig === 0 && r.metrics.verification_total === 0;
      return {
        sid: r.session_id,
        x,
        lane: lanes[i],
        ghost,
        name: nameOf(r.session_id),
        models,
        primColor: models.length ? (colors.get(models[0]) ?? MODEL_OVERFLOW_COLOR) : MODEL_OVERFLOW_COLOR,
        cost: ratios.measured ? `$${Math.round(r.metrics.estimated_cost_usd ?? 0)}` : '—',
        sig,
        sigHot: median > 0 && dens >= 2 * median && sig > 2,
        passPct: t2 > 0 ? `${trim1((r.metrics.verification_passed / t2) * 100)}%` : '—',
        date: r.first_observed_at.slice(5, 10),
        events: r.event_count,
      };
    });
  }, [rows, nameOf, colors, span]);
  const laneCount = cards.reduce((a, c) => Math.max(a, c.lane + 1), 1);
  /** 시간축 틱 — 5~7개가 되게 일 단위 간격을 고른다(결정론). */
  const ticks = useMemo(() => {
    if (rows.length === 0) return [];
    const ord0 = dayOrd(rows[0].first_observed_at);
    const step = Math.max(1, Math.ceil(span / 6));
    const out: Array<{ pct: number; label: string }> = [];
    for (let i = 0; i <= span; i += step) {
      out.push({
        pct: (i / span) * 100,
        label: new Date((ord0 + i) * 86_400_000).toISOString().slice(5, 10),
      });
    }
    return out;
  }, [rows, span]);

  if (rows.length === 0) return null;
  return (
    <section className="mt-7">
      <div className="mb-2.5 flex items-baseline justify-between">
        <span className="text-[13.5px] font-semibold">
          {t('dash.lane.title')}
          <InfoTip label={t('dash.lane.title')} text={t('dash.lane.tip')} />
          <small className="ml-2 text-[11.5px] font-medium text-(--wimcc-fg-subtle)">
            {t('dash.lane.desc')}
          </small>
        </span>
        <span className="font-mono text-[11px] text-(--wimcc-fg-subtle)">
          {cards[0]?.date} → {cards[cards.length - 1]?.date}
        </span>
      </div>
      <div className="rounded-[13px] border border-(--wimcc-border) bg-(--wimcc-surface-1) px-4 pt-4 pb-2">
        <div className="relative" style={{ height: laneCount * LANE_H - 10 }}>
          {/* 시간축 그리드 — 틱 위치마다 세로선이 레인을 관통해 카드 위치가
              "시간"으로 읽히게 한다. 코호트 마커는 위 차트와 같은 보라 점선. */}
          {ticks.map((tk) => (
            <span
              key={tk.pct}
              aria-hidden
              className="absolute inset-y-0 w-px bg-(--wimcc-border)"
              style={{ left: `${tk.pct}%`, opacity: 0.55 }}
            />
          ))}
          {markers.map((m) => (
            <span
              key={m.label}
              aria-hidden
              title={m.label}
              className="absolute inset-y-0 w-px border-l border-dashed"
              style={{ left: `${(m.dayIdx / span) * 100}%`, borderColor: 'rgba(176,125,255,.55)' }}
            />
          ))}
          {cards.map((c) =>
            c.ghost ? (
              <div
                key={c.sid}
                role="button"
                tabIndex={0}
                data-ghost="true"
                onClick={() => onOpen(c.sid)}
                onKeyDown={(e) => e.key === 'Enter' && onOpen(c.sid)}
                title={`${c.name} · ${c.date} · ${c.events.toLocaleString()}ev`}
                className="absolute w-[108px] cursor-pointer rounded-md border border-dashed border-(--wimcc-border) px-2 py-1 opacity-55 transition-opacity hover:opacity-90"
                style={{ left: `${c.x}%`, top: c.lane * LANE_H + 14 }}
              >
                <div className="truncate font-mono text-[9.5px] text-(--wimcc-fg-subtle)">{c.name}</div>
                <div className="font-mono text-[9px] text-(--wimcc-fg-subtle)">{c.date} · {c.events}ev</div>
              </div>
            ) : (
              <div
                key={c.sid}
                role="button"
                tabIndex={0}
                onClick={() => onOpen(c.sid)}
                onKeyDown={(e) => e.key === 'Enter' && onOpen(c.sid)}
                className="absolute w-[172px] cursor-pointer rounded-lg border border-(--wimcc-border) bg-(--wimcc-surface-2) px-2.5 py-1.5 transition-transform hover:-translate-y-px hover:border-(--wimcc-border-strong)"
                style={{ left: `${c.x}%`, top: c.lane * LANE_H, borderLeftWidth: 3, borderLeftColor: c.primColor }}
              >
                <div className="truncate font-mono text-[10.5px] font-semibold" title={c.name}>
                  {c.name}
                </div>
                <div className="truncate font-mono text-[9.5px]">
                  {c.models.length ? (
                    c.models.map((m, i) => (
                      <span key={m}>
                        {i > 0 && <span className="text-(--wimcc-fg-subtle)"> · </span>}
                        <b style={{ color: colors.get(m) ?? MODEL_OVERFLOW_COLOR }}>{displayModel(m)}</b>
                      </span>
                    ))
                  ) : (
                    <span className="text-(--wimcc-fg-subtle)">{t('dash.lane.notMeasured')}</span>
                  )}
                </div>
                <div className="mt-0.5 font-mono text-[10px] text-(--wimcc-fg-muted)">
                  {c.cost} · <span className={c.sigHot ? 'font-semibold text-[#f0b429]' : ''}>{t('dash.lane.sig', c.sig)}</span> · {c.passPct}
                </div>
                <div className="font-mono text-[10px] text-(--wimcc-fg-subtle)">
                  {c.date} · {c.events.toLocaleString()}ev
                </div>
              </div>
            ),
          )}
        </div>
        <div className="relative mt-1.5 h-4 border-t border-(--wimcc-border)">
          {ticks.map((tk) => (
            <span
              key={tk.pct}
              className="absolute top-0.5 -translate-x-1/2 font-mono text-[9.5px] text-(--wimcc-fg-subtle)"
              style={{ left: `${Math.min(Math.max(tk.pct, 2), 98)}%` }}
            >
              {tk.label}
            </span>
          ))}
        </div>
      </div>
    </section>
  );
}
