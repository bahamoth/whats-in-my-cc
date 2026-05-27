//! `FixtureJudge` — replays recorded verdicts from a JSON file (slice-15).
//!
//! Used by tests and smoke scenarios for deterministic judge output without
//! real LLM calls. Key format in the file: `"category||evidence_hash"`.
//!
//! `judge_with_hash()` is a test helper that bypasses the SHA-256 derivation
//! to allow tests to address verdicts by pre-known hash strings.

use std::collections::HashMap;
use std::path::Path;

use crate::insight::judge::cache::evidence_hash;
use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

pub struct FixtureJudge {
    table: HashMap<(String, String), JudgeVerdict>,
}

impl FixtureJudge {
    /// Load the fixture JSON file. Key format: `"category||evidence_hash"`.
    pub fn load(path: &Path) -> anyhow::Result<Self> {
        let raw: HashMap<String, JudgeVerdict> =
            serde_json::from_reader(std::fs::File::open(path)?)?;
        let table = raw
            .into_iter()
            .map(|(k, v)| {
                let mut parts = k.splitn(2, "||");
                let cat = parts.next().unwrap_or("").to_string();
                let hash = parts.next().unwrap_or("").to_string();
                ((cat, hash), v)
            })
            .collect();
        Ok(Self { table })
    }

    /// Test helper: look up verdict by supplying the evidence hash directly,
    /// bypassing SHA-256 derivation. Allows tests with pre-known hash strings.
    pub async fn judge_with_hash(
        &self,
        p: JudgePrompt,
        hash: &str,
    ) -> Result<JudgeVerdict, JudgeError> {
        let key = (p.category.clone(), hash.to_string());
        self.table.get(&key).cloned().ok_or_else(|| {
            JudgeError::Schema(format!(
                "FixtureJudge: no entry for {}||{}",
                p.category, hash
            ))
        })
    }
}

#[async_trait::async_trait]
impl JudgeProvider for FixtureJudge {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        let hash = evidence_hash(&p.evidence_projection);
        let key = (p.category.clone(), hash.clone());
        self.table.get(&key).cloned().ok_or_else(|| {
            JudgeError::Schema(format!(
                "FixtureJudge: no entry for {}||{}",
                p.category, hash
            ))
        })
    }

    fn model_id(&self) -> &'static str {
        "fixture"
    }

    fn prompt_template_version(&self) -> &'static str {
        "fixture@v1"
    }
}
