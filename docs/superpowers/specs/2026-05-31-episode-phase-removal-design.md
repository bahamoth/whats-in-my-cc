# Episode / Phase 전체 제거 — Design Spec

- 날짜: 2026-05-31
- 결정 출처: 이 세션의 대화에서 확정 (사용자 지시 "B로 삭제하고 episode/phase 전체 제거 스펙 작성")
- 상위 맥락: `docs/superpowers/specs/2026-05-31-telemetry-facet-fold-and-episode-redesign-design.md`(§6 Episode 처분), `docs/superpowers/issues/2026-05-31-episode-classification-issues.md`
- **이 스펙은 "제거"에 한정**한다. 대체물(per-event 태그 분류기)은 별도 후속 스펙.

## 1. 왜 제거하는가 (실데이터로 검증된 근거)

1. **`action` phase가 신뢰 불가.** read-only 명령이 action으로 오분류된다. 실예: 노드 `nd_6b7882f679680b8e3e5363ac` = `ls -la *.sqlite* 2>/dev/null; … find . -name "*.sqlite" … | head` (순수 읽기, write 없음)인데 phase=`action`. 원인: `2>/dev/null`의 `>`와 `;`를 분류기 denylist가 mutating marker로 봐서 보수적으로 action 강제. **`2>/dev/null`(stderr 버림)과 `> file`(쓰기), `; ls`와 `; rm`을 셸 파서 없이 구분 불가** → 휴리스틱으로는 정확해질 수 없음.
2. **`diagnosis`/`repair`가 죽어 있음.** 분류기 `is_error_result`가 최상위 `payload.is_error`(실데이터 0건)를 읽는다. 실제 에러는 `payload.tool_result.is_error`(전 세션 2169건)에 있음 — L1 `tool_failure` 추출기는 올바른 경로로 정확히 2169건을 잡는다. 같은 데이터, 분류기만 틀린 경로 → 7 phase 중 2개가 영영 0건.
3. **데이터 하이라키가 틀렸다.** 분류 결과(phase)가 *이벤트*가 아니라 파생물인 *episode*에 붙어 있다. per-event 사실은 이벤트에 있어야 한다.
4. **`episode_id`는 무가치.** 읽기/조인/성능 어디에도 안 쓰임(유일한 `/episodes/{id}` 단건 라우트는 프론트 미호출, FK 없음). hot path는 `idx_episode_session_started`(session_id)로 처리. 내용 해시 id는 누적 버그(end_event_id 포함 → 라이브 rebuild마다 추가)의 원인이었음.
5. **유일 소비자 `missing_verification`는 32건짜리에 토대가 불안정.** episode 윈도잉(인플레이션·phantom intake) 위에서 돌고, 검증 체크가 "action 이후"가 아니라 "윈도우 전역"(코드와 주석 불일치)이라 false-negative 성향. → 삭제(옵션 B). 재파생에 필요한 raw 신호(diff_hunk·verification_run)는 보존되므로 나중에 가치가 필요하면 깨끗이 재파생 가능.
6. **메시지 뷰 목표와 무관.** 사용자가 원한 "메시지 사이 이벤트를 가시성 위해 접기"는 `buildStreamModel`의 activity-run 그룹핑이 이미 한다(episode/phase 불필요). episode가 메시지 뷰에 닿는 유일한 지점은 phase 배지이고, 그건 원치 않는 "하나의 의미"라서 제거 대상.

## 2. 결정 (확정됨)

- **`episode` + `phase`를 전면 삭제**: 테이블·분류기 모듈·repo·API·프론트·테스트·마이그레이션·docs.
- **`missing_verification` 추출기 삭제** (옵션 B). diff_hunk·verification_run 데이터는 보존.
- **유지**: `buildStreamModel`의 activity-run 그룹핑(phase 배지만 제거), telemetry fold/facet(Slice 1/2 — graph/detail 패널용), `verification_run`·`diff_hunk` 서브시스템, 나머지 추출기(tool_failure·context_bloat·final_state_mismatch·risky_action).
- **범위 외 (후속 별도 스펙)**: per-event 태그 분류기(Read→code/docs, Bash→search/test/build/edit/vcs/destructive 등)를 렌더타임에 생성. `missing_verification` raw-재파생(가치가 필요해질 때).

## 3. 제거 범위 — 백엔드

| 대상 | 조치 |
|---|---|
| `src/insight/episode/` (classifier.rs, types.rs, rules.rs, mod.rs) | **모듈 디렉터리 삭제** |
| `src/insight/mod.rs` | `pub mod episode;` 제거 |
| `src/db/repo_episode.rs` | **파일 삭제** |
| `src/db/mod.rs` | `repo_episode` `pub mod` 선언 제거 |
| `src/graph/build.rs::rebuild_session` | episode 분류 블록(`classify_session` 호출 + `catch_unwind` + `delete_session`/insert 루프, 약 50~117행) 제거. `compute()` 그래프 빌드/커밋은 유지. 반환 시그니처에서 episode 카운트 제거(현재 `(usize,usize,usize)`는 node/edge/finding이므로 영향 없음 — 확인). |
| `src/insight/view.rs` | `SessionInsightView.episodes` 필드 + `repo_episode::list_session` 호출(42행) 제거 |
| `src/insight/extractors/missing_verification.rs` | **파일 삭제** |
| `src/insight/registry.rs` (11행), `src/insight/pipeline.rs` (304행) | `missing_verification::MissingVerification` 등록 제거 |
| `src/insight/extractors/risky_action.rs` (108·162행) | evidence projection의 `"episode_phase": "action"` 라벨 필드 제거(실제 episode 의존 아님, 잔재 라벨) |
| `src/api/routes.rs` | `/episodes` 목록(667행 `episodes_list`)·`/episodes/{id}` 단건(686행 `episode_detail`) 핸들러 제거 |
| `src/api/mod.rs` | 위 두 라우트 등록 제거 |
| `src/api/dto.rs` | episode DTO(189행 부근 `episode_id` 등) 제거 |
| 마이그레이션 | **신규 forward 마이그레이션** `20260531xxxxxx_0017_drop_episode.sql`에 `DROP TABLE IF EXISTS episode;`. 기존 `0006_episode.sql`은 히스토리에 보존(forward-only, sqlx 규약). |

## 4. 제거 범위 — 프론트엔드

| 대상 | 조치 |
|---|---|
| `webui/src/routes/episodePhase.ts` (+ `__tests__/episodePhase.test.ts`) | **삭제** (phaseAt) |
| `webui/src/components/replay/EpisodeStrip.tsx` (+ test) | **삭제** (phase 막대) |
| `webui/src/routes/SessionDetailPage.tsx` | `phaseByEventId`/`phaseAt` 계산 및 stream·NodeDetail에 넘기던 phase 배지 prop 제거. **activity-run 렌더는 유지(배지 없이).** EpisodeStrip 마운트 제거. |
| `webui/src/components/replay/detail/NodeDetail.tsx` | `episodePhase` prop(10·168행) + "episode" 행 렌더(176·194행) 제거 |
| `webui/src/components/replay/timeline/Timeline.tsx` | `EpisodeDto` import(8행), `episodes` prop(22행), `EPISODE_HEIGHT`(33행), `PHASE_COLORS`(51~), episode band 렌더 제거 |
| `webui/src/components/replay/insight-strip/InsightStrip.tsx` | EpisodeStrip 언급 주석(6·9행) 갱신(기능 변경 없음) |
| `webui/src/api/types.ts` | `EpisodeDto`(126~) 제거 |
| `webui/src/api/client.ts` | `getEpisodes`(77~78), `EpisodeDto` import(8), 관련 주석(67) 제거 |
| `webui/src/lib/queries.ts` | `useEpisodesQuery`(79~81), `sessionKeys.episodes`(45), `getEpisodes`/`EpisodeDto` import 제거 |
| 관련 테스트 | `api_episodes` 계약/엔드포인트 테스트, `SessionDetailPage.test`·`Timeline.test`·`NodeDetail.test`·`client.endpoints`·`types.contract`·`queries`·`sse` 중 episode 단언 제거. `buildStreamModel.test.ts:144`는 한국어 텍스트의 우연 매치 — **변경 불필요**. |

## 5. 제거 범위 — 테스트 (백엔드)

삭제 또는 갱신:
- 삭제: `tests/episode_classifier_basic.rs`, `episode_determinism.rs`, `episode_drift_no_overlap.rs`, `episode_gold.rs`, `episode_gold_count_invariant.rs`, `episode_no_overlap_real.rs`, `episode_rebuild_no_accumulation.rs`, `episode_rebuild_writes_rows.rs`, `episode_rule_registry.rs`, `api_episodes.rs`, `extractor_missing_verification.rs`, `migration_episode_schema.rs`.
- 갱신: `insight_pipeline.rs`·`insight_registry.rs`(missing_verification 등록 기대 제거), `graph_*`(episode 미생성 — rebuild가 episode 행을 더는 안 쓴다는 invariant로 전환 가능).

신규(제거를 잠그는 회귀 테스트, TDD red 우선):
- `GET /v1/sessions/{id}/episodes` 및 `/episodes/{id}` → **404/라우트 부재**.
- `rebuild_session` 후 (테이블 부재이므로) episode 관련 쓰기 없음 — 그래프·finding 파이프라인은 정상 동작.
- 마이그레이션 적용 후 `episode` 테이블이 스키마에 **없음**.
- finding 파이프라인이 `missing_verification` 없이 정상(다른 4개 추출기 green).

## 6. 제거 범위 — 문서

- `docs/03_data_model_spec.html §6` — Episode + 7-phase 정의 제거 (**source of truth, 이번에 반드시 정합**).
- `docs/04_api_mcp_spec.html` — `/episodes`* 엔드포인트 제거.
- `docs/00_prd_revised.html`, `02_technical_architecture_spec.html`, `06_mvp_execution_plan.html`, `index.html` — episode/phase 언급 정리.
- `docs/implementation-notes.html` — 제거 결정·근거·마이그레이션 노트 추가.
- `CLAUDE.md` — episode 상태 노트(3건) 제거 + "episode/phase 제거됨, migration 0017, init-db 필요" 운영 주의 추가.

## 7. 제거 후 결과 상태

- **메시지 뷰**: 메시지 + 접힌 activity-run(**phase 배지 없음**) + thinking 마커. 가시성용 접기는 그대로.
- **그래프/타임라인/detail**: episode band만 사라짐. 노드 선택 시 facet 지표 표시는 유지.
- **Findings**: tool_failure·context_bloat·final_state_mismatch·risky_action 유지. missing_verification 사라짐.
- episode 테이블·phase·분류기 전부 없음.

## 8. 리스크 / 결정

- **finding 수 변화**: missing_verification 32건 소멸(전부). 다른 finding 불변. (옵션 B 합의됨.)
- **DB 마이그레이션**: episode 테이블 drop. 기존 dev DB는 `witmcc init-db`(또는 신규 마이그레이션) 적용 필요. CLAUDE.md 운영 주의에 명시.
- **배지 없는 activity-run**: run이 라벨 없이도 깔끔히 렌더되는지 브라우저 smoke로 확인.
- **문서 정합을 이번에 끝낸다**: 03/04는 source-of-truth라 deferral 금지(과거 Phase 4 deferral 반복 안 함).
- **결정: forward 마이그레이션으로 drop** (0006 편집 아님) — sqlx forward-only 규약.
- **브랜치/통합**: 이 제거는 fold(Slice 1/2)·#27 keeper 위에 올려야 하는지, main 기준인지 = 구조 결정. **임의로 정하지 않고 사용자 지시를 받는다.**

## 9. Non-goals (이번 스펙)

- per-event 태그 분류기 구현 (Read→code/docs, Bash→category 등) — 별도 후속 스펙.
- `missing_verification` raw-재파생 — 가치가 필요해질 때 별도.
- `verification_run`·`diff_hunk`·fold/facet 변경 — 유지, 손대지 않음.
