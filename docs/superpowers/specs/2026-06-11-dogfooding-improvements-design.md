# 개밥먹기 개선점 일괄 구현 — 설계

2026-06-11 두 OTel 세션(e8d51785, 6a254a2a) 분석에서 도출된 개선점을 일괄 구현한다.
이미 main 분기로 커밋된 ②③④①(아래 "완료") 위에 나머지를 쌓아 한 PR로 묶는다.

## 이미 완료 (브랜치 dogfooding-improvements에 포함된 기존 5커밋)

- ② `3ddc934` verification clippy "Finished" lint 성공 인식 (command_kind 한정)
- ④ `061ebe3` untagged collect Read/Write 확장자 분류 (full-path BASH 오분류 제거)
- ③ `322e633` re_read를 (scope=main|sidechain, 범위) 단위로 (페이지네이션·subagent 오판 제거)
- ① `eef5d0e` fable-5 가격 추가·Opus 3배 과대 정정·가격표 버전을 갱신날짜로
- `fdef3dc` implementation-notes

## 이번에 구현 (순서대로)

### 1. ingest --all 세션별 1회 재계산 최적화 (성능, 결과 불변)

문제: `store.rs`가 파일마다 그 파일이 속한 세션 전체를 재계산해(`sessions_touched` →
list_session + verification/usage/signal 재derive), 같은 세션의 subagent 파일 N개면
그 세션을 N번 중복 재계산. 733파일 + DB 1GB에서 ~37분.

해결: raw 삽입은 파일별로 두되, insight 재계산(verification_run·usage_facet·run_detectors)을
모든 파일 처리 후 **touched 세션 합집합에 대해 1회**만 수행. 세 재계산 모두 list_session(전체
events) 기반 + 멱등(insert_or_replace / raw_event_id dedupe / dedup_key+reconcile)이므로
"1회 = 현재의 마지막 회차"와 동일 산출 → 결과 불변, 중복 N−1회 제거.

테스트: 동일 입력에 대해 (a) 파일별 재계산과 (b) 세션 합집합 1회 재계산의 verification_run·
signal 산출이 동일함을 잠근다. + 같은 세션 다중 파일 ingest 시 재계산이 세션당 1회임.

### 2. ④ 잔여 Bash 토크나이저 노이즈

`eventTags.ts` `collectUntagged`/세그먼터에서 명령이 아닌 토큰을 걸러낸다(corpus 잔여):
- 서브셸 `(cd …`·`(npm …` 선두 `(` strip
- 루프 제어 `break`/`continue`를 CONTROL_TOKENS에 추가
- 빈 토큰('')과 줄바꿈으로 분리된 순수 플래그줄(`-u`/`-s`/`-c` 등 첫 토큰이 `-`로 시작)을 제외

`eventTags.test.ts`로 각 케이스 잠금. UI 영향 → 브라우저 smoke.

### 3. turn_duration 완전성

조사 결과 "결손"이 아니라 분류였다: system subtype turn_duration 27 + away_summary 10 +
compact_boundary 1 = 38 = user_turns. away/compact turn은 turn_duration 대신 별도 레코드를 쓴다.

해결: `metrics.rs`에서 `away_summary`/`compact_boundary` 카운트를 SessionMetrics에 노출
(turn_duration_count가 활성 turn만 셈을 정직화). 합 = user_turns 검증 테스트.

### 4. agent_id 세분 (가장 무거움) — backfill 방식 (init-db 회피)

- migration 0023: `observed_event`에 `agent_id TEXT` 컬럼 추가.
- `mapping.rs`: transcript record의 `agentId`를 `ObservedEvent.agent_id`로 파싱(sidechain 라인).
- `re_read` scope를 `main`/`sidechain` → `main`/`agent:<id>`로 세분(agent_id 있으면 그것, 없으면 sidechain).
- **재ingest = backfill**: raw_event.payload(원본 jsonl 라인)에서 agentId를 읽어 기존
  observed_event 행의 agent_id를 UPDATE하는 backfill 경로. init-db(1GB 재생성)를 회피.
  backfill 후 영향 세션 insight 재계산(1번 최적화된 경로로).
- 테스트: mapping이 agentId를 채움, re_read가 서로 다른 agent를 분리 발화, backfill이 기존 행 갱신.

### 5. trace_id 상관 — won't-fix (소스 한계)

transcript 이벤트(tool_call/result/assistant 등)에는 OTel correlation용 trace_id가 없다
(파싱상 8건은 본문에 우연히 든 문자열). OTel span/log만 trace_id 보유. timestamp 근사 조인은
부정확해 비채택. → 구현하지 않고 implementation-notes에 "소스 한계, won't-fix"로 확정 기록.

## git / PR

- 브랜치 `dogfooding-improvements`(기존 5커밋 포함), main은 origin/main(2c70c9f)으로 되돌림.
- 위 1~5를 항목별 커밋으로 쌓고 push → PR(base main). 기존 5 + 신규 커밋 전부 PR에 포함.
- 4번 backfill 검증 + 프로덕션 반영 후 PR.

## 비-목표

- init-db 전체 재생성(4번은 backfill로 회피)
- trace_id 구현(5번 won't-fix)
- ingest --all 최적화에서 동작 변경(1번은 성능만, 결과 불변)
