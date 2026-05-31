# Episode Redesign — Slice 3 Design (autonomous session)

- 날짜: 2026-05-31
- 브랜치: `episode-redesign-slice3-bugfixes` (Slice 2 `telemetry-fold-slice2-groupbc` 위 스택)
- 상위 설계: `docs/superpowers/specs/2026-05-31-telemetry-facet-fold-and-episode-redesign-design.md` (Slice 3 = episode 재설계)
- 발단 issue: `docs/superpowers/issues/2026-05-31-episode-classification-issues.md`

> **⚠️ 무인(원격) 세션에서 작성·구현됨.** 사용자가 외출 중 검증 불가 상태에서 진행. 아래 **§B(자율 구현)** 는 사용자가 실제 관측·지적한 버그를 직접 고치는 저위험 surgical 수정만 포함한다. **§C(검증 대기)** 는 의미론을 바꾸는 깊은 재설계로, **사용자 승인 전 구현하지 않는다** — spec으로만 남긴다. 모든 자율 결정은 **§D**에 플래그.

## A. 배경 — 사용자가 관측한 문제

1. **grep이 `action`으로 오태깅** (`?selected=nd_0d70b110…`): 노드는 `Bash: grep -n ...`. classifier가 `Bash ∈ MUTATION_TOOLS` → Action으로만 판정, read-only `Bash: grep`을 구분 못 함.
2. **stale 겹침 배지**: 누적된 넓은 episode가 좁은 정확 episode를 덮어 잘못된 phase 배지.
3. (상위 issue) phantom intake, 누적 폭증(한 시작 이벤트 124 episode), drift/exploration 휴리스틱의 약함.

## B. 자율 구현 범위 — surgical 버그수정 3건 (저위험, 관측된 문제 직접 해결)

현 7-phase 모델을 유지한 채, 관측된 버그만 고친다. **어떤 미래 disposition에서도 옳은** 수정들이다.

### B1. Bash 의도 판정 — read-only Bash는 Action이 아니다
- `src/insight/episode/classifier.rs`: `tool_name == "Bash"` 인 tool_call에 대해, `payload.input.command`의 **첫 명령어가 read-only allowlist**에 속하고 mutating 연산자(`>`,`>>`,`rm`,`mv`,`mkdir`,`git commit/push` 등)가 없으면 → **read-only 취급**(현 모델의 Exploration/Diagnosis 경로), 아니면 기존대로 Action.
- read-only allowlist(보수적, §D2에서 플래그): `grep rg egrep fgrep ls cat find head tail wc which pwd echo env file stat du df tree sort uniq` + git read-only 서브커맨드(`status log diff show branch blame`). VerificationRun 처리(Bash-on-verify-allowlist→Verification)는 그대로.
- 보수적 기본값: 모호하면 **Action 유지**(변경 가능성 있는 명령을 read-only로 잘못 낮추지 않음).

### B2. Episode 누적 제거 — rebuild 시 세션 episode delete-후-insert
- `src/graph/build.rs::rebuild_session`: graph는 `delete_session_in_tx` 후 insert하나 episode는 delete 없이 `INSERT OR REPLACE`만 → live rebuild마다 stale episode 누적. 수정: episode insert **전에** 세션 episode 삭제(`repo_episode::delete_session`). graph delete와 같은 트랜잭션에 합치는 것이 이상적이나, 현재 episode insert는 pool 기반(tx 밖) — 최소 변경으로 insert 직전 세션 delete 추가.

### B3. 프론트 겹침 해소 — 가장 좁은/늦게 시작한 episode 선택
- `webui/src/routes/SessionDetailPage.tsx` `phaseByEventId`: `eps.find(첫 매칭)` → 겹치면 가장 일찍 시작한(넓은 stale) 것이 이김. 수정: 시각 `t`를 덮는 모든 episode 중 **가장 좁은(또는 가장 늦게 시작한)** 것을 결정적으로 선택. (B2로 누적이 사라지면 겹침 자체가 크게 줄지만, drift off-by-one 등 잔여 겹침에 대한 방어.)

### 테스트/검증
- 각 수정 TDD red-first. golden 테스트(`episode_gold*`)가 read-only Bash 포함 픽스처로 인해 phase 시퀀스가 바뀌면, **정당화 주석과 함께 golden 재생성**(테스트 자체 규칙 준수). `episode_rebuild_writes_rows`는 B2로 행 수가 바뀔 수 있음 — 갱신.
- 브라우저 smoke(무인이므로 **특히 신중**): 바이너리 재빌드 + 재ingest + serve + 스크린샷으로 (i) `Bash: grep` 노드가 더 이상 action 아님, (ii) 겹침/누적 사라짐, (iii) 배지 정확. 스크린샷을 사용자 검증용으로 저장.

## C. 검증 대기 범위 — 깊은 재설계 (사용자 승인 전 구현 금지)

상위 설계가 명시한 다음 항목은 **의미론을 바꾸고 design judgment가 필요**하다. 무인 세션에서 임의 구현하지 않고 사용자 검토를 기다린다:

- **Tier2 phase 삭제** (drift/exploration/diagnosis/repair): 삭제 시 read-only 활동이 "무phase"가 되어 episode 모델(연속 동종 구간)이 sparse 3-driver(intake/action/verification) 세계에서 재정의되어야 함. UX(phase strip이 성김) 결정 포함.
- **classifier를 backbone 이벤트만으로 제한**: 현재 classifier는 observed_event 전체(telemetry 포함)를 순회 → phantom intake 잔존(Slice 1/2는 graph만 정리, classifier 입력 스트림은 그대로). telemetry 이벤트를 분류에서 제외하는 변경.
- **missing_verification raw-derivation**: 현재 Tier1(intake/action/verification) episode를 읽음. B 범위에서 Tier1은 유지되므로 동작 지속 → 재작성 불필요(이번엔). 단 "영속 분류기 폐기/렌더타임" 전환 시 재작성 필요.
- **영속 분류기 폐기 vs 유지**: 상위 설계는 "결정론적 태그 + 렌더타임, 영속·누적 없음"을 선호했으나, B2(delete-후-insert)로 누적은 제거되고 테이블은 유지된다. 완전한 렌더타임 전환은 더 큰 변경 — 사용자 결정.

## D. 자율 결정 플래그 (사용자 검토 요망)

1. **범위 분할**: 무인이므로 관측된 버그(B)만 구현하고 의미론 재설계(C)는 보류 — 사용자가 외출 후 C 방향을 결정. (B는 어떤 C 결정에서도 유효한 수정.)
2. **Bash read-only allowlist**: 휴리스틱. 명령 파싱은 pipe/`&&`/redirection 때문에 불완전 — 보수적(모호하면 Action 유지)으로 잡았으나 allowlist 항목·판정 규칙은 사용자 조정 가능.
3. **겹침 해소 규칙**: "가장 좁은" vs "가장 늦게 시작한" 중 택일 — 구현은 결정적 규칙 하나를 택하고 주석에 명시.

## Non-goals (이번 슬라이스)
- Tier2 삭제, backbone-filter, 렌더타임 전환, missing_verification 재작성 (→ §C, 승인 대기).
- raw_event/observed_event 변경.
