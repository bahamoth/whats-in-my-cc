/** 지시문 스냅샷 diff 뷰 — 경계(전 해시 → 후 해시)의 실제 내용 변화를
 *  렌더한다(스펙 §2 4차 개정: 해시는 키, 내용이 맥락). */
import { useEffect, useState } from 'react';
import { getInstructionSnapshot } from '../../api/client';
import { lineDiff, type DiffLine } from '../../lib/lineDiff';
import { useT } from '../../i18n';

type State =
  | { kind: 'loading' }
  | { kind: 'ok'; lines: DiffLine[] }
  | { kind: 'error' };

export function InstructionDiff({
  source,
  beforeHash,
  afterHash,
}: {
  source: string;
  /** null이면 신규 파일(전무→후) 취급. */
  beforeHash: string | null;
  afterHash: string | null;
}) {
  const t = useT();
  const [state, setState] = useState<State>({ kind: 'loading' });
  useEffect(() => {
    let alive = true;
    setState({ kind: 'loading' });
    Promise.all([
      beforeHash ? getInstructionSnapshot(beforeHash) : Promise.resolve(null),
      afterHash ? getInstructionSnapshot(afterHash) : Promise.resolve(null),
    ])
      .then(([b, a]) => {
        if (!alive) return;
        setState({ kind: 'ok', lines: lineDiff(b?.content ?? '', a?.content ?? '') });
      })
      .catch(() => {
        if (alive) setState({ kind: 'error' });
      });
    return () => {
      alive = false;
    };
  }, [beforeHash, afterHash]);

  return (
    <div className="mt-2 overflow-hidden rounded-lg border border-(--wimcc-border)">
      <div className="flex items-center justify-between border-b border-(--wimcc-border) bg-(--wimcc-surface-2) px-3 py-1.5">
        <span className="font-mono text-[10.5px] text-(--wimcc-fg-muted)">
          {source} · {beforeHash ? beforeHash.slice(0, 8) : '∅'} → {afterHash ? afterHash.slice(0, 8) : '∅'}
        </span>
        <span className="font-mono text-[10px] text-(--wimcc-fg-subtle)">
          {state.kind === 'ok' &&
            t('instr.diff.counts', {
              add: state.lines.filter((l) => l.type === 'add').length,
              del: state.lines.filter((l) => l.type === 'del').length,
            })}
        </span>
      </div>
      {state.kind === 'loading' && (
        <p className="px-3 py-2 text-[11px] text-(--wimcc-fg-subtle)">{t('insight.loading')}</p>
      )}
      {state.kind === 'error' && (
        <p className="px-3 py-2 text-[11px] text-(--wimcc-danger)">{t('instr.diff.error')}</p>
      )}
      {state.kind === 'ok' && (
        <pre className="max-h-72 overflow-auto bg-(--wimcc-surface-1) px-0 py-1 font-mono text-[11px] leading-[1.6]">
          {state.lines.map((l, i) => (
            <div
              key={i}
              className={
                l.type === 'add'
                  ? 'bg-[#41c285]/10 px-3 text-[#8fdcb6]'
                  : l.type === 'del'
                    ? 'bg-[#ef4747]/10 px-3 text-[#f0a0a0]'
                    : 'px-3 text-(--wimcc-fg-subtle)'
              }
            >
              {l.type === 'add' ? '+ ' : l.type === 'del' ? '− ' : '  '}
              {l.text}
            </div>
          ))}
        </pre>
      )}
    </div>
  );
}
