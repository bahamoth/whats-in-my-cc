//! Detector implementations, one module per category. All are deterministic:
//! every emitted `SignalCandidate` promotes directly to a `signal` row (no
//! judge, no pending queue, no confidence floor).
//!
//! Slice-14:
//!   - `tool_failure`: is_error=true tool_result (facts: retried/tool_name/...).
//!
//! Slice-16:
//!   - `risky_action`: destructive Bash command or user_modified diff_hunk.
//!   - `context_bloat`: large tool_result not reused in subsequent turn.
//!
//! (`final_state_mismatch`는 2026-07-03 제거 — 영어 고정 lexical 의미 판별이
//! 원칙과 긴장, 판별은 session-retrospect LLM으로 이관. migration 0027 참고.)

pub mod context_bloat;
pub mod re_read;
pub mod risky_action;
pub mod tool_failure;
