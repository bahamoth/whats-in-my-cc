# BACKLOG — 남은 작업 (2026-07-04 갱신)

다른 세션이 이어서 작업할 수 있도록 보류·계획 항목을 한곳에 모은 문서.

**2026-07-04 스윕:** B-1~B-11 전 항목 처리 완료. 남은 것은 명시적 표본/관문
게이트뿐이다 — B-2 보류분(비Rust 검증 도구: 코퍼스 표본 관측 시), B-7b
(claude CLI: 표본 관측 시), B-8 잔여 2건(성능: 실측상 무통 — 아플 때),
B-10(detector 후보: 서로 다른 세션 2개+ 확정 시). 게이트 조건과 착수 절차는
각 항목에 기록돼 있다.

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

## B-3. MCP 응답 구조 개선 + 세션 다이제스트 — 완료 (2026-07-04)

(implementation-notes `#mcp-digest-events-2026-07-04`) 9종→11종:

- **① structuredContent**: `tool_success`가 MCP 2025-06-18 구조화 출력
  (`structuredContent`) + 하위호환 text 블록을 함께 싣는다.
- **② 페이지네이션**: `get_session_turns`에 limit(기본 100)/offset —
  절단은 `total_count`로 노출. (get_file_lineage는 소형 응답이라 보류.)
- **③ `get_session_events`**: 원문 이벤트 창 — HTTP events와 같은 커서
  계약(prev/next_cursor, tip이면 next=null).
- **④ `get_session_digest`**: 토큰 상한 설계 단일 콜 — summary+fingerprint
  +SessionMetrics+signal 절단 목록(total/returned) + 드릴다운 links. 순수
  결정론 집계 조합(판단 문장 없음). 00/01/02/04 spec·회고 스킬 치트시트
  동기화.

## B-4. export bundle — 완료 (2026-07-04)

`POST /v1/export-bundles` 구현(implementation-notes
`#export-bundle-2026-07-04`). kind=session, 기본 redacted normalized
evidence + raw explicit opt-in(§09-3 결정), 번들은
`$WIMCC_CONFIG_DIR/exports/` 로컬 JSON + sha256, audit
`export_bundle_created` 기록. signal/lineage kind는 세션 번들이
상위집합이라 필요 관측 시 추가. 04·05 spec·CLAUDE.md 예외 절 동기화.

## B-5. implementation-notes 이원화 — 완료 (2026-07-04)

① 토픽별 "현재 진실" 인덱스 = `docs/notes-index.md`(마크다운 — 에이전트
소비 비용 해소; 항목 추가 시 같은 PR에서 갱신). ② 열린 질문 분리 = 이
BACKLOG.md(기수행). ③ 마크다운 저작+HTML 생성은 **기각** — 원장 재저작은
기존 앵커(BACKLOG·커밋·스킬이 참조)를 깨고 파이프라인 유지비 대비 이득이
없다. 원장은 append-only HTML 유지, 소비는 인덱스+앵커 추출로.
CLAUDE.md Document Map에 인덱스 등재.

## B-6. Teammate 후속 묶음 — 완료 (2026-07-04)

CC 2.1.198 teammate 실행 모델 대응 잔여 전부 처리
(implementation-notes `#teammate-followups-2026-07-04`):

- **B-6a 완료**: preview에서 `<teammate-message>` 래퍼 스트립(직접·relayed
  두 형태 모두 본문 노출) — `strip_teammate_wrapper`.
- **B-6b 완료**: 이산 북엔드 — teammate 응답 카드·Agent 디스패치 라벨을
  agentColor로 페어링, 응답→디스패치 점프(`dispatchEventId`, 창에 없으면
  생략). 연속 레일은 설계대로 그리지 않음.
- **B-6c 완료**: `agent-setting` → session_state/agent_setting 정규화 +
  `SessionDetail.agent_setting` live 필드 + TeamStrip 배지.
- **B-6d 보류 확정(재확인)**: system 레코드 agentName/teamName — 표본 2에서도
  실림 확인. 대화 kind가 세션 상수로 이미 제공하므로 승격은 정보 무추가.
- **B-6e 완료**: 이 세션(CC 2.1.200)에서 직접 스폰해 실측 — **클래식(이름
  없는) 서브에이전트는 여전히 사이드카**, named는 별도 세션(teamName
  "session-<리드 8자>" **표본 2**). fixture `teammate_v02/` 동결 + invariant
  테스트.

## B-7. 태깅 인프라 — 완료 (2026-07-04)

(implementation-notes `#tagging-infra-2026-07-04`)

- **B-7a 완료**: `$()`/`<()` 내부를 세그먼트로 편평화(`extract_command_subs`
  — 바깥 우선, 자리표시자로 오염 차단, 이스케이프 제외, quote-naive 유지).
  잔존 주원인(pr·rev-parse 등) 해소 + 서브셸에서 표면화된 `seq`는 Control.
  같은 라운드에 **무확장 경로 규칙**(디렉토리/미지 무확장 → read.file/
  write.file, Makefile은 FILENAME_OBJECT code)로 디렉토리 Read 클래스 종결.
- **B-7b 보류 유지 확정**: 2026-07-04 코퍼스 재스캔에서 `claude` CLI 표본
  0건(구 표본 transcript는 정리됨) — 표본 관측 시 추가.
- **B-7c 완료**: 기결정 목록(`INTENTIONALLY_UNMATCHED_MCP`,
  `webui/src/lib/taggingGate.ts` — 결정 SSOT는 Rust SERENA_TOOLS 주석)을
  unidentified-plugins 스크립트가 필터.
- **B-7d 완료**: `webui/scripts/tagging-gate.ts` — untagged count≥2(비
  baseline)·community MCP 미태깅 시 exit 1. 보류는
  `tagging-gate-baseline.json`(토큰→사유) 커밋으로. CLAUDE.md 개선 루프
  절에 편입. CC hook이 아니라 repo 스크립트(Non-goal 준수).

## B-8. 성능 부채 — 실측 완료, 1건 해소 (2026-07-04)

§10.1 원칙대로 실측 후 판단(implementation-notes `#metrics-cache-2026-07-04`):

- **SessionMetrics 해소**: 실측 6003-이벤트 세션 232ms/콜, series(18세션)
  1.2s — B-1 대시보드가 series를 인터랙티브 경로로 만들어 게이트 충족.
  프로세스 수명 인메모리 캐시(키: event_count+last_observed_at) 도입 —
  warm 단일 세션 2ms(116배), series 0.19s(7배).
- **live tail 재해시 게이트 유지**: 최대 transcript 33.4MB의 sha256
  ≈ 10ms/flush — 무통(실측 2026-07-04).
- **recompute_session 게이트 유지**: 현 코퍼스 최대 세션 6k행 — 10만 행
  상한과 거리가 멀어 무통. 아플 때 재실측.

## B-9. PRD §09 열린 결정 — 완료 (2026-07-04)

세 건 모두 사용자 결정으로 종결. ① raw API body: local_full_evidence
**기본 포함**(수집원 도입 시 — 현재 수집원 없어 실수집 변화 없음).
② MCP LAN 접근 **불허**(localhost 전용 확정). ③ export bundle
**redacted 기본 + raw explicit opt-in**(B-4 계약). PRD §09·05 spec
동기화 완료 — implementation-notes `#prd-09-decisions-2026-07-04`.

추가 유의(결정 아님): OTLP 수집기는 인증 예외 — `--bind 0.0.0.0` 설정 시
무인증 수신 노출. local-first 전제상 현재 무해하나 LAN 불허 확정과 정합
(bind 변경은 사용자 explicit 설정 책임).

## B-10. Detector 후보 졸업 경로 (관문 유지 — 2026-07-04 재확인)

- 보류 중 후보: `re_edit_churn`, `duplicate_edit_stream`
  (session-retrospect workflow.md "판별→fixture 승격" 절).
- **2026-07-04 관문 확인**: 회고 LLM의 판별 확정 기록이 서로 다른 세션
  2개 이상에서 존재하지 않는다(no-annotation 원칙상 판별 기록은 저장되지
  않으므로, 확정은 회고 실행 시 fixture 동결 행위로만 성립). **관문 미충족
  — 보류 유지.** 다음 회고에서 같은 구조 패턴을 확정하면 그 자리에서
  payload를 `tests/fixtures/**/real/`에 동결하고 invariant로 잠근 뒤에만
  detector화한다.
- final_state_mismatch 제거(2026-07-03)로 lexical 의미 판별을 L1에 넣지
  않는 선례가 강화됨 — 후보 설계 시 참고
  (`#final-state-mismatch-removal-2026-07-03`).

## B-11. 소형 정리 — 완료 (2026-07-04)

- **① 검토 종결(유지 결정)**: 하드코딩 필드(`tool_failure_count`·
  `context_bloat_count`)의 소비자 실측 — AnalysisPanel(비율 계산)·
  대시보드 strip·API 계약 3곳. 제거는 breaking이고 소비자가 실재하므로
  **유지**. 신규 detector는 `detector_firing` map으로만 노출(현행 구조
  그대로) — 하드코딩 추가 금지.
- **② doctor OTLP 안내 구현**: transcript는 있는데 OTLP 소스가 전부
  no_data면 "성공 verification이 구조적 unknown으로 남는 클래스 — OTLP
  수집이 measured로 해소" 권고를 출력(단위 테스트 2건).
