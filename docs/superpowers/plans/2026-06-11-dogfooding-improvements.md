# 개밥먹기 개선점 일괄 구현 Plan

> **For agentic workers:** REQUIRED SUB-SKILL: superpowers:executing-plans (inline) — 이 세션 컨텍스트가 풍부하므로 inline 실행. 각 task RED→GREEN→commit.

**Goal:** 2026-06-11 분석에서 도출된 나머지 개선점(ingest 최적화·잔여 노이즈·turn_duration·agent_id backfill)을 구현하고 trace_id는 won't-fix로 확정, 한 PR로 묶는다.

**Architecture:** insight 재계산은 멱등이므로 ingest --all을 세션 합집합 1회로 최적화(결과 불변). 토크나이저/메트릭은 코드 변경. agent_id는 migration + backfill(init-db 회피). spec: `docs/superpowers/specs/2026-06-11-dogfooding-improvements-design.md`.

**Tech Stack:** Rust(ingest/insight/metrics/db), TypeScript(eventTags), sqlx migration.

---

### Task 1: ingest --all 세션별 1회 재계산 최적화

**Files:** Modify `src/ingest/store.rs` (sessions_touched 재계산 루프); Test `tests/ingest_all_dedup.rs` (신규) 또는 store.rs #[cfg(test)].

- [ ] **RED**: 같은 세션의 메인+subagent 두 파일을 ingest할 때 insight 재계산(run_detectors/extract_verification_runs)이 **세션당 1회**만 호출됨을 검증하는 테스트. 호출 카운트 또는 결과 동일성으로 잠금.
- [ ] **Verify RED**: 현재는 파일별 재계산(2회) → 실패.
- [ ] **GREEN**: `ingest_file`이 raw만 삽입하고 sessions_touched를 반환 → 호출부(`ingest_cmd`/`run`)에서 전체 파일 처리 후 touched 합집합 재계산. 또는 store 내부에서 재계산을 batch 분리. list_session 기반 멱등이라 결과 동일.
- [ ] **Verify GREEN**: 산출 verification_run/signal이 파일별 재계산과 비트 동일. 전체 cargo test.
- [ ] **Commit**: `perf(ingest): --all 재계산을 세션 합집합 1회로 (결과 불변, 중복 제거)`

### Task 2: ④ 잔여 Bash 토크나이저 노이즈

**Files:** Modify `webui/src/components/replay/stream/eventTags.ts`; Test `eventTags.test.ts`.

- [ ] **RED**: collectUntagged/segmenter가 서브셸 `(cd …`·`(npm …`, 루프 `break`/`continue`, 빈 토큰, 순수 플래그줄(`-u`/`-s`)을 untagged 토큰으로 surface하지 않음을 검증.
- [ ] **Verify RED**: 현재 이들이 토큰으로 나옴 → 실패.
- [ ] **GREEN**: 선두 `(` strip, CONTROL_TOKENS에 break/continue 추가, 첫 토큰이 `-`로 시작하거나 빈 세그먼트면 firstMeaningfulSegment에서 skip.
- [ ] **Verify GREEN**: vitest 전체 + `node scripts/untagged-bash.ts --all`로 노이즈 감소 확인.
- [ ] **브라우저 smoke** (UI 영향): untagged 패널 정상.
- [ ] **Commit**: `fix(webui): untagged에서 서브셸·루프제어·플래그줄·빈토큰 노이즈 제거`

### Task 3: turn_duration 완전성 (away/compact 노출)

**Files:** Modify `src/insight/metrics.rs`, `src/api/dto.rs`(노출), `tests/`(metrics).

- [ ] **RED**: SessionMetrics에 `away_summary_count`/`compact_boundary_count`가 노출되고, turn_duration_count + away + compact == user_turns(6a254a2a: 27+10+1=38)임을 잠그는 테스트.
- [ ] **Verify RED**: 현재 필드 없음 → 실패.
- [ ] **GREEN**: metrics.rs에서 system subtype away_summary/compact_boundary 카운트 집계 + SessionMetrics/DTO 노출.
- [ ] **Verify GREEN**: cargo test + 재ingest 후 6a254a2a 합 검증.
- [ ] **Commit**: `feat(insight): turn 카운트에 away_summary·compact_boundary 노출 (turn_duration 정직화)`

### Task 4: agent_id 세분 (migration + backfill)

**Files:** Create `migrations/..._0023_observed_agent_id.sql`; Modify `src/model/observed.rs`, `src/ingest/mapping.rs`, `src/db/repo_observed.rs`(insert+backfill), `src/insight/extractors/re_read.rs`; Test `tests/extractor_re_read.rs`, mapping/backfill 테스트.

- [ ] **RED (mapping)**: transcript sidechain record의 `agentId`가 `ObservedEvent.agent_id`로 파싱됨.
- [ ] **RED (re_read)**: 서로 다른 agent_id의 동일 파일·구간 재읽기가 **agent별로 분리** 발화(scope=`agent:<id>`).
- [ ] **Verify RED**: agent_id 필드/컬럼 없음 → 실패.
- [ ] **GREEN**: migration 0023(agent_id 컬럼), mapping 파싱, ObservedEvent 필드, repo insert, re_read scope = agent_id ? `agent:<id>` : (sidechain ? "sidechain" : "main").
- [ ] **GREEN (backfill)**: raw_event.payload에서 agentId 읽어 기존 observed_event.agent_id UPDATE하는 backfill (init-db 회피). CLI 또는 ingest 경로에 backfill 단계.
- [ ] **Verify GREEN**: cargo test + clippy. backfill 후 6a254a2a re_read가 agent별 분리.
- [ ] **Commit**: `feat(ingest): observed agent_id (migration+backfill) — re_read를 subagent별 세분`

### Task 5: trace_id won't-fix 확정 (문서)

**Files:** Modify `docs/implementation-notes.html` (deferred → won't-fix).

- [ ] dogfooding-deferred의 trace_id 항목을 "won't-fix, 소스 한계(transcript에 correlation trace_id 없음, OTel span/log만, timestamp 근사 비채택)"로 갱신.
- [ ] **Commit**: `docs(impl-notes): trace_id 상관 won't-fix 확정 (소스 한계)`

### 마무리

- [ ] agent_id backfill 후 두 대상 세션 + 가능하면 전체에 insight 재계산(Task 1 최적화된 경로).
- [ ] 프로덕션 serve 재시작 + 검증.
- [ ] push `dogfooding-improvements` + PR (base main).

## Self-Review

- 스펙 5항목 모두 task 매핑됨(1→Task1, 2→Task2, turn_duration→Task3, agent_id→Task4, trace_id→Task5). ✓
- backfill(init-db 회피)·won't-fix 결정 반영. ✓
- Task 간 타입 일관: agent_id 컬럼/필드/scope 명칭 통일(`agent:<id>`). ✓
