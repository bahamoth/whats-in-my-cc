// webui/src/components/replay/TeamStrip.tsx
//
// 세션 상세의 팀 관계 배지 (2026-07-03). 리드 세션이면 이 세션이 스폰한
// teammate 세션 칩들을, teammate 세션이면 리드로 돌아가는 역링크(+형제 칩)를
// 보여준다. 데이터는 /v1/sessions의 team 필드 클라이언트 조인 — 조인 규칙
// (표본 1 형태, 부재·모호 시 포기)은 lib/teamGrouping이 SSOT다. 팀 관계가
// 없으면 아무것도 렌더하지 않는다.
import { Link } from 'react-router-dom';
import { Users } from 'lucide-react';
import { useSessionsListQuery } from '../../lib/queries';
import { teammatesOf, leadOf } from '../../lib/teamGrouping';
import { agentColor } from '../../lib/colorHash';
import { useT } from '../../i18n';
import styles from './TeamStrip.module.css';

export function TeamStrip({ sessionId }: { sessionId: string }) {
  const t = useT();
  const sessions = useSessionsListQuery();
  const rows = sessions.data ?? [];
  const mates = teammatesOf(rows, sessionId);
  const lead = leadOf(rows, sessionId);
  if (!mates.length && !lead) return null;

  return (
    <div className={styles.strip} data-testid="team-strip">
      <Users size={13} aria-hidden className={styles.icon} />
      {lead && (
        <Link className={styles.leadLink} to={`/sessions/${lead.session_id}`}>
          ← {t('team.lead')} {lead.slug ?? lead.session_id.slice(0, 8)}
        </Link>
      )}
      {mates.length > 0 && !lead && (
        <span className={styles.count}>{t('team.teammates', mates.length)}</span>
      )}
      {mates.map((m) => (
        <Link
          key={m.session_id}
          className={styles.chip}
          style={{ color: agentColor(m.agent_name), borderColor: agentColor(m.agent_name) }}
          to={`/sessions/${m.session_id}`}
          title={m.session_id}
        >
          {m.agent_name ?? m.session_id.slice(0, 8)}
        </Link>
      ))}
    </div>
  );
}
