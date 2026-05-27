//! Versioned inference rule files. Each `_v1.rs` has frozen constants.
//! Bumping thresholds requires a new `_v2.rs`, never in-place edits.

pub mod caused_repair_v1;
pub mod large_output_to_next_action_v1;
pub mod triggered_by_user_message_v1;
