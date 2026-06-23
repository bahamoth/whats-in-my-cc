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
//!   - `final_state_mismatch`: user goal not corroborated in final state.

pub mod context_bloat;
pub mod final_state_mismatch;
pub mod re_read;
pub mod risky_action;
pub mod tool_failure;
