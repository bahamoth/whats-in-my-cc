//! Extractor implementations, one module per category.
//! Slice-14 ships two L1 deterministic extractors:
//!   - `missing_verification`: action episode with no following verification.
//!   - `tool_failure`: is_error=true with no compensating retry within 5 events.

pub mod missing_verification;
pub mod tool_failure;
