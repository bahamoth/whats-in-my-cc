/**
 * TaskList — the session's tasks collected into ONE inline block in the stream
 * (the todo list, 모아보기), rows in id order. Each row:
 *   - is SELECTABLE (click → the TaskCreate event opens in the detail panel),
 *   - shows the glance: status · #id · subject · duration + work-span summary
 *     (activity · verb.object tags · diff · verification 4-outcome · tokens),
 *   - is FOLDABLE only when the list is well-tracked and the task has nested
 *     work (every task had an in_progress span). The raw `description` is not
 *     shown — it is unstructured prose, not the work.
 *
 * Attribution is per-task and marker-driven: a task with an in_progress marker
 * nests the main-chain work in its window; a task without one (e.g. created →
 * completed directly) owns no work and carries an honest "no in_progress" flag.
 * We never guess which work was an untracked task's — that is retrospect's job.
 */
import { useState, type ReactNode } from 'react';
import { ChevronDown, ChevronRight, ListTodo } from 'lucide-react';
import type { TaskDto } from '../../../api/types';
import { useT } from '../../../i18n';
import type { StreamItem, TaskListRow } from './streamModel';
import styles from './TaskList.module.css';

interface Props {
  rows: TaskListRow[];
  selectedEventId?: string | null;
  /** task ids whose nested work contains the selected event → open by default. */
  autoOpenTaskIds?: Set<string>;
  onSelect: (eventId: string) => void;
  renderChild: (item: StreamItem) => ReactNode;
}

function fmtDuration(ms: number | null): string {
  if (ms == null) return '';
  const s = Math.round(ms / 1000);
  const m = Math.floor(s / 60);
  return m > 0 ? `${m}m ${s % 60}s` : `${s}s`;
}

/** input + output + cache_creation (billed-ish; cache_read is mostly free). */
function fmtTokens(t: TaskDto['tokens']): string | null {
  if (!t) return null;
  const billed = t.input + t.output + t.cache_creation;
  if (billed >= 1_000_000) return `${(billed / 1_000_000).toFixed(1)}M tok`;
  if (billed >= 1000) return `${Math.round(billed / 1000)}k tok`;
  return `${billed} tok`;
}

const verbOf = (tag: string) => tag.slice(0, tag.indexOf('.') >= 0 ? tag.indexOf('.') : tag.length);
const STATUS_ICON: Record<string, string> = {
  completed: '✓',
  in_progress: '■',
  created: '○',
  pending: '○',
};

export function TaskList({
  rows,
  selectedEventId,
  autoOpenTaskIds,
  onSelect,
  renderChild,
}: Props) {
  const t = useT();
  const [override, setOverride] = useState<Record<string, boolean>>({});
  if (rows.length === 0) return null;

  const doneCount = rows.filter((r) => r.task.status === 'completed').length;

  return (
    <section className={styles.block} aria-label={t('detail.taskBoard.title')}>
      <div className={styles.head}>
        <ListTodo size={13} aria-hidden className={styles.headIcon} />
        <span className={styles.title}>{t('detail.taskBoard.title')}</span>
        <span className={styles.count}>
          {rows.length} · {t('detail.taskBoard.done')} {doneCount}
        </span>
      </div>

      {rows.map((row) => {
        const { task, work } = row;
        const expandable = work.length > 0;
        const isOpen = override[task.task_id] ?? autoOpenTaskIds?.has(task.task_id) ?? false;
        const selected = selectedEventId != null && task.event_id === selectedEventId;
        const label =
          task.status === 'in_progress' && task.active_form ? task.active_form : task.subject;
        const v = task.verification;
        const tokens = fmtTokens(task.tokens);

        return (
          <div
            key={task.task_id}
            data-testid="task-row"
            data-task-id={task.task_id}
            data-expanded={String(isOpen && expandable)}
            className={`${styles.row}${selected ? ` ${styles.selected}` : ''}`}
          >
            <div className={styles.rhead}>
              {expandable ? (
                <button
                  type="button"
                  className={styles.caret}
                  aria-expanded={isOpen}
                  aria-label="toggle work"
                  onClick={() => setOverride((p) => ({ ...p, [task.task_id]: !isOpen }))}
                >
                  {isOpen ? <ChevronDown size={13} aria-hidden /> : <ChevronRight size={13} aria-hidden />}
                </button>
              ) : (
                <span className={styles.caretSpacer} aria-hidden />
              )}
              <button type="button" className={styles.rsel} onClick={() => onSelect(task.event_id)}>
                <span className={styles.titleline}>
                  <span className={`${styles.st} ${styles[task.status] ?? ''}`}>
                    {STATUS_ICON[task.status] ?? '·'}
                  </span>
                  <span className={styles.id}>#{task.task_id}</span>
                  <span className={styles.subj}>{label}</span>
                  <span className={styles.dur}>
                    {fmtDuration(task.work_duration_ms ?? task.duration_ms)}
                  </span>
                </span>
                <span className={styles.outcome}>
                  {task.saw_in_progress ? (
                    <>
                      {task.activity_count != null && (
                        <span className={styles.dim}>{task.activity_count} calls</span>
                      )}
                      {task.tag_histogram.slice(0, 5).map((h) => (
                        <span key={h.tag} className={styles.tagchip} data-verb={verbOf(h.tag)}>
                          {h.tag} {h.count}
                        </span>
                      ))}
                      {(task.lines_added ?? 0) + (task.lines_removed ?? 0) > 0 && (
                        <span className={styles.dim}>
                          +{task.lines_added ?? 0}/−{task.lines_removed ?? 0}
                        </span>
                      )}
                      {v && (
                        <span className={styles.verif} title="passed · failed · unknown · not_executed">
                          <span className={styles.vk}>test</span>
                          <span className={`${styles.v_pass}${v.passed ? '' : ` ${styles.zero}`}`}>✓{v.passed}</span>
                          <span className={`${styles.v_fail}${v.failed ? '' : ` ${styles.zero}`}`}>✗{v.failed}</span>
                          <span className={`${styles.v_unk}${v.unknown ? '' : ` ${styles.zero}`}`}>?{v.unknown}</span>
                          <span className={`${styles.v_nx}${v.not_executed ? '' : ` ${styles.zero}`}`}>⊘{v.not_executed}</span>
                        </span>
                      )}
                      {tokens && <span className={styles.dim}>{tokens}</span>}
                    </>
                  ) : (
                    <span className={styles.flag}>{t('detail.taskBoard.noInProgress')}</span>
                  )}
                </span>
              </button>
            </div>

            {isOpen && expandable && (
              <div className={styles.body}>
                <div className={styles.timeline}>
                  {task.transitions.map((tr) => (
                    <span key={tr.event_id + tr.status} className={styles.tstep}>
                      <b>{tr.status}</b> {new Date(tr.at_ms).toLocaleTimeString()}
                    </span>
                  ))}
                </div>
                <div className={styles.work}>
                  {work.map((it) => (
                    <div key={it.id} data-selected={String(it.id === selectedEventId)}>
                      {renderChild(it)}
                    </div>
                  ))}
                </div>
              </div>
            )}
          </div>
        );
      })}
    </section>
  );
}
