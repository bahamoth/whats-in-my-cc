# 개밥먹기 회고 — 2026-07-06 (unknown verification 집중)

> **대상 세션**: `31f1fb03-b98c-4f6b-8dba-188826f5ead0` (2026-07-04 13:01 ~
> 2026-07-06 10:33, 52,858 이벤트, 111턴, cozy-yawning-pebble). **표본 1 세션** —
> 일반화 주장 없음. 분석 도구: Pull API(read-only) + `unknown-verification.ts` +
> sqlite read-only 조회. 회고 실행 세션: `98cca611`.

## 0. 이전 예측 검증 (2026-06-12 원장)

이전 원장은 R-ID·예측 형식 도입 전이라 **전후 지표 비교는 판정불가**. 채택
여부만 판정:

| 이전 제안 | 판정 | 근거 |
|-----------|------|------|
| §3-3 프로젝트→세션 매핑 | 채택 ✓ | `GET /v1/sessions?project=` 이번 회고 Step 1에서 실사용 |
| §3-2 턴 집계 endpoint | 채택 ✓ | `GET /v1/sessions/:id/turns` 실사용 (111턴 + file_churn) |
| §3-1 kind 필터 | 채택 ✓ | `events?kind=user_message` 호출 → user_message만 반환 확인 |
| §5 session-retrospect skill | 채택 ✓ | 이 회고가 그 스킬로 실행됨 |
| §2 re_edit_churn류 detector | 부분 채택 | `re_read` detector 존재·발화 10건 (commit `1c22ede`) — duplicate_edit_stream은 미구현 |
| §4 Signal 품질 3건 | 미판정 | 이번 회고 범위 밖 (다음 회고에서 확인) |

이번 원장부터 각 제안에 **반증 가능한 예측**을 붙인다.

## 1. 사용자 질문 — "unknown verification 비중이 왜 높은가"

### 판정: 두 겹의 원인. 지배 원인은 **측정 아티팩트**, 잔여는 파서 사각지대.

**겹 1 — 사용자가 본 38%는 낡은 수치다 (측정 아티팩트, 라이브 확증).**

| 소스 | total | passed | failed | unknown | unknown 비율 |
|------|-------|--------|--------|---------|-------------|
| `GET /v1/sessions/:id/metrics` (20:07 KST 재호출도 동일) | 541 | 277 | 24 | **207** | **38.3%** |
| `GET /v1/sessions/:id/verification-runs` = DB 테이블 실측 | 362 | 243 | 43 | **43** | **11.9%** |

- 세션 기간(7/4~7/6)에 verification_run rows를 쓴 것은 **당시 떠 있던 구세대
  serve 파서**(541 runs / 207 unknown). 7/4~7/5 PR들에서 looks_like_* 패턴이
  대거 추가됐지만 운영 serve는 재시작 금지 원칙으로 구버전인 채 ingest를 계속했다.
- 오늘 19:53 serve 재기동(backfill 재ingest)이 **현행 파서로 rows를 재계산** —
  테이블 실측 43/362 (rows `created_at` = 2026-07-06 11:08Z, 오늘 재작성 확인).
- 그런데 `/metrics`는 **인메모리 캐시**(`src/insight/metrics.rs` B-8,
  키 = `(event_count, last_observed_at)`)가 구값을 반환. 닫힌 세션은 이벤트가
  늘지 않으므로 **backfill이 verification_run·signal 테이블을 다시 써도 캐시가
  영원히 무효화되지 않는다.** 캐시 주석의 가정("detector 재구성은 새 이벤트
  flush 또는 프로세스 재시작을 동반")이 **재기동-후-backfill 경로에서 깨진다** —
  캐시 엔트리는 backfill이 이 세션 rows를 다시 쓰기 *전에* 생성될 수 있고,
  이후 rows가 바뀌어도 키는 그대로다. 20:07 재호출로 라이브 확증.

**겹 2 — 현행 파서 기준 잔여 unknown 43건(11.9%)의 구조** (unknown-verification.ts,
세션 단위):

| 버킷 | 건수 | 원인 | 복구 가능성 |
|------|------|------|------------|
| 에이전트 자작 exit 마커 (`fmt applied exit=0`, `EXIT: 0`, `FMT_EXIT=0`) | 19 (fmt 10 + tsc 8 + fmt-check 1) | quiet-success 도구 + CC가 성공 시 exit code를 transcript에 안 남김 → 에이전트가 임의 형식 마커를 echo했으나 파서 미인식 | 파서 확장 또는 마커 표준화 |
| 요약이 출력에 있는데 파서가 놓침 | 12 (build 6 `Finished …target(s)` + vitest 3 `Test Files 1 failed (1)` + cargo test 3 `failures:` 섹션) | looks_like_success/failure 패턴 부재 | 파서 확장 (전형적 unknown-루프 대상) |
| 출력 소실 (`(Bash completed with no output)`, 무관 echo) | 11 | 파이프 하위 필터가 요약 제거 / vitest `-t` 불일치 무출력 | **파서로 복구 불가** — 행동 개선 대상 |
| 하네스 타임아웃 (`Command timed out after 5m 0s`, clippy) | 1 | 결정론 마커인데 미분류 | 파서 확장 (failed 또는 별도 disposition) |

파서 확장(아래 R-2)으로 43 → **23건(6.4%) 실측** (스크래치 재ingest 3회 측정:
① 패턴팩 후 19 → ② "exit code: N" 형태 추가 후 17 → ③ **파이프-echo 신뢰
게이트** 추가 후 23). ③에서 6건이 unknown으로 *되돌아온* 것은 개선이다 —
`도구 | tail; echo "exit: $?"` 형태의 `$?`는 tail의 exit라 도구 성패를 반영하지
않는데(real fixture `verification_tsc_v01.jsonl` 178fae97이 잠금) ①②가 이를
가짜 passed/failed로 승격시키고 있었다. 예측치(≤15)는 반증 — 두 이유:
"EXIT: 0"류 그룹 다수가 실제로는 무출력이었고(그룹 count는 대표 샘플 1건의
tail만 봄 — 과대추정 함정), 파이프-echo 6건은 애초에 복구 대상이 아니었다.
잔여 23 전수: 무출력 13 · 파이프-echo 신뢰 불가 6 · 요약 잘림(ANSI 덤프 등) 3 ·
임의 마커("fmt clean") 1 — 파서로 복구 불가, R-3 행동 지침의 대상.

## 2. 세션 행태 관측 (Step 3 판별)

- **4-PR 분할 사고 수습이 세션 후반을 지배**: 11:57 사용자 교정("여러 PR을
  능동적 검증 없이 & 통합 없이 머지했다", turn `b7c6b231`) → 재검증 턴
  `4ef10c79`(164 tools/19 edits)·`6a04683a`(278 tools/15 edits) + 리뷰
  서브에이전트 세션 3개. 교훈은 이미 CLAUDE.md(never-split-integration-line,
  2026-07-05)에 반영됨 — 재제안 불필요, 채택 확인만.
- **장시간 검증 대기 독촉 4턴**: `1cae333a`(01:08)·`ac4fbff2`(01:29)
  ·`25ec7db4`(05:14)·`2e28b171`(06:16) — "cargo test 완료 대기가 오래 걸리는
  것 같은데"/"대기가 길다"/"너무 오래 기다린다". clippy 5분 타임아웃 1건과 같은
  계보(무거운 검증의 포그라운드 대기). user_interruption_count=5.
- file_churn 상위 `progress.md` 50턴/56편집은 SDD 워크플로우의 의도된 원장,
  i18n `en.ts`/`ko.ts` 락스텝(11/10턴)은 이중 카탈로그 설계상 정상 — 문제 아님.
- tool_failure 79/4853 (1.6%), re_read 10건 — 특이 없음.

## 3. 제안 원장

| ID | 제안 | 상태 |
|----|------|------|
| R-20260706-1 | **metrics 캐시 무효화 수정** (`src/insight/metrics.rs`): 캐시는 이벤트 스캔 파생값만 담당, 사이드테이블(signal·verification_run·usage) 파생 필드는 히트 시에도 `apply_side_table_metrics`로 매번 재계산. TDD: `tests/metrics_compute.rs::side_table_rebuild_reflected_despite_metrics_cache`(위반을 빨강으로 재현 후 구현). 키 확장안(MAX(created_at))은 초 단위 해상도라 같은 초 내 재기록을 못 잡아 기각. | 반영(이 PR) |
| R-20260706-2 | **파서 패턴 확장** (`src/ingest/verification_run.rs`, TDD·실픽스처): ① looks_like_failure += vitest 전실패형 `" failed ("`, cargo test `"\nfailures:"` 섹션 헤더, 하네스 `"Command timed out after"`(→failed est.) ② `Finished …target(s)` 승격 게이트를 lint→lint+build로 확장(test_suite_*는 계속 제외 — caveat 3 정정) ③ 에이전트 echo 마커 `echoed_exit_status`(`EXIT[:=]N`·`_EXIT=N`, 마지막 5행 한정; 0→passed est., 비0→failed est.). | 반영(이 PR) |
| R-20260706-3 | **CLAUDE.md — 검증 출력 위생**: 요약이 살아남게 실행(grep/head/tail 필터 금지), quiet-success 도구는 `echo EXIT=$?` 한 형식만. | 반영(이 PR) |
| R-20260706-4 | **CLAUDE.md — 무거운 검증은 백그라운드**: 1분+ 검증은 run_in_background + 완료 통지 회수. | 반영(이 PR) |
| R-20260706-5 | **파서 세대 가시화**: PARSER_VERSION을 `verification_run@v1.1`로 bump + "패턴팩 변경 시 minor bump" 규약을 상수 주석에 명문화. | 반영(이 PR) |

### 예측 (다음 회고가 검증할 것)

- **R-1** 개선: 닫힌 세션 `/metrics` vs `/verification-runs` 불일치 0건(불변식
  테스트 green 유지). 악화 가능: `cache_hits()` 적중률 하락 → 대시보드 series
  응답 시간 상승(B-8 도입 사유였던 1.2s/18세션으로 회귀 여부 측정).
- **R-2** 개선: 이 세션 재ingest 시 unknown 43→≤15, unknown 비율 11.9%→≤4%.
  **[반영 당일 실측: 43→23(6.4%) — 건수 예측(≤15)은 반증(무출력 과소평가 +
  파이프-echo 6건은 복구가 아니라 오승격이었음), 방향은 검증. §1 겹 2 참조.]** 악화 가능: passed(estimated) false-positive —
  `Finished` 게이트가 test류로 새면 실패한 테스트가 passed로 둔갑(실픽스처
  테스트로 잠금, caveat 3 참조).
- **R-3** 개선: 다음 유사 규모 세션에서 unknown(piped·no-output) 버킷 감소
  (이번 11건 대비). 악화 가능: tool_result_truncated_count·컨텍스트 사용 증가.
- **R-4** 개선: "대기 독촉"류 사용자 개입 턴 감소(이번 4턴 대비),
  하네스 타임아웃 unknown 소멸. 악화 가능: tool_backgrounded 증가 자체는
  중립이나, 백그라운드 결과 미회수로 verification not_executed(background)가
  늘면 지표 게임 — outcome 지표(passed/failed 합) 동반 확인 필요.
- **R-5** 개선: 다음 회고에서 "어느 파서 세대가 쓴 rows인가"를 쿼리 1번으로
  판별 가능. 악화 가능: 없음(라벨 문자열 변경뿐).

## 4. wimcc 피드백 (분석 마찰)

- `/metrics`와 `/verification-runs`의 불일치(§1 겹 1)는 이 회고 스킬의
  Step 2 데이터 신뢰를 직접 훼손한다 — 회고가 metrics만 읽었으면 38%를
  실질로 오판했을 것. R-1이 곧 피드백.
- `unknown-verification.ts --all`의 세션 합산(510건)과 세션 단위 실행(43건)은
  파서 세대가 섞인 DB에서 크게 다른 그림을 준다 — backfill 완주 후 재실행이
  안전(이번엔 세션 단위 수치로 판별).
