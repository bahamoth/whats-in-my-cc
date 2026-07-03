# BACKLOG — 남은 작업 (2026-07-03 기준)

다른 세션이 이어서 작업할 수 있도록 보류·계획 항목을 한곳에 모은 문서.

**사용법 (새 세션 진입 순서):**

1. `CLAUDE.md` — 작업 원칙(TDD red 우선, real-data anchoring, 개선 루프 등)을 먼저 읽는다.
2. 이 문서에서 항목을 고른다. 각 항목의 **참조** 앵커가 결정 이력의 SSOT다
   (`docs/implementation-notes.html#<anchor>` — 텍스트 추출은 CLAUDE.md의 python 스니펫).
3. 항목 착수·완료 시 이 문서를 같은 PR에서 갱신한다. 설계 결정·편차는
   implementation-notes에 기록한다(중복 서술 금지 — 여기는 포인터만).

**배경 맥락:** 이 목록은 2026-07-03 구현 리뷰 세션(PR #78,
`feat/mcp-parity-and-detector-config`)에서 도출·정리됐다. 당시 완료된 것:
MCP parity 9종·DetectorConfig 파일 로딩·teammate 세션 관측(정규화+가시화)·
final_state_mismatch L1 제거·태깅 루프 2026-07-03분. 식별자는 날짜 기준
(v1/v2 금지).

---

## B-1. 프로젝트 대시보드 라우트 — 완료 (2026-07-04)

`/dashboard` 라우트로 구현(PR #82). 코호트 레일(모델·CC 버전) + verification
outcome 스택 + 프로세스 strip 6종, 공유 세션 축, 모델 집합 변화만 관통 룰.
브라우저 smoke(headless 2회) 완료. 설계 결정은 implementation-notes
`#project-dashboard-2026-07-04`. (이 BACKLOG 갱신은 #82에 누락돼 후속 PR에서
정정 — "같은 PR에서 갱신" 규칙 위반 1회 기록.)

## B-2. verification allowlist 생태계 확장 (부분 완료 2026-07-04, 표본 게이트)

- **완료분**: `tsc` 승격(패턴 17) — 코퍼스 실 표본 3세션을
  `verification_tsc_v01.jsonl`로 동결(DEV-S11-03). `--noEmit`→build_check
  post-match 승격, `pnpm exec` wrapper, tsc 진단 포맷(`: error TS`) 실패
  휴리스틱. implementation-notes `#verification-tsc-2026-07-04`.
- **보류분**: make/bazel/tox/ruff/eslint/ctest — 2026-07-04 코퍼스 전수
  스캔에서 실행 표본 0건(이 머신은 Rust+TS 코퍼스). real-data anchoring
  원칙상 표본 없이 추가하지 않는다 — **해당 도구가 코퍼스에 관측되면 그
  표본을 동결해 추가**(unknown-verification 루프가 표면화한다: allowlist
  진입 후에만 run이 되므로, 관측 여부는 transcript 직접 스캔으로 확인).
- **참조**: implementation-notes `#unknown-verification-loop`.

## B-3. MCP 응답 구조 개선 + 세션 다이제스트 (우선순위 중)

- **무엇**: ① MCP 툴 응답의 단일 text 블록 통 JSON 직렬화
  (`src/api/mcp/tools/mod.rs` `tool_success`) → structured content 검토,
  ② 대형 응답(get_session_turns·get_file_lineage) 페이지네이션,
  ③ 원문 이벤트 창(events) MCP 툴 부재 — 순수 MCP 클라이언트는 raw 이벤트에
  접근 불가, ④ **세션 다이제스트 툴**: 토큰 상한이 설계된 단일 콜
  (집계+Signal 요약+드릴다운 링크) — 전용 스킬 없이도 임의 에이전트가
  "방금 세션에서 뭐가 있었나"를 한 콜로.
- **왜**: 에이전트 소비 목표의 다음 단계. parity(9종)는 완료됐으나 응답
  구조가 토큰 비효율적.
- **주의**: 다이제스트는 순수 집계 조합이어야 함(측정/판별 분리). 절단은
  반드시 `matched_count`류로 노출(no silent truncation — get_otel_trace
  선례).
- **참조**: implementation-notes `#mcp-parity-detector-config-2026-07-03`
  "미해소".

## B-4. export bundle 구현 (우선순위 중, §09 결정 포함)

- **무엇**: `POST /v1/export-bundles` — owner-only local export (PRD에 계획만
  존재, 코드 없음). read-only 원칙의 유일한 예외로 이미 지정돼 있다.
- **왜**: 회고 결과·evidence를 PR·이슈·팀 공유로 반출하는 유일한 경로.
  현재 인사이트는 로컬 브라우저와 LLM 컨텍스트 안에 갇혀 있다.
- **선행 결정 해소(2026-07-04)**: redacted normalized evidence 기본 +
  raw는 explicit opt-in으로 사용자 결정 완료 — B-9 참조. 구현만 남음.
- **참조**: `docs/00_prd_revised.html` §09, CLAUDE.md Non-goals 예외 절,
  implementation-notes `#prd-09-decisions-2026-07-04`.

## B-5. implementation-notes 이원화 (우선순위 중)

- **무엇**: append-only 원장은 유지하되 ① 토픽별 "현재 진실" 인덱스,
  ② 열린 질문만 모은 백로그 추출(→ 이 문서로 흡수 가능)을 분리.
  마크다운 저작 + HTML 생성 방식도 검토.
- **왜**: 텍스트 추출만 1만 줄의 단일 HTML — "에이전트가 소비하기 좋은
  정보를 만드는 프로젝트의 설계 이력이 정작 에이전트가 소비하기 가장 비싼
  형식"(2026-07-03 리뷰). 이 BACKLOG.md가 ②의 첫 걸음이다.
- **참조**: CLAUDE.md "Implementation Notes (지속 유지 의무)" 절과의 정합
  유지 필요.

## B-6. Teammate 후속 묶음 (우선순위 중)

CC 2.1.198의 teammate 실행 모델 대응(0026/0027 라운드)의 잔여:

- **B-6a. 목록 preview 정리**: 팀메이트 세션의 first_user_message_preview가
  raw `<teammate-message …>` XML을 노출 — preview 계산(`session_facets`의
  preview 쿼리, `src/db/repo_observed.rs`)에서 마커 처리.
- **B-6b. 북엔드형 시간축 가시화**: 리드 replay에서 팀메이트 [시작→종료]
  연속 레일은 **그리지 말 것** — 실행 구간이 메시지 경계(디스패치·응답·idle)
  에서만 관측되는 estimated라 measured/estimated 구분을 흐린다. 이산
  북엔드(디스패치/응답 노드) 중심으로 설계.
- **B-6c. `agent-setting` 레코드 정규화**: `agentSetting: "Explore"`(agent
  타입) — 현재 Unknown으로 raw 보존만. 배지 후보.
- **B-6d. system 레코드의 agentName/teamName**: 실측상 붙지만 미승격
  (SystemRecord가 serde flatten 구조). 대화 kind로 충분해 보류했음.
- **B-6e. 표본 확인**: 2.1.198에서 클래식(이름 없는) 서브에이전트가 여전히
  사이드카인지 미관측 — 표본 확보 시 notes 갱신. teamName의
  "session-<리드 8자>" 형태는 **표본 1** — 조인 로직 확장 전 fixture 추가.
- **참조**: implementation-notes `#teammate-observability-2026-07-03`,
  `#teammate-in-session-2026-07-03`. fixture:
  `tests/fixtures/transcripts/real/teammate_v01/`. 조인 SSOT:
  `webui/src/lib/teamGrouping.ts`.

## B-7. 태깅 인프라 (우선순위 중하)

- **B-7a. `$()` 서브셸 토크나이저**: untagged 잔존의 주원인(pr·rev-parse·
  status·ls-files·tr·log·read — 파이프/서브셸 내부 토큰). 토크나이저가
  `$()`·`< <()` 내부를 세그먼트로 인식해야 함.
- **B-7b. `claude` 멀티플렉서**: 표본이 `plugins --help` 1형태뿐이라 보류
  (오태깅 위험) — 표본 축적 시 TOOL_SUBCOMMAND_TAGS 추가.
- **B-7c. 기결정 인지**: unidentified-plugins 스크립트가 intentionally
  unmatched 항목(serena `activate_project`/`onboarding` —
  `src/insight/event_tags.rs`의 SERENA_TOOLS 주석)을 매 루프 재표면화 —
  스크립트에 기결정 목록 필터 추가.
- **B-7d. PR-전 게이트 훅**: Noise disposition으로 untagged가 진짜 후보만
  남게 됐으므로 "보편 후보 잔존 시 차단" 훅이 유의미해짐(6/30 노트 언급).
- **참조**: implementation-notes `#tagging-loop-2026-07-03`,
  `#noise-disposition-2026-06-30`, `#untagged-bash-loop`,
  `#unidentified-plugins-loop`.

## B-8. 성능 부채 (§10.1 게이트 — 아플 때 착수)

셋 다 같은 조건(대형 세션·잦은 조회)에서 동시에 아파진다:

- live tail이 변경 파일 **전체**를 매 flush 재해시(바이트 커서 없음 —
  `src/transcript_tail.rs` 모듈 헤더의 의도된 트레이드오프, DEV-S7-01).
- `recompute_session`/turn_id backfill이 세션당 최대 10만 행 인메모리 로드
  (`src/ingest/store.rs`).
- SessionMetrics 매 호출 full-scan 재계산(무캐시 — `src/insight/metrics.rs`
  헤더에 의도 명시).
- **원칙**: 캐싱·커서는 호출 빈도가 정당화할 때(§10.1). 착수 전 실측 먼저.

## B-9. PRD §09 열린 결정 — 완료 (2026-07-04)

세 건 모두 사용자 결정으로 종결. ① raw API body: local_full_evidence
**기본 포함**(수집원 도입 시 — 현재 수집원 없어 실수집 변화 없음).
② MCP LAN 접근 **불허**(localhost 전용 확정). ③ export bundle
**redacted 기본 + raw explicit opt-in**(B-4 계약). PRD §09·05 spec
동기화 완료 — implementation-notes `#prd-09-decisions-2026-07-04`.

추가 유의(결정 아님): OTLP 수집기는 인증 예외 — `--bind 0.0.0.0` 설정 시
무인증 수신 노출. local-first 전제상 현재 무해하나 LAN 불허 확정과 정합
(bind 변경은 사용자 explicit 설정 책임).

## B-10. Detector 후보 졸업 경로 (관문 준수)

- 보류 중 후보: `re_edit_churn`, `duplicate_edit_stream`
  (session-retrospect workflow.md "판별→fixture 승격" 절).
- **관문**: 같은 구조 패턴이 **서로 다른 세션 2개 이상**에서 확정되면 실
  payload를 `tests/fixtures/**/real/`에 동결하고 invariant 테스트로 잠근
  뒤에만 detector화. 표본 축적은 annotation이 아니라 fixture로
  (no-annotation 원칙).
- final_state_mismatch 제거(2026-07-03)로 lexical 의미 판별을 L1에 넣지
  않는 선례가 강화됨 — 후보 설계 시 참고
  (`#final-state-mismatch-removal-2026-07-03`).

## B-11. 소형 정리

- `SessionMetrics`가 detector별 하드코딩 필드(`tool_failure_count`,
  `context_bloat_count`)와 generic `detector_firing` map을 중복 유지
  (`src/insight/metrics.rs`) — 소비자 정리 후 통합 검토 (API 필드 제거는
  breaking).
- unknown-verification의 "빈 출력" 클래스: 성공 시 transcript에 exit 0
  신호가 없어 구조적 unknown — 코드 대상 아님. OTLP tool_result 수집이
  있으면 measured로 해소되는 클래스라, doctor/문서에서 OTLP 수집을 안내하는
  것이 실질 해법.
