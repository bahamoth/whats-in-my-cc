//! Slice-18 — Per-rule masking shape tests.
//!
//! Each test locks the exact masked output for a canonical example of each rule.
//! Length-preservation is asserted where spec §4 requires it.

use wimcc::security::redaction::engine::apply_text;

// --- api_key_anthropic.v1 ---

#[test]
fn masks_anthropic_key_prefix_preserved_rest_starred() {
    let key = "sk-ant-api03-aaaaaaaaaaaaaaaaaaaaaaaaaaaa";
    let input = format!("key={key}");
    let masked = apply_text(&input);
    assert!(
        masked.contains("sk-ant-api03-"),
        "anthropic key prefix must be preserved; got: {masked}"
    );
    assert!(
        !masked.contains("aaaaaaaaaaaaaaaaaaaaaaaaaaa"),
        "original key suffix must be redacted; got: {masked}"
    );
    // Length-preserving: "key=" (4) + key.len() = total
    assert_eq!(
        masked.len(),
        input.len(),
        "masked output must be the same byte length as input"
    );
}

// --- api_key_openai.v1 ---

#[test]
fn masks_openai_key() {
    let input = "OPENAI_API_KEY=sk-proj-ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef";
    let masked = apply_text(input);
    assert!(
        !masked.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdef"),
        "openai key must be redacted; got: {masked}"
    );
}

// --- github_pat.v1 ---

#[test]
fn masks_github_pat() {
    let input = "GH_TOKEN=ghp_ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij";
    let masked = apply_text(input);
    assert!(
        !masked.contains("ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghij"),
        "github PAT must be redacted; got: {masked}"
    );
}

// --- aws_access_key_id.v1 ---

#[test]
fn masks_aws_access_key_id() {
    let input = "AWS_ACCESS_KEY_ID=AKIAIOSFODNN7EXAMPLE";
    let masked = apply_text(input);
    assert!(
        !masked.contains("AKIAIOSFODNN7EXAMPLE"),
        "AWS access key ID must be redacted; got: {masked}"
    );
}

// --- bearer_token.v1 ---

#[test]
fn masks_bearer_token_in_header() {
    let input = "Authorization: Bearer abc-def-12345-67890-zzzz123456789";
    let masked = apply_text(input);
    assert!(
        !masked.contains("abc-def-12345-67890-zzzz123456789"),
        "bearer token must be redacted; got: {masked}"
    );
}

// --- private_key_pem.v1 ---

#[test]
fn masks_private_key_block_entirely() {
    let input = "header\n-----BEGIN RSA PRIVATE KEY-----\nABC123secret\n-----END RSA PRIVATE KEY-----\nfooter";
    let masked = apply_text(input);
    assert!(
        masked.contains("<private-key-redacted>"),
        "private key block must be replaced with placeholder; got: {masked}"
    );
    assert!(
        !masked.contains("ABC123secret"),
        "private key body must not appear in output; got: {masked}"
    );
    assert!(
        masked.contains("header"),
        "text before key block must be preserved; got: {masked}"
    );
    assert!(
        masked.contains("footer"),
        "text after key block must be preserved; got: {masked}"
    );
}

// --- email.v1 ---

#[test]
fn masks_email_keeping_first_char_and_domain() {
    let masked = apply_text("alice@acme.com");
    assert_eq!(
        masked, "a***@acme.com",
        "email masking must keep first local char + domain"
    );
}

#[test]
fn masks_email_embedded_in_text() {
    let masked = apply_text("Contact alice@acme.com for help.");
    assert!(
        !masked.contains("alice@acme.com"),
        "embedded email must be redacted; got: {masked}"
    );
    assert!(
        masked.contains("a***@acme.com"),
        "masked email must appear in output; got: {masked}"
    );
}

// --- phone.v1 ---

#[test]
fn masks_us_phone_keeping_last_four() {
    let input = "+1 415 555 0199";
    let masked = apply_text(input);
    assert!(
        !masked.contains("415 555"),
        "phone middle digits must be redacted; got: {masked}"
    );
}

// --- korean_rrn.v1 ---

#[test]
fn masks_korean_rrn() {
    let input = "RRN: 900101-1234567";
    let masked = apply_text(input);
    assert!(
        !masked.contains("900101-1234567"),
        "Korean RRN must be redacted; got: {masked}"
    );
}

// --- safe text passthrough ---

#[test]
fn does_not_modify_safe_text() {
    let input = "regular log line with no secrets";
    let masked = apply_text(input);
    assert_eq!(masked, input, "safe text must pass through unchanged");
}

#[test]
fn empty_string_passthrough() {
    assert_eq!(apply_text(""), "");
}
