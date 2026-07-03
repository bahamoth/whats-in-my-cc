-- teammate 세션 필드 (2026-07-03): CC 2.1.198부터 Agent 툴의 named 스폰이
-- 별도 최상위 세션("teammate")을 만든다 — 자기 sessionId + envelope 필드
-- agentName/teamName (실측 동결: tests/fixtures/transcripts/real/teammate_v01,
-- 표본 1 세션). 세션 목록 그룹핑·리드 세션 조인을 위해 correlation 컬럼으로
-- 승격한다. 두 필드는 세션 내 상수로 관측됐고 user/assistant/attachment/system
-- 레코드에 붙는다.
--
-- startup backfill 없음: envelope 필드라 observed payload에 보존되지 않아
-- 재ingest로만 채워진다. 신방식 첫 관측이 2026-07-03이라 그 이전 세션에는
-- 원래 존재하지 않는 필드다 — 기존 행 NULL이 곧 정확한 값이다.
ALTER TABLE observed_event ADD COLUMN agent_name TEXT;
ALTER TABLE observed_event ADD COLUMN team_name TEXT;

-- 세션 단위 식별 facet (perf-2026-06-29 패턴): recompute_session이 채우고
-- GET /v1/sessions가 읽는다. 팀메이트가 아닌 세션은 NULL.
ALTER TABLE session_summary ADD COLUMN agent_name TEXT;
ALTER TABLE session_summary ADD COLUMN team_name TEXT;
