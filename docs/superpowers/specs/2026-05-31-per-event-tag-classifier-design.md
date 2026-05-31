# Per-event 태그 분류기 — Design Spec

- 날짜: 2026-05-31
- 브랜치: `per-event-tag-classifier` (`episode-phase-removal` 위 스택)
- 선행: episode/phase 제거(`2026-05-31-episode-phase-removal-design.md`). phase 배지가 사라진 자리에, 각 이벤트가 *실제로 한 일*을 사실대로 보여주는 per-event 태그를 넣는다.
- 결정 출처: 이 세션의 brainstorming 대화 (taxonomy·확장성·미매치 노출·실측 모두 사용자와 합의).

## 1. 목적 / 원칙

- 분류는 **이벤트**에 붙는다(파생물 episode 아님). 태그는 **렌더타임·로컬·경량** — 새 API 호출 없음, 교차-이벤트 참조 없음.
- 도구 이름이 의도를 안 알려주는 **Bash**(2870건)와 **Read**(1364건)에만 태그가 가치 있다. Edit/Write/Agent/MCP 등은 도구명이 곧 라벨이라 태그 없음.
- **모호/복합 명령은 태그 없이 명령 원문을 그대로 보여준다** — phase 분류기를 망친 "셸 파서 없이 추측" 함정을 반복하지 않는다.
- **확장 용이**: 태그 규칙은 단일 소스 데이터 테이블. 새 도구/언어 = 키 한 줄 추가.
- **미매치 가시화**: 인식 못 한 simple 명령 패턴을 dev 패널이 라이브로 보여줘 테이블 확장을 손쉽게 한다.

## 2. Taxonomy (실데이터 근거, 확정)

전 세션 실측 분포에 근거한다(추측 아님).

### Read (도구) — `input.file_path` 확장자
| 태그 | 확장자 | 실측 건수 |
|---|---|---|
| `code` | .rs .ts .tsx .css .js .jsx | rs 659, ts(x) 451, css 47 |
| `docs` | .md .html .txt | md 64, html 37 |
| `config` | .toml .yaml .yml .ini | (소수) |
| `data` | .json .sql .jsonl .log .output | sql 43, json 19 |
| (없음) | 그 외 / 확장자 없음 | |

### Bash (도구) — 첫 의미 토큰 + git 서브커맨드
| 태그 | 첫 토큰 | 실측 |
|---|---|---|
| `search·read` | grep rg egrep fgrep find ls cat head tail wc jq tree which file stat du df pwd env | grep 518, find 138, ls 110, cat 53, jq 31, wc 14 … |
| `vcs-read` | git {status log diff show branch blame rev-parse describe} | diff 64, show 59, branch 42, log 33, status 21 |
| `vcs-write` | git {add commit push checkout switch stash rm reset merge rebase fetch pull tag clone} | add 66, checkout 20, commit 11, push 9, stash 8 |
| `build·test` | cargo npm npx pnpm yarn make tsc vitest pytest go | cargo 269, npm 45, npx 31 |
| `query·script` | sqlite3 python3 python node osascript psql ruby | sqlite3 185, python3 97, osascript 19 |
| `destructive` | rm mv rmdir (및 마커 오버라이드) | (소수, 안전상 명시) |
| `control` (칩 없음, **미매치 아님**) | cd echo sleep for export source set pgrep kill wait true : | cd 672, echo 105, sleep 14 |
| (미매치 → 패널) | 인식 못 한 simple 명령 첫 토큰 | gh 16, curl 10 … (테이블 확장 후보) |

### 그 외 도구
Edit/Write/MultiEdit/Agent/Task*/Skill/Workflow/MCP/WebSearch/WebFetch → **태그 없음**(도구명이 라벨).

## 3. 분류 로직 — `tagForEvent(event)` (순수·렌더타임·로컬)

반환: `{ tag: string | null, disposition: 'tagged' | 'control' | 'ambiguous' | 'unmatched' }`.

**Read**: `input.file_path` 확장자를 `READ_EXT_TAGS`로 조회 → `{tag, 'tagged'}`. 확장자 없거나 미등록 → `{null, 'unmatched'}`(드물어 패널에 거의 안 뜸; Read 미매치도 확장자 추가로 해소).

**Bash**: `input.command` 기준,
1. **복합/리다이렉트 연산자**(`&&` `||` `;` `|` `>` `>>` `<` `$(` 백틱) 포함 → `{null, 'ambiguous'}`. 칩 없음, 명령 원문 표시. (패널에 안 뜸 — 테이블로 못 고침.)
2. 아니면 **simple 명령**: 첫 토큰 추출.
   - `rm`/`mv`/`rmdir` → `{destructive, 'tagged'}` (오버라이드).
   - `git` → 2번째 토큰을 `GIT_SUBCOMMAND_TAGS`로 → vcs-read/vcs-write. 미등록 서브커맨드 → `{null,'unmatched'}`.
   - `BASH_FIRST_TOKEN_TAGS[token]` 있으면 → `{tag,'tagged'}`.
   - `CONTROL_TOKENS`에 있으면 → `{null,'control'}` (칩 없음, **패널 제외**).
   - 그 외 → `{null,'unmatched'}` (**패널에 표시** — 테이블 확장 후보).

**그 외 도구**: `{null, 'control'}` 취급(칩 없음, 패널 제외).

→ **패널에 뜨는 것은 `disposition==='unmatched'`인 simple 명령뿐** — 테이블에 키를 추가하면 다음 렌더에 자동으로 빠진다(단일 소스 파생).

## 4. 단일 소스 테이블 — `webui/src/components/replay/stream/eventTags.ts`

```ts
export type ReadTag = 'code' | 'docs' | 'config' | 'data';
export type BashTag = 'search·read' | 'vcs-read' | 'vcs-write' | 'build·test' | 'query·script' | 'destructive';

export const READ_EXT_TAGS: Record<string, ReadTag> = { rs:'code', ts:'code', tsx:'code', /* … */ md:'docs', /* … */ json:'data', sql:'data' };
export const BASH_FIRST_TOKEN_TAGS: Record<string, BashTag> = { grep:'search·read', /* … */ cargo:'build·test', sqlite3:'query·script' };
export const GIT_SUBCOMMAND_TAGS: Record<string, BashTag> = { status:'vcs-read', /* … */ commit:'vcs-write' };
export const DESTRUCTIVE_FIRST_TOKENS = new Set(['rm','mv','rmdir']);
export const CONTROL_TOKENS = new Set(['cd','echo','sleep','for','export','source','set','pgrep','kill','wait','true',':']);
export const BASH_COMPOUND_MARKERS = ['&&','||',';','|','>','>>','<','$(','`'];

export function tagForEvent(e: ObservedEventDto): TagResult { /* §3 로직 */ }
```
**태그 추가 = 이 파일의 맵에 키 한 줄.** 단일 소스 — 칩·패널·테스트가 전부 여기서 파생.

## 5. 표시 (스트림)

- `ActivityStack`(activity-run) 안 각 이벤트 행에, `disposition==='tagged'`면 도구명 옆 **작은 칩**: 예 `Bash · search·read`, `Read · code`. 칩 없으면 도구명 + (Bash는) 명령 원문 그대로.
- 경량: 칩은 텍스트 span 하나. run은 접힌 상태 유지(가시성). Bash/Read 외 도구엔 칩 없음.

## 6. Untagged dev 패널 — `UntaggedBashPanel`

- 세션 뷰에 **기본 숨김·토글·소형 footprint**(예: 우하단 토글 또는 dev 플래그). 평소 공간 차지 안 함.
- 현재 세션 이벤트에서 `disposition==='unmatched'`인 것을 **라이브 집계**: `첫토큰 · 건수 · 샘플 명령(1)`. 빈도 내림차순.
- 각 행에 **추가 힌트**: `eventTags.ts의 BASH_FIRST_TOKEN_TAGS에 '<토큰>': '<태그>' 추가` — 코딩 에이전트/사람이 보고 즉시 테이블 확장.
- 단일 소스 파생이라 **규칙 추가 시 다음 렌더에 해당 토큰 자동 제거**.
- (참고용) `ambiguous`(복합) 건수도 한 줄 요약 — 테이블로 못 고치는 것이라 구분.

## 7. 실측 (스펙 필수)

- `SessionDetailPage`의 메모이즈된 `buildStreamModel(...)` 계산을 `performance.now()`로 감싸 ms 로깅(dev). 고정 세션(예: 2c5d9a5a, 그리고 대형 세션 하나)에서:
  - **baseline**: 태그 추가 *전* 브랜치(`episode-phase-removal`) 계산 시간.
  - **after**: 태그 추가 후 계산 시간.
- **Network 탭으로 신규 요청 0건** 확인(태그는 이미 로드된 이벤트에서 계산).
- **합격선**: 대형 세션에서 태그 계산 추가분 **< ~5ms**, 신규 fetch **0**. 회귀 시 원인 분석.
- 결과(before/after ms, request 0)를 PR 본문에 기록.

## 8. 아키텍처 / 파일 / 테스트

- 신규: `webui/src/components/replay/stream/eventTags.ts`(테이블 + `tagForEvent` + `collectUntagged(events)`), `UntaggedBashPanel.tsx`(+ module.css).
- 수정: `buildStreamModel`이 각 `ActivityEvent`에 `tag` 부착(또는 `ActivityStack`이 `tagForEvent`를 직접 호출 — 둘 중 단순한 쪽, 계획에서 확정). `ActivityStack.tsx` 칩 렌더. `SessionDetailPage.tsx` 패널 토글 마운트.
- **테스트 (TDD red-first)**:
  - `tagForEvent` 단위 — **실데이터 첫토큰 앵커**: grep→search·read, `git commit`→vcs-write, `git diff`→vcs-read, `cargo test`→build·test, sqlite3→query·script, rm→destructive, `cd x && grep`→ambiguous(null), `cd`→control(null), `gh`(미등록)→unmatched, Read `.rs`→code, `.md`→docs.
  - `collectUntagged` — unmatched만 집계, control/ambiguous 제외, 규칙 추가 시 제외됨.
  - `UntaggedBashPanel` 렌더 — 토큰·건수·힌트 표시.
  - `tagForEvent`가 순수·O(1)/이벤트임을 보장(교차참조 없음).

## 9. 열린 결정 / Non-goals

- **결정**: 태그를 `ActivityEvent`에 미리 부착(buildStreamModel) vs 렌더 시점에 `ActivityStack`에서 호출 — 계획에서 단순한 쪽 택일(둘 다 O(N) 로컬, 성능 동일).
- **결정**: dev 패널 토글 위치/노출 방식(우하단 버튼 vs dev 플래그) — 계획/구현에서 최소 footprint로.
- **Non-goals**: 태그 영속화(렌더타임만), 백엔드 변경, 교차-이벤트 태그(예: "에러 후 고침"), Bash 복합 명령의 정밀 파싱(의도적으로 ambiguous 처리). missing_verification 재파생(별개).
