/**
 * Task lifecycle board — a per-session summary of the TaskCreate/TaskUpdate
 * todos, laid out on a SHARED time axis so the whole plan reads at a glance:
 * which tasks ran when, how long each took, and which jumped to completion with
 * no observed in_progress transition. Pure correlation lives in `buildTaskBoard`.
 */
import { useMemo } from 'react';
import type { TaskBoardEntry } from '../../../lib/taskBoard';
import { useT } from '../../../i18n';
import styles from './TaskBoard.module.css';

interface Props {
  entries: TaskBoardEntry[];
  selectedEventId?: string | null;
  onSelectEvent?: (eventId: string) => void;
}

function statusKey(status: string): 'created' | 'in_progress' | 'completed' | 'other' {
  if (status === 'created' || status === 'in_progress' || status === 'completed') return status;
  return 'other';
}

function formatDuration(ms: number | null): string {
  if (ms == null) return '';
  const totalSec = Math.round(ms / 1000);
  const m = Math.floor(totalSec / 60);
  const s = totalSec % 60;
  return m > 0 ? `${m}m ${s}s` : `${s}s`;
}

export function TaskBoard({ entries, selectedEventId, onSelectEvent }: Props) {
  const t = useT();

  // Shared time axis across all tasks: domain = [earliest created, latest transition].
  const domain = useMemo(() => {
    let min = Infinity;
    let max = -Infinity;
    for (const e of entries) {
      const c = Date.parse(e.createdAt);
      if (Number.isFinite(c)) min = Math.min(min, c);
      for (const tr of e.transitions) {
        const at = Date.parse(tr.at);
        if (Number.isFinite(at)) max = Math.max(max, at);
      }
    }
    return Number.isFinite(min) && Number.isFinite(max) ? { min, max } : null;
  }, [entries]);

  if (entries.length === 0 || !domain) return null;

  const span = Math.max(domain.max - domain.min, 1);
  const pct = (iso: string) => {
    const v = Date.parse(iso);
    if (!Number.isFinite(v)) return 0;
    return ((v - domain.min) / span) * 100;
  };

  return (
    <section className={styles.board} aria-label={t('detail.taskBoard.title')}>
      <div className={styles.head}>
        <span className={styles.title}>{t('detail.taskBoard.title')}</span>
        <span className={styles.count}>{entries.length}</span>
      </div>

      <div className={styles.rows}>
        {entries.map((task) => {
          const startPct = pct(task.createdAt);
          const last = task.transitions[task.transitions.length - 1];
          const endPct = pct(last.at);
          const isSel = selectedEventId != null && task.eventId === selectedEventId;
          return (
            <button
              key={task.taskId}
              type="button"
              className={`${styles.row}${isSel ? ` ${styles.selected}` : ''}`}
              onClick={() => onSelectEvent?.(task.eventId)}
              title={task.subject}
            >
              <span className={styles.label}>
                <span className={styles.id}>#{task.taskId}</span>
                <span className={styles.subj}>{task.subject}</span>
              </span>
              <span className={styles.track}>
                <span className={styles.baseline} />
                <span
                  className={styles.segment}
                  data-final={statusKey(task.status)}
                  style={{ left: `${startPct}%`, width: `${Math.max(endPct - startPct, 0)}%` }}
                />
                {task.transitions.map((tr) => (
                  <span
                    key={tr.eventId}
                    className={styles.node}
                    data-status={statusKey(tr.status)}
                    style={{ left: `${pct(tr.at)}%` }}
                    title={`${tr.status} · ${new Date(tr.at).toLocaleTimeString()}`}
                  />
                ))}
                <span className={styles.meta}>
                  {task.durationMs != null && <span>{formatDuration(task.durationMs)}</span>}
                  {task.status === 'completed' && !task.sawInProgress && (
                    <span className={styles.flag}>{t('detail.taskBoard.noInProgress')}</span>
                  )}
                </span>
              </span>
            </button>
          );
        })}
      </div>

      <div className={styles.legend}>
        <span>
          <span className={styles.dot} style={{ background: 'var(--wimcc-accent)' }} />
          created
        </span>
        <span>
          <span
            className={styles.dot}
            style={{ background: 'var(--wimcc-warning)', borderRadius: '2px' }}
          />
          in_progress
        </span>
        <span>
          <span className={styles.dot} style={{ background: 'var(--wimcc-success)' }} />
          completed
        </span>
      </div>
    </section>
  );
}
