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
