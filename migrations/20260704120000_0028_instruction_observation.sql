-- 4차 개정(스펙 §2) — instruction 전향 관측.
-- serve가 세션 활동 수신 시 cwd/user CLAUDE.md를 그 순간 직접 읽어 기록한다
-- (measured). 소급 backfill 없음: 과거 세션·serve 미가동 세션은 영원히 미측정 —
-- git 추정으로 메꾸지 않는다(dirty tree·시점 추론이 가짜 코호트 경계를 만든다).
-- serve/ingest startup backfill 불필요(관측 전용 테이블).

-- 내용 주소화 스냅샷 — 같은 내용은 한 번만. 경계 diff 렌더의 원료.
CREATE TABLE IF NOT EXISTS instruction_snapshot (
    content_sha256    TEXT PRIMARY KEY,
    content           TEXT NOT NULL,
    first_observed_at TEXT NOT NULL
);

-- 세션 ↔ 스냅샷 관측 사실. source:
--   'project' — 세션 cwd 루트 CLAUDE.md (코호트 차원 키)
--   'user'    — ~/.claude/CLAUDE.md      (코호트 차원 키)
--   'import'  — Tier1 파일 안의 @path 참조 (존재만 기록, 로드 무주장)
CREATE TABLE IF NOT EXISTS instruction_observation (
    observation_id  TEXT PRIMARY KEY,
    session_id      TEXT NOT NULL,
    source          TEXT NOT NULL,
    path            TEXT NOT NULL,
    content_sha256  TEXT NOT NULL,
    observed_at     TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_instruction_obs_session
    ON instruction_observation(session_id);
CREATE UNIQUE INDEX IF NOT EXISTS idx_instruction_obs_unique
    ON instruction_observation(session_id, source, path, content_sha256);
