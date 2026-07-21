//! 표면 언어 게이트 — 프로젝트의 사용자 가시 문자열은 영어여야 한다
//! (2026-07-19 사용자 지시; 코드 주석·docs는 한국어 유지).
//!
//! v1.6.0 실사고(2026-07-22): `wimcc service restart`가 "재시작 완료"를
//! 출력 — i18n 정리 커밋이 `println!`과 같은 줄의 리터럴만 바꿔, if 분기
//! 안의 다른 줄 리터럴을 놓쳤다. 줄 단위 grep 검증은 이 부류를 못 잡으므로
//! 문자열 리터럴 단위로 스캔하는 이 게이트가 재발을 막는다.

use std::path::Path;

/// 주석을 제외한 문자열 리터럴 내용만 모은다. 간이 스캐너 — `//` 주석,
/// `"…"` 이스케이프를 처리한다(블록 주석·raw string은 이 코드베이스의
/// 사용자 가시 문자열 표면에 없으므로 다루지 않는다).
fn string_literal_chars(line: &str) -> String {
    let mut out = String::new();
    let mut chars = line.chars().peekable();
    let mut in_string = false;
    while let Some(c) = chars.next() {
        if in_string {
            match c {
                '\\' => {
                    chars.next();
                }
                '"' => in_string = false,
                _ => out.push(c),
            }
        } else {
            match c {
                '"' => in_string = true,
                '/' if chars.peek() == Some(&'/') => break,
                _ => {}
            }
        }
    }
    out
}

fn is_hangul(c: char) -> bool {
    ('\u{AC00}'..='\u{D7A3}').contains(&c) || ('\u{1100}'..='\u{11FF}').contains(&c)
}

/// 보류 allowlist — DetectorManifest intent/rule/rationale(API·MCP 노출)의
/// 한글 스펙 미러 문단. 영어 전환은 BACKLOG G-9(테스트 3파일 동반 수정 필요).
const PENDING_FILES: &[&str] = &["src/insight/extractors/"];

#[test]
fn src_string_literals_contain_no_hangul() {
    let mut violations = Vec::new();
    for entry in walkdir::WalkDir::new(Path::new(env!("CARGO_MANIFEST_DIR")).join("src")) {
        let entry = entry.unwrap();
        if entry.path().extension().is_none_or(|e| e != "rs") {
            continue;
        }
        let path_str = entry.path().to_string_lossy().replace('\\', "/");
        if PENDING_FILES.iter().any(|p| path_str.contains(p)) {
            continue;
        }
        let text = std::fs::read_to_string(entry.path()).unwrap();
        for (i, line) in text.lines().enumerate() {
            // 테스트 모듈(파일 말미 관행)의 assert 메시지·한글 픽스처 데이터는
            // 사용자 표면이 아니다 — cfg(test)부터 파일 끝까지 제외.
            if line.trim_start().starts_with("#[cfg(test)]") {
                break;
            }
            let literals = string_literal_chars(line);
            if literals.chars().any(is_hangul) {
                violations.push(format!(
                    "{}:{}: {}",
                    entry.path().display(),
                    i + 1,
                    line.trim()
                ));
            }
        }
    }
    assert!(
        violations.is_empty(),
        "user-visible surface language is English — Korean found in src string literals:\n{}",
        violations.join("\n")
    );
}

#[test]
fn scanner_separates_comments_from_literals() {
    // 주석의 한글은 허용 대상이므로 스캐너가 리터럴만 골라내는지 잠근다.
    assert_eq!(
        string_literal_chars(r#"bail!("지원하지 않는 OS")"#),
        "지원하지 않는 OS"
    );
    assert_eq!(string_literal_chars(r#"    "재시작 완료""#), "재시작 완료");
    assert_eq!(string_literal_chars("// 재시작(crash-loop)된다"), "");
    assert_eq!(string_literal_chars(r#"println!("ok") // 한글 주석"#), "ok");
    assert_eq!(
        string_literal_chars(r#""https://a.b" // url"#),
        "https://a.b"
    );
    assert_eq!(string_literal_chars(r#""esc \" 안""#), "esc  안");
}
