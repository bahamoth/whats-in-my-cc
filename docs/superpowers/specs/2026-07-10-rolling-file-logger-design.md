# 롤링 파일 로거 (serve) — 설계

- 날짜: 2026-07-10
- 상태: 승인됨 (구현 대기)
- 영역: `src/telemetry.rs`, `src/cli.rs`, `src/main.rs`, `.gitignore`, `CLAUDE.md`

## 문제

`wimcc serve`의 로그는 현재 `tracing_subscriber` fmt 레이어를 통해 **stdout으로만**
나간다(`src/telemetry.rs`). 그래서 사람이 띄웠든 AI가 띄웠든, 백엔드 상태를 사후에
보려면 stdout을 캡처했어야 하거나 프로세스를 재기동해 다시 관찰해야 한다 — 재기동
오버헤드와 "지금 무슨 일이 일어나는지 바로 알 수 없음"이 통증이다.

목표: serve가 **항상 알려진 위치의 로그 파일**에 기록하게 해, 실행 방식과 무관하게
`tail -f`로 상시 관측한다. 파일은 **일자별 회전 + 보관수 상한**으로 무한 증가를 막는다.

## 핵심 결정 — 기본 로그 위치 = `--db-path`의 부모 디렉터리

사용자 요구는 "기본 위치 = 프로세스 실행 위치 = 아무 설정 안 했을 때의 DB와 동일한
위치"다. CWD로 못박지 않고 **실행에 쓰인 `--db-path`의 부모 디렉터리**로 파생시킨다.

| 상황 | `--db-path` | 로그 디렉터리 |
|------|-------------|---------------|
| 무설정 | `.wimcc.sqlite`(기본, 부모=빈 경로) | `.` (CWD로 정규화) — 요구 충족 |
| DB 이동 | `/data/x.sqlite` | `/data/` (DB 옆) |
| 로그만 이동 | 임의 | `--log-dir`/`WIMCC_LOG_DIR` 우선 |

이 파생의 부수 효과가 **테스트 위생**을 공짜로 해결한다: subprocess serve를 띄우는
테스트(`cli_serve`·`doctor`·`sse_subprocess`·`events_subprocess`)는 이미 전부
`--db-path <tempdir>/…`를 넘기므로, 로그도 그 tempdir로 들어가 저장소를 오염시키지
않고 tempdir와 함께 정리된다. **기존 테스트 수정 0.** (in-process 테스트
`auth_default_off`·`event_tags`는 axum `TestServer`/순수 함수라 로거 초기화 자체를
안 타므로 무관.)

## 컴포넌트

### 1. `src/cli.rs` — 설정 노출
전역 옵션 2개 추가(비-serve 명령은 무시):
- `--log-dir: Option<PathBuf>` (env `WIMCC_LOG_DIR`).
  help: "Directory for rotating serve logs. Default: same directory as --db-path."
- `--log-retention-days: u16` (env `WIMCC_LOG_RETENTION_DAYS`, default `7`,
  `value_parser = value_parser!(u16).range(1..=365)`).
  help: "How many daily serve log files to keep. Default: 7."

### 2. `src/telemetry.rs` — 로거 조립
- `resolve_log_dir(db_path: &Path, override_dir: Option<&Path>) -> PathBuf`
  순수 함수. `override_dir`가 `Some`이면 그것을, 아니면 `db_path.parent()`를 쓰되
  빈 경로/None이면 `.`(CWD)로 정규화.
- `file_appender(dir: &Path, keep_days: u16) -> RollingFileAppender`
  `tracing-appender` 빌더: `Rotation::DAILY` · `filename_prefix("wimcc")` ·
  `filename_suffix("log")` · `max_log_files(keep_days as usize)`. 파일명은
  `wimcc.YYYY-MM-DD.log`.
- `init(format: &LogFormat, verbose: bool, file: Option<(&Path, u16)>) -> Option<WorkerGuard>`
  (`file` = `Some((dir, keep_days))`일 때만 파일 레이어 부착)
  - EnvFilter는 현행 유지, registry 레벨에 1개 — 콘솔·파일 두 레이어에 공통 적용.
  - 레이어는 `Vec<Box<dyn Layer>>`로 합성:
    - **콘솔 레이어**(현행 유지): 기본 writer(stdout), ANSI 유지, `--log-format`(Pretty/Json).
    - **파일 레이어**(`file`가 `Some`일 때만): `non_blocking(file_appender(dir, keep_days))` →
      `(NonBlocking, WorkerGuard)`. **항상 Pretty + `with_ansi(false)`** — `--log-format`을
      따르지 않는다. 파일은 tail로 사람이 읽는 대상이라 JSON은 불편하고 ANSI 이스케이프는
      파일을 오염시키므로 사람 가독 텍스트로 고정한다(2026-07-10 사용자 결정).
  - 파일 레이어가 있으면 `WorkerGuard`를 반환(없으면 `None`). appender 빌드 실패 시
    `eprintln!` 경고 후 파일 레이어 생략(콘솔만).

### 3. `src/main.rs` — 배선
- 파싱 직후 `file` 계산: serve일 때만 `Some((resolve_log_dir(&cli.db_path,
  cli.log_dir.as_deref()), cli.log_retention_days))`, 그 외 `None`.
- `let _log_guard = telemetry::init(&cli.log_format, cli.verbose, file.as_ref().map(|(d, n)| (d.as_path(), *n)));`
  `_log_guard`는 main 스택에 남아 프로세스 종료 시 drop→flush를 보장한다
  (`WorkerGuard`는 drop 시 버퍼를 비운다).
- serve 경로에서 **해석된 절대 로그 경로를 INFO 1줄로 출력**(콘솔·파일 양쪽에 남음):
  예) `rotating file log enabled dir=/abs/path keep_days=7`. 이 첫 write가 당일
  파일을 생성하므로 파일 존재가 보장된다.

### 4. `.gitignore`
`wimcc.*.log` 추가 — 실제 serve가 저장소 루트에서 돌 때 dated 로그가 커밋에 섞이지
않게(기존 `.wimcc.sqlite` 무시 규칙과 대칭).

### 5. `CLAUDE.md` — 운영 지시문
Operations 섹션에 우선순위 동작과 폴백을 **모두** 명시(사용자 요구). 제안 문구:

> - **serve 로그 관측/디버깅**: 프로세스 상태 확인·디버깅은 **재기동하지 말고 로그
>   파일을 tail**한다(재기동은 상태를 초기화). 파일 위치 우선순위: ①`--log-dir`/
>   `WIMCC_LOG_DIR`(명시 시) → ②없으면 `--db-path`의 부모 디렉터리 → ③그것도
>   비면(기본 `.wimcc.sqlite`) CWD. 파일명 `wimcc.<date>.log`, 일자 회전·기본 7일
>   보관(`--log-retention-days`). serve 시작 로그의 `rotating file log enabled
>   dir=…` 줄에 그 시점 절대 경로가 찍히니 그것을 tail한다.

미니멀 원칙(메모리 [[claude-md-minimalism]])과 충돌하지 않게 Operations 하위 1블록으로
압축하되, 우선순위 3단계와 폴백은 생략하지 않는다.

## 새 의존성
`tracing-appender = "0.2"` — tracing 공식 동반 크레이트. non-blocking writer +
일자 회전 + `max_log_files` 보관수. 무거운 신규 의존성 아님.

## 데이터 흐름
```
main: parse CLI
  → file = serve ? Some((resolve_log_dir(db_path, log_dir), log_retention_days)) : None
  → guard = telemetry::init(format, verbose, file)        // 콘솔(format) + 파일(pretty,no-ansi)
  → (serve) info!("rotating file log enabled …")          // 첫 write, 파일 생성
  → rt.block_on(serve …)                                   // 이후 모든 로그가 양쪽에
  → main 반환 시 guard drop → flush
```

## 테스트 (TDD red-first)
- `resolve_log_dir` 단위: bare filename→`.`, `/data/x.sqlite`→`/data`, override 우선.
- `file_appender` 단위: tempdir에 실제 write→drop 후 `wimcc*.log` 파일 생성 확인
  (실파일 앵커 — Real-data anchoring).
- `--log-dir`·`--log-retention-days` 파싱 단위(`cli.rs` tests): 전역 인자·env·기본값
  (`log_dir=None`, `log_retention_days=7`), 범위(`0`·`366` 거부).
- e2e(`tests/`): `serve --db-path <tempdir>/x --shutdown-after-ms N` 실행 후
  tempdir에 `wimcc.<date>.log` 생성 확인(전체 배선 증명). 파일 생성은 startup
  write에서 즉시 일어나므로 shutdown 이전에 존재; 플레이크 방지 위해 짧은 폴링.
- e2e 파일 포맷: `--log-format json` 으로 serve해도 **파일 내용은 JSON이 아니고
  ANSI 이스케이프(`\x1b[`)가 없음**을 확인(파일=Pretty·no-ANSI 고정 잠금).

## YAGNI (의도적 제외)
- 크기 기반(logrotate식) 회전 — 일자 기반 선택. 필요 시 별도 크레이트로 후속.
- 파일 전용/콘솔 전용 토글 — 콘솔+파일 동시(tee)가 손해 없는 상위집합.
- 파일 JSON 포맷 노출 — 파일은 tail 대상이라 JSON이 불편(2026-07-10 결정). 파일은
  Pretty·no-ANSI 고정. 콘솔은 `--log-format json` 그대로 지원.

## 검증되지 않은 가정 / 열린 질문
- `RollingFileAppender`는 첫 write에서 지연 생성한다고 가정 — e2e에서 startup INFO
  write로 파일 존재를 보장하고 폴링으로 확인해 잠근다.
- `max_log_files`는 `tracing-appender` 0.2.3+ 빌더 API. 실제 해석된 버전에서
  존재를 `cargo build`로 확인한다.
