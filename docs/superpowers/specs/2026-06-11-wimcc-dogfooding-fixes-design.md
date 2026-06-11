# 설계 — OTel 세션 분석 기반 개밥먹기 수정

- **날짜**: 2026-06-11
- **출처**: 두 OTel 세션(`e8d51785-d200-4449-8716-1d47408cb0b4`, `6a254a2a-bd02-4b85-a0c1-bd179c72b808`)을
  `/v1/sessions/*/metrics·signals·verification-runs·usage`와 브라우저 뷰(insight strip + 분석 패널)에서
  교차 분석한 개밥먹기 결과.
- **승인**: 사용자 (piped→estimated 승격 + §6.2 갱신 / re_read 안정키+재조정 선택 후 "진행").

## 배경 — 분석에서 확정된 사실 (evidence-linked)

| 발견 | 근거 |
|------|------|
| re_read signal 154행이 distinct 53파일로 붕괴 (2.9배 부풀림). `verification_run.rs` 한 파일에 19행. | `/v1/sessions/6a254a2a/signals`. signal들의 `created_at`이 00:24:44~00:25:05 21초 윈도우 + 03:20 재ingest에 몰림(세션 실제 활동 시간 아님). read_count 7→12→…→54 단조 증가. |
| read_count **값**은 실제 — count=54 signal의 `evidence_refs`에 distinct event 54개. 행 수만 오염. | `/v1/signals/{id}` evidence_refs 길이 == read_count, 같은 파일 signal들의 evidence는 포함관계(51개 ⊂ 54개). |
| verification exit_code 97/97 전부 null. piped 50건 전부 unknown. | `/v1/sessions/6a254a2a/verification-runs`. status_basis: piped 50(전부 unknown)·exit 43(17 pass+14 fail+12 unknown)·policy_denied 4. |
| Bash에 연결된 hook_event 0개(전부 transcript 유래 `hook_success`/Stop류). hook collector(`/hooks/v1/events`)는 0행. | events 스캔 + `/v1/health/sources` hook row_count=0. |
| trace_id는 otel_span·log_record(33%)에만, transcript 유래 이벤트(tool_call/result/thinking/hook 등) 100% 공란. | events 스캔. |
| 비용 미가격 — 6a254a2a 지배 모델 `claude-fable-5` `priced:false`($0) → 헤드라인 $27.70 과소추정. | `/v1/sessions/6a254a2a/usage` by_model. |

## 원인 분석 (코드 위치 확정)

### re_read 부풀림
`src/insight/pipeline.rs::build_signal_row`:
```
signal_id: derive_signal_id(c.detector, session_id, &c.evidence_refs)
```
`signal_id`가 **evidence_refs(읽기 이벤트 ID 목록)** 로 파생된다. re_read처럼 evidence가 누적되는
detector는 읽기가 늘 때마다 evidence set이 바뀌어 새 `signal_id`가 생성 → `INSERT OR REPLACE`
(`src/db/repo_signal.rs::insert`)가 옛 행을 덮지 못하고 누적된다. 파이프라인 주석의 "idempotent"는
입력 이벤트가 동일할 때만 참 — 증분/재ingest로 evidence가 늘면 깨진다.

### verification piped → unknown
`src/ingest/verification_run.rs`:
- 라인 140–163: unknown인 verification kind에 대해 tier-4 `looks_like_success`/`looks_like_failure`
  (라인 558–587)가 `test result: ok` / `Test Files … passed` / pytest / jest 요약을 인식해
  Passed/Failed(Estimated)를 **이미 계산**한다.
- 라인 204–209: `if m.status_basis == "piped" || result_disposition.is_some() { ("unknown", "unknown") }`
  — piped면 위 estimated 결과를 **폐기**하고 무조건 unknown으로 덮는다(문서화된 design §6.2).

즉 파서가 못 읽는 게 아니라 piped 분기가 estimated 판정을 버린다. tier-4 휴리스틱은 이미
vitest·cargo·pytest·jest를 커버한다.

## Track 1 — re_read signal 멱등화 (backend, Rust)

**수정 (안정키 + 재조정):**
1. `SignalCandidate`(`src/insight/types.rs`)에 `dedup_key: Option<String>` 추가.
   - re_read 추출기(`src/insight/extractors/re_read.rs`)는 `dedup_key = Some(file_path)`.
   - tool_failure·risky_action·context_bloat은 `None`(evidence가 안정적이라 현행 id 유지).
2. `build_signal_row`: `derive_signal_id(detector, session, dedup_key.as_deref().unwrap_or 기존 evidence 경로)`.
   읽기가 늘어도 file_path가 같으면 같은 id → REPLACE로 read_count만 갱신.
3. `run_detectors`: detector별 현재 패스의 `signal_id` 집합을 모아
   `repo_signal::reconcile(pool, session, detector, keep_ids)`로 현재 패스에 없는 그 detector의
   signal을 삭제. 기존 누적 중복은 다음 ingest에서 self-heal, 더는 안 잡히는 신호도 정리.

**불변 유지**: read_count 값(evidence-backed)·detector facts·"severity/confidence 없음" 원칙.

**테스트 (TDD red 먼저):**
- 같은 file_path를 N회 읽은 뒤 N+3회로 재ingest → 해당 파일 re_read signal **정확히 1개**,
  read_count=N+3.
- 사전 주입한 stale signal(현재 패스에 없는 id)이 `run_detectors` 후 삭제됨(reconcile).
- `derive_signal_id`가 re_read에서 evidence 증가에도 안정적(dedup_key 경로).
- 기존 tool_failure/risky_action/context_bloat id 불변(회귀 방지).

**재ingest**: 스키마 변경 없음(detector 로직만). reconcile가 자가치유하므로 init-db 불필요 —
두 세션 재ingest로 검증.

## Track 2 — verification piped → estimated (backend) + 사양 §6.2 갱신

**수정** (`src/ingest/verification_run.rs` 라인 204–209):
- disposition(user_rejected/policy_denied/cancelled/background)이 있으면 → unknown (불변).
- piped이고 disposition 없음:
  - tier-4가 Passed/Failed(Estimated)를 냈으면 그대로 유지, `status_basis="piped"` 보존.
  - tier-4도 결정 못 함(요약줄 부재 — 예: `tail`로 잘림) → unknown 유지.
- exit-code 경로는 계속 measured (불변).

`looks_like_success`/`looks_like_failure`는 현재 커버 범위 유지; Track 3 루프가 찾아낸 미인식
요약 패턴을 이후 라운드에서 확장(잠그는 테스트 동반).

**사양 갱신**: design §6.2를 다음으로 정정 —
> piped는 exit code를 가려 measured 판정이 불가하지만, piped content 내 도구의 결정론적 요약줄
> (`test result: ok`, `Test Files … passed` 등)이 있으면 estimated pass/fail로 해소한다.
> `status_basis="piped"`로 측정 불가 사유를 투명하게 보존한다.

`docs/`의 §6.2 해당 절(파악 후 정정) + `docs/implementation-notes.html`에 편차·근거 기록.

**테스트 (TDD red 먼저):**
- piped vitest 통과 content → status=passed, provenance=estimated, status_basis=piped.
- piped cargo 실패 content → failed/estimated/piped.
- piped인데 요약줄 없는 content(tail로 잘린 모사) → unknown.
- disposition(policy_denied 등) content → unknown 유지.
- 가능한 한 `tests/fixtures/transcripts/real/`의 실 payload로 잠금(없으면 인라인 + 근거 주석).

**재ingest**: 스키마 변경 없음. 두 세션 재ingest로 unknown 66→감소 실측 기록.

## Track 3 — unknown-verification 수집 루프 (webui/scripts, untagged-bash 미러)

**새 스크립트** `webui/scripts/unknown-verification.ts` (read-only, Node 22+ 직접 실행, 클린 JSON):
- Pull API: `/v1/sessions` → 각 세션 `/v1/sessions/:id/verification-runs` → `status=unknown`만 필터.
  trigger tool_result content는 `/v1/events`(또는 `/raw`)에서 가져와 tail 발췌.
- 출력: `[{command, command_kind, status_basis, count, sample_content_tail, hint}]` count 내림차순.
- **hint 로직**:
  - `status_basis=="piped"` & content tail에 미인식 요약 패턴 있음 → "요약 존재, `looks_like_success/failure` 확장 후보".
  - content 비어있음/요약 없음 → "piped 출력에 요약 없음(하위 필터가 잘라냄) — 복구 불가".
  - `status_basis` ∈ {policy_denied, user_rejected, cancelled, background} → "disposition, 정상 unknown".
- **권위 파싱은 Rust에 유지** — 스크립트는 후보 surfacing만(중복 로직 없음, untagged-bash와 동일 철학).
  스크립트의 hint는 휴리스틱이며 권위 판정이 아님을 문서에 명시.

**루프**: 스크립트 실행 → 미인식 요약 확인 → Rust 휴리스틱 확장(잠그는 테스트) → 재ingest → 재실행 →
unknown 감소. CLAUDE.md에 "verification unknown 루프" 절 + `implementation-notes.html`에 설계 근거 기록
(untagged-bash 루프와 병치).

## Track 4 — UI: 분석 패널 drill-down + 검증 헤드라인 정직화 (webui, React)

**4a drill-down** (`webui/src/components/replay/analysis/AnalysisPanel.tsx`):
- DETECTOR 신호 분포 행을 펼치면 해당 detector의 signal 목록 표시:
  - re_read: `file_path` + `read_count` (read_count 내림차순).
  - tool_failure: tool_name + error_excerpt.
  - 기타: summary.
- 각 항목에서 evidence 이벤트로 딥링크(기존 "deeplink around" 재사용 — replay stream의 해당 이벤트 주변으로 이동).
- 데이터원: `/v1/sessions/:id/signals` (이미 `facts`·`evidence_refs` 보유).

**4b 헤드라인 정직화** (`webui/src/components/replay/insight-strip/`):
- 검증 카드 "가드 97 · 통과 17"에 미측정 비율을 함께 노출 — 분석 패널의
  "측정분 17/31 · 미측정 66 · 55%"와 일치하는 표기. Track 2로 unknown이 줄면 자동 반영.

**테스트**: vitest(red 먼저) — drill-down 펼침/목록/딥링크, 헤드라인 미측정 표기.
**브라우저 smoke 필수**(CLAUDE.md): `wimcc serve` + claude-in-chrome navigation + 시각 검증 후 commit.

## 순서 · 의존성

1. **T1 (re_read 멱등)** — 독립 backend.
2. **T2 (piped→estimated + §6.2)** — 독립 backend. verification 카운트가 바뀜.
3. **T4 (UI)** — T1/T2 후(고친 데이터를 표시). 4a는 signals 독립, 4b는 T2 수치 반영.
4. **T3 (unknown 루프)** — 마지막. 개선된 파서 위에서 잔여 unknown을 surfacing → 휴리스틱 추가 라운드.

각 backend 트랙 후 두 세션 **재ingest**(스키마 변경 없음). UI 트랙은 브라우저 smoke.

## 명시적 범위 외 (이번 미선택 — deferred)

- **비용 미가격**: `claude-fable-5`/`<synthetic>` `priced:false` → 6a254a2a 비용 과소추정. `models_without_pricing`
  플래그는 있으나 헤드라인 숫자가 오해 유발. (`src/insight/pricing.rs`)
- **trace_id 상관 공백**: transcript 유래 이벤트가 trace_id 공란이라 OTel span과 trace_id 조인 불가
  (OTel-first 원칙과 부분 충돌).
- **turn_duration 결손**: 6a254a2a turn_duration_count 27 < user_turns 38 (11턴 누락).

분석에서 발견했으나 이번 4트랙에 미포함. 후속 작업 후보로 기록만.
