# Episode / Phase 전체 제거 Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** episode/phase 분류 시스템과 missing_verification 추출기를 전면 제거하되, 메시지 뷰의 activity-run 접기·fold/facet·verification_run·diff_hunk은 보존한다.

**Architecture:** 삭제 위주. 컴파일 그린 유지를 위해 **소비자 제거 → 모듈/테이블 삭제 → 신규 회귀 가드** 순서. 프론트는 백엔드와 독립이라 먼저 처리 가능. 각 task는 자체로 빌드·테스트 그린 상태를 만든다.

**Tech Stack:** Rust (axum, sqlx, SQLite), TypeScript/React (Vite, Vitest), 자기완결 HTML 사양서.

**Branch:** `episode-phase-removal` (= slice 스택 #26/#28/#29 + #27 머지 위에 스택). 설계: `docs/superpowers/specs/2026-05-31-episode-phase-removal-design.md`.

---

### Task 1: 프론트엔드 — 컴포넌트/라우트에서 episode/phase 제거 (activity-run 유지)

**Files:**
- Delete: `webui/src/routes/episodePhase.ts`, `webui/src/routes/__tests__/episodePhase.test.ts`
- Delete: `webui/src/components/replay/EpisodeStrip.tsx`, `webui/src/components/replay/__tests__/EpisodeStrip.test.tsx`
- Modify: `webui/src/routes/SessionDetailPage.tsx` (10,19,35,54,177-192,348,386), `webui/src/components/replay/detail/NodeDetail.tsx` (10,168,176,194), `webui/src/components/replay/timeline/Timeline.tsx` (8,22,33,51-,episode band render), `webui/src/components/replay/insight-strip/InsightStrip.tsx` (6,9 주석)
- Modify tests: `Timeline.test.tsx`, `NodeDetail.test.tsx`, `SessionDetailPage.test.tsx`

- [ ] **Step 1: 삭제 — phaseAt / EpisodeStrip 파일 + 테스트**

`episodePhase.ts`, `episodePhase.test.ts`, `EpisodeStrip.tsx`, `EpisodeStrip.test.tsx` 4개 삭제.

- [ ] **Step 2: SessionDetailPage.tsx 에서 phase 배지·EpisodeStrip 제거 (activity-run 렌더는 유지)**

제거: `import { EpisodeStrip }`(10행), `useEpisodesQuery` import(19행), `import { phaseAt }`(35행), `const episodes = useEpisodesQuery(sessionId)`(54행), `phaseByEventId` useMemo와 그 콜백(177-192행), `<EpisodeStrip episodes={...} />`(348행), 하위 컴포넌트에 넘기던 `episodes={episodes.data ?? []}` prop(386행) 및 `phaseByEventId`/phase 콜백 prop 전달부. **stream/activity-run 자체 렌더는 손대지 않는다.**

- [ ] **Step 3: NodeDetail.tsx 에서 episodePhase 행 제거**

제거: `episodePhase: string | null;`(10행), 함수 시그니처의 `episodePhase`(168행), `if (episodePhase) rows.push(['episode', episodePhase]);`(176행), `k === 'episode' ? <span className={styles.phase}>{v}</span> : v`(194행)을 `v`로 단순화. 호출처(SessionDetailPage)에서 넘기던 `episodePhase` prop도 제거.

- [ ] **Step 4: Timeline.tsx 에서 episode band 제거**

제거: `EpisodeDto` import(8행), `episodes: EpisodeDto[]` prop(22행), `EPISODE_HEIGHT`(33행), `PHASE_COLORS`(51행~), episode band를 그리는 JSX/레이아웃 계산. 노드/엣지/timescale 렌더는 유지.

- [ ] **Step 5: InsightStrip.tsx 주석 갱신**

6·9행의 "phase bar (EpisodeStrip) stays" 류 주석을 현실(EpisodeStrip 제거됨)에 맞게 수정. 기능 변경 없음.

- [ ] **Step 6: 관련 vitest 갱신 — episode 단언 제거**

`Timeline.test.tsx`(episodes prop), `NodeDetail.test.tsx`(episodePhase), `SessionDetailPage.test.tsx`(phase 배지/EpisodeStrip)에서 episode 관련 단언·props 제거. `buildStreamModel.test.ts`는 손대지 않음(144행 매치는 한국어 텍스트 우연).

- [ ] **Step 7: 빌드·테스트 확인**

Run: `cd webui && npx vitest run && npx tsc --noEmit`
Expected: 모두 PASS, tsc exit 0. (api 레이어의 EpisodeDto는 아직 남아 있어도 OK — Task 2에서 제거.)

- [ ] **Step 8: Commit**

```bash
git add webui/src
git commit -m "refactor(webui): remove episode/phase badge + EpisodeStrip (keep activity-run fold)"
```

---

### Task 2: 프론트엔드 — api 레이어에서 episode 타입/쿼리 제거

**Files:**
- Modify: `webui/src/api/types.ts` (126-), `webui/src/api/client.ts` (8,67,77-78), `webui/src/lib/queries.ts` (14,27,45,79-81)
- Modify tests: `webui/src/api/__tests__/client.endpoints.test.ts`, `types.contract.test.ts`, `webui/src/lib/__tests__/queries.test.tsx`, `sse.test.tsx`

- [ ] **Step 1: api/types.ts 에서 EpisodeDto 제거**

`EpisodeDto` 타입 정의(126행~) 삭제.

- [ ] **Step 2: api/client.ts 에서 getEpisodes 제거**

`EpisodeDto` import(8행), `/episodes` 주석(67행), `getEpisodes`(77-78행) 삭제.

- [ ] **Step 3: lib/queries.ts 에서 useEpisodesQuery 제거**

`getEpisodes`/`EpisodeDto` import(14,27행), `sessionKeys.episodes`(45행), `useEpisodesQuery`(79-81행) 삭제.

- [ ] **Step 4: 관련 테스트에서 episode 엔드포인트/쿼리 단언 제거**

`client.endpoints.test.ts`(getEpisodes/episodes 엔드포인트), `types.contract.test.ts`(EpisodeDto), `queries.test.tsx`(useEpisodesQuery), `sse.test.tsx`(episodes 무효화) 에서 episode 관련 케이스 제거.

- [ ] **Step 5: 빌드·테스트 확인**

Run: `cd webui && npx vitest run && npx tsc --noEmit`
Expected: 모두 PASS, tsc exit 0.

- [ ] **Step 6: Commit**

```bash
git add webui/src
git commit -m "refactor(webui): drop EpisodeDto type + episodes API client/query"
```

---

### Task 3: 백엔드 — episode 소비자 제거 (missing_verification·build.rs·view.rs·routes·dto·risky_action)

**Files:**
- Delete: `src/insight/extractors/missing_verification.rs`, `tests/extractor_missing_verification.rs`
- Modify: `src/insight/registry.rs` (11,21), `src/insight/pipeline.rs` (304,310), `src/graph/build.rs` (6,13,50-117), `src/insight/view.rs` (6,8,17-20,28-31,41-48,60-61), `src/api/routes.rs` (658-705), `src/api/mod.rs` (128-131), `src/api/dto.rs` (episode DTO ~189), `src/insight/extractors/risky_action.rs` (108,162)

- [ ] **Step 1: missing_verification 추출기 + 등록 제거**

`src/insight/extractors/missing_verification.rs` 삭제. `registry.rs`에서 import(11행)·`Box::new(MissingVerification)`(21행) 제거. `pipeline.rs`에서 import(304행)·`Box::new(MissingVerification)`(310행) 제거. `tests/extractor_missing_verification.rs` 삭제.

- [ ] **Step 2: build.rs 에서 episode 분류 블록 제거**

`src/graph/build.rs`: `use crate::db::repo_episode;`(6행), `use crate::insight::episode::classifier::classify_session;`(13행) 제거. doc 주석(25-29행)에서 episode 언급 정리. **50행 `// Slice-12 — episode classification` 부터 117행 `match` 블록 끝까지 전체 삭제** (classify_session·catch_unwind·delete_session·insert 루프). 반환값 `(usize,usize,usize)`(nodes,edges,findings)은 episode와 무관하므로 그대로.

- [ ] **Step 3: view.rs 에서 episodes 제거 (verification_runs는 유지)**

`src/insight/view.rs`: `repo_episode` import(6행에서 제거, repo_verification_run 등 유지), `use crate::db::repo_episode::EpisodeRow;`(8행) 제거, borrowed struct의 `pub episodes`(20행)·owned struct의 `pub episodes`(31행)·`repo_episode::list_session`(42행)·assignment(48행)·borrow(61행) 제거.

- [ ] **Step 4: routes.rs + mod.rs 에서 episode 엔드포인트 제거**

`src/api/routes.rs`: `session_episodes`(658-680행)·`episode_detail`(683-705행) 핸들러 삭제. `src/api/mod.rs`: `.route("/v1/sessions/:id/episodes", ...)`(128-129행)·`.route("/v1/episodes/:id", ...)`(131행) 제거.

- [ ] **Step 5: dto.rs episode DTO + risky_action 라벨 제거**

`src/api/dto.rs`: episode DTO struct(~189행 `episode_id` 등) 삭제. `src/insight/extractors/risky_action.rs`: evidence projection의 `"episode_phase": "action"`(108,162행) 키 제거.

- [ ] **Step 6: 빌드·테스트 확인 (episode 모듈은 아직 존재, 미사용)**

Run: `cargo build 2>&1 | tail -5`
Expected: 컴파일 성공(episode 모듈/repo_episode는 미사용 상태로 잔존, pub이라 경고 없음).
Run: `cargo test --all 2>&1 | grep -E "test result|error" | tail`
Expected: 기존 episode_*·api_episodes·migration_episode 테스트는 아직 모듈/테이블이 있어 green.

- [ ] **Step 7: Commit**

```bash
git add src
git commit -m "refactor(insight): remove episode consumers (missing_verification, build classify, view, routes, dto)"
```

---

### Task 4: 백엔드 — episode 모듈/repo/테이블 삭제 + 고아 테스트 정리 + drop 마이그레이션

**Files:**
- Delete: `src/insight/episode/` (classifier.rs, types.rs, rules.rs, mod.rs), `src/db/repo_episode.rs`
- Delete tests: `tests/episode_classifier_basic.rs`, `episode_determinism.rs`, `episode_drift_no_overlap.rs`, `episode_gold.rs`, `episode_gold_count_invariant.rs`, `episode_no_overlap_real.rs`, `episode_rebuild_no_accumulation.rs`, `episode_rebuild_writes_rows.rs`, `episode_rule_registry.rs`, `api_episodes.rs`, `migration_episode_schema.rs`
- Modify: `src/insight/mod.rs` (2), `src/db/mod.rs` (3)
- Create: `migrations/20260606120000_0017_drop_episode.sql`, `tests/episode_removed.rs`

- [ ] **Step 1: 회귀 가드 테스트 먼저 작성 (red 우선)**

Create `tests/episode_removed.rs` — 기존 `tests/api.rs`의 앱 스폰 헬퍼 패턴을 따라:
```rust
//! Locks the episode/phase removal: the endpoints are gone (404) and the
//! episode table no longer exists after migrations.
mod common; // if a shared harness exists; otherwise inline the spawn like tests/api.rs

#[tokio::test]
async fn episodes_list_endpoint_is_gone() {
    // spawn app on an in-memory/temp DB exactly as tests/api.rs does, then:
    // GET /v1/sessions/any/episodes  -> 404 NOT_FOUND
    // (assert the StatusCode is 404, mirroring tests/api.rs request style)
}

#[tokio::test]
async fn episode_detail_endpoint_is_gone() {
    // GET /v1/episodes/any -> 404 NOT_FOUND
}

#[tokio::test]
async fn episode_table_absent_after_migrations() {
    // init a temp DB + run migrations (as tests/db_init.rs does), then query
    // sqlite_master: SELECT count(*) FROM sqlite_master WHERE type='table' AND name='episode'
    // assert == 0.
}
```
구체 스폰/마이그레이션 헬퍼는 `tests/api.rs`·`tests/db_init.rs`에서 그대로 차용(삭제 전에 패턴 확인). 라우트·테이블이 아직 있으므로 이 시점엔 **404 단언이 FAIL(=red), table 단언도 FAIL**.

- [ ] **Step 2: red 확인**

Run: `cargo test --test episode_removed 2>&1 | tail`
Expected: FAIL (엔드포인트가 아직 존재 → 404 아님; 테이블 아직 존재).

- [ ] **Step 3: episode 모듈·repo·mod 선언 삭제**

`src/insight/episode/` 디렉터리 전체 삭제, `src/db/repo_episode.rs` 삭제. `src/insight/mod.rs`의 `pub mod episode;`(2행) 제거, `src/db/mod.rs`의 `pub mod repo_episode;`(3행) 제거.

- [ ] **Step 4: 고아 테스트 삭제**

위 Files의 11개 테스트 파일 삭제 (episode 모듈/테이블/엔드포인트를 참조해 더는 컴파일 안 됨).

- [ ] **Step 5: drop 마이그레이션 추가**

Create `migrations/20260606120000_0017_drop_episode.sql`:
```sql
-- Remove the episode side-table. Episode/phase classification was removed;
-- the message view's activity-run fold replaces it and needs no persistence.
DROP TABLE IF EXISTS episode;
```

- [ ] **Step 6: 빌드 + 회귀 가드 green 확인**

Run: `cargo build 2>&1 | tail -5` → 성공.
Run: `cargo test --test episode_removed 2>&1 | tail` → **PASS** (404 + 테이블 부재).
Run: `cargo test --all 2>&1 | grep -E "test result|error|FAILED" | tail -20` → 0 failed.

- [ ] **Step 7: Commit**

```bash
git add -A
git commit -m "feat(db): drop episode table + delete episode classifier module; lock removal with regression test"
```

---

### Task 5: 문서 정합 (source-of-truth 포함, deferral 금지)

**Files:**
- Modify: `docs/03_data_model_spec.html` (§6 Episode/phase 정의 제거), `docs/04_api_mcp_spec.html` (/episodes 제거), `docs/00_prd_revised.html`, `docs/02_technical_architecture_spec.html`, `docs/06_mvp_execution_plan.html`, `docs/index.html`, `docs/implementation-notes.html`, `CLAUDE.md`, `docs/superpowers/specs/2026-05-31-episode-phase-removal-design.md` (0014→0017 정정)

- [ ] **Step 1: 데이터모델·API 사양서 갱신**

`03_data_model_spec.html §6`에서 Episode 객체 + 7-phase 정의/표 제거(또는 "제거됨" 명시). `04_api_mcp_spec.html`에서 `GET /v1/sessions/:id/episodes`·`/v1/episodes/:id` 항목 제거. (HTML 텍스트 직접 편집; `python3` 텍스트 추출로 위치 확인 후 해당 섹션만 수정.)

- [ ] **Step 2: 나머지 사양서·노트 갱신**

`00_prd_revised.html`·`02_technical_architecture_spec.html`·`06_mvp_execution_plan.html`·`index.html`의 episode/phase 언급 정리. `implementation-notes.html`에 제거 결정·근거·migration 0017·init-db 주의 추가. 설계 스펙의 마이그레이션 번호 `0014`→`0017` 정정.

- [ ] **Step 3: CLAUDE.md 갱신**

episode 상태 노트(3건) 제거, "운영 주의: episode/phase + missing_verification 제거. migration 0017(DROP TABLE episode) — `witmcc init-db` 필요. 메시지 뷰 activity-run 접기·fold/facet은 유지." 추가.

- [ ] **Step 4: Commit**

```bash
git add docs CLAUDE.md
git commit -m "docs: remove episode/phase from specs (data-model §6, api), notes, CLAUDE.md"
```

---

### Task 6: 통합 검증 — 빌드·전체 테스트·브라우저 smoke

- [ ] **Step 1: 전체 회귀**

Run: `cargo test --all 2>&1 | grep -E "test result|FAILED|error" | tail -20` → 0 failed.
Run: `cd webui && npx vitest run && npx tsc --noEmit` → PASS, exit 0.

- [ ] **Step 2: 재빌드 + serve/vite 재시작 (디스크 dist 반영)**

webui dist 빌드 → `cargo build` → 기존 serve/vite 종료 후 재기동(`./target/debug/witmcc serve --bind 127.0.0.1 --port 7878`, `npm --prefix webui run dev`).

- [ ] **Step 3: 브라우저 smoke (정적 세션)**

라이브 변동 없는 정적 세션(예: 2c5d9a5a) 메시지 뷰: **메시지 사이 이벤트가 접힌 activity-run으로 보이고, phase 배지가 없으며, 콘솔에 제거된 /episodes 호출 에러가 없음** 확인. 스크린샷 저장.

- [ ] **Step 4: PR 생성**

`gh pr create` — base는 스택 부모(`episode-redesign-slice3-bugfixes`). 본문에 제거 범위·결과 상태·결정(missing_verification 삭제, fold/activity-run 유지)·후속(per-event 태그 분류기 별도) 명시. 정직한 plan-vs-done 맵 포함.

---

## Self-Review

**Spec coverage:** 스펙 §3(백엔드)→Task 3·4, §4(프론트)→Task 1·2, §5(테스트)→Task 3·4 삭제+신규, §6(문서)→Task 5, §7(결과상태)→Task 6 smoke, §8(리스크: 마이그레이션·배지없는 run·문서정합)→Task 4·5·6. 누락 없음.

**Placeholder scan:** 신규 산출물(드롭 마이그레이션 SQL, 회귀 테스트 의도/단언)은 구체 명시. 회귀 테스트의 스폰 헬퍼는 기존 `tests/api.rs`/`db_init.rs` 패턴 차용으로 지시(삭제 전 확인). 삭제 step은 코드 블록 불필요.

**Type/순서 일관성:** 컴파일 그린 순서 — 프론트(독립) → 백엔드 소비자 제거(모듈 잔존) → 모듈/테이블 삭제+고아 테스트 정리. EpisodeDto는 Task1(소비자) 후 Task2(정의)로 제거해 tsc 그린 유지. verification_run/diff_hunk/fold는 어느 task에서도 건드리지 않음.
