// webui/src/components/replay/stream/TeamLinkContext.tsx
//
// teammate 응답 카드 → 그 팀메이트 세션 replay로 점프하는 링크의 데이터원.
// agent_name → session_id 매핑을 SessionDetailPage가 세션 목록(team 필드)
// 클라이언트 조인으로 만들어 내려준다. 매핑이 없으면(구버전 서버·목록 미도착)
// 카드가 라벨만 보여주고 링크를 생략한다 — 단정 불가한 조인을 만들지 않는다.
import { createContext, useContext, type ReactNode } from 'react';

const TeamLinkContext = createContext<Record<string, string>>({});

export function TeamLinkProvider({
  value,
  children,
}: {
  value: Record<string, string>;
  children: ReactNode;
}) {
  return <TeamLinkContext.Provider value={value}>{children}</TeamLinkContext.Provider>;
}

/** teammate 이름의 세션 id — 매핑이 없으면 null. */
export function useTeammateSessionId(name: string | null | undefined): string | null {
  const map = useContext(TeamLinkContext);
  return (name && map[name]) || null;
}
