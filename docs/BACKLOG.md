# BACKLOG — 남은 작업 (2026-07-04 스윕 2차)

다른 세션이 이어서 작업할 수 있도록 보류·계획 항목을 한곳에 모은 문서.

**2026-07-04 스윕 2차:** B-1~B-14 중 완료 항목 전부 삭제(이력은 git과
implementation-notes 앵커가 SSOT). 남은 것은 아래 **명시적 표본/관문 게이트**
항목뿐이다 — real-data anchoring 원칙상 표본 없이 착수하지 않는다. 게이트
충족 여부는 개선 루프 스크립트(unknown-verification·tagging-gate)가
표면화한다(2026-07-04 게이트 pass = 표본 여전히 0건).

**사용법 (새 세션 진입 순서):**

1. `CLAUDE.md` — 작업 원칙(TDD red 우선, real-data anchoring, 개선 루프 등)을 먼저 읽는다.
2. 이 문서에서 항목을 고른다. 각 항목의 **참조** 앵커가 결정 이력의 SSOT다
   (`docs/implementation-notes.html#<anchor>` — 텍스트 추출은 CLAUDE.md의 python 스니펫).
3. 항목 착수·완료 시 이 문서를 같은 PR에서 갱신한다. 설계 결정·편차는
   implementation-notes에 기록한다(중복 서술 금지 — 여기는 포인터만).

---

## G-1. verification allowlist 생태계 확장 (표본 게이트 — 구 B-2 보류분)

make/bazel/tox/ruff/eslint/ctest — 2026-07-04 코퍼스 전수 스캔에서 실행 표본
0건(이 머신은 Rust+TS 코퍼스). **해당 도구가 코퍼스에 관측되면 그 표본을
`tests/fixtures/**/real/`에 동결해 추가**. unknown-verification 루프가
표면화한다. 참조: implementation-notes `#unknown-verification-loop`.

## G-2. `claude` CLI 태깅 (표본 게이트 — 구 B-7b)

2026-07-04 코퍼스 재스캔에서 `claude` CLI 호출 표본 0건(구 표본 transcript는
정리됨) — 표본 관측 시 추가. untagged-bash 루프가 표면화한다.

## G-3. 성능 재실측 게이트 (구 B-8 잔여)

live tail 재해시(≈10ms/flush)·recompute_session(현 코퍼스 최대 6k행)은 실측상
무통 — **아플 때 재실측**. 기준·실측치는 implementation-notes
`#metrics-cache-2026-07-04`.

## G-4. Detector 후보 졸업 관문 (구 B-10)

`re_edit_churn`·`duplicate_edit_stream` — 회고 LLM의 판별 확정이 **서로 다른
세션 2개 이상**에서 성립할 때만(확정 = 회고 실행 시 fixture 동결 행위) L1
detector화. no-annotation 원칙상 판별 기록은 저장되지 않으므로 관문은 회고
세션에서만 닫힌다. lexical 의미 판별을 L1에 넣지 않는 선례:
`#final-state-mismatch-removal-2026-07-03`.

## G-5. 세션 상세 API 무제한 반환 (표본 게이트 — 2026-07-18 감사)

`session_turns`(`repo_observed.rs::list_session_conversation` LIMIT 없음)·
diff-hunks·verification-runs·signals는 세션 전체 세트를 반환한다. 현 코퍼스
최대 세션(≈25k행)에서는 무통 — **수십만 행 세션이 실제로 관측되어 응답이
아플 때** 커서 페이지네이션을 도입한다. 참조: implementation-notes
`#growth-resource-sweep-2026-07-18`.

## G-6. Linux inotify recursive watch 스케일 (표본 게이트)

`transcript_tail`의 recursive watch는 Linux에서 디렉터리당 watch descriptor를
잡는다(macOS FSEvents는 무관). `~/.claude/projects/**` 하위 디렉터리 수천 개
환경의 실측 표본이 생기면 `max_user_watches` 대응(폴링 폴백 또는 안내)을
넣는다. 참조: implementation-notes `#growth-resource-sweep-2026-07-18`.

## G-7. 증분 ingest 오프셋 (성능 재실측 게이트 — G-3 연동)

live tail은 flush마다 파일 전체를 재해시한다(DEV-S7-01 의도된 트레이드오프,
recompute 스킵과 별개로 남는 비용). G-3과 같은 기준 — **재해시가 실측으로
아플 때**(대형 세션에서 flush 지연 관측) byte-offset 커서를 재검토한다.

## G-8. 인사이트 → 다음 행동 연결 (설계 논의 필요)

대시보드·insight strip은 판정 문장 금지 원칙상 숫자·관측 사실만 보여주고,
판단은 `session-retrospect` 스킬(LLM 온디맨드)로 외주화되어 있다 — 스킬의
존재를 모르는 사용자는 "그래서?"에서 이탈한다(2026-07-18 UX 감사 GAP-5).
원칙을 깨지 않고 스킬로의 발견 가능한 연결(예: 대시보드에서 retrospect 안내
표면)을 설계할지 사용자 논의 후 착수.

## G-9. DetectorManifest intent/rule/rationale 영어 전환

표면 언어 게이트(`tests/it/surface_language.rs`, 2026-07-22 신설)의 유일한
보류 allowlist. `/v1/detectors`·MCP `list_detectors`로 노출되는 API 표면이라
"표면 기본 언어는 영어" 원칙(2026-07-19) 대상이지만, 한글 스펙 미러 문단
12개의 번역 품질 검토 + 참조 테스트 3파일(`detector_manifest`·
`api_detectors`·`mcp_tools_call`) 동반 수정이 필요해 핫픽스에서 분리했다.
전환 완료 시 게이트의 `PENDING_FILES`를 비운다.
