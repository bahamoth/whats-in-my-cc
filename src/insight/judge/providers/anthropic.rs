//! `AnthropicJudge` — real Anthropic API call using hand-rolled reqwest (slice-15).
//!
//! Per DEV-S15-05: we hand-roll the Anthropic client over reqwest rather than
//! depending on a Rust SDK crate. The surface we need is small (one
//! structured-output call shape).
//!
//! Uses prompt caching (cache_control: ephemeral) on the stable system prompt +
//! schema block to minimise token cost per call.
//!
//! Model is pinned to `claude-sonnet-4-6`. Changing the model requires bumping
//! MODEL constant and updating the PROMPT_TEMPLATE_VERSION.

use crate::insight::judge::{JudgeError, JudgePrompt, JudgeProvider, JudgeVerdict};

/// Anthropic Messages API endpoint.
const ANTHROPIC_API_URL: &str = "https://api.anthropic.com/v1/messages";

/// System prompt loaded from the embedded template file.
const SYSTEM_TEMPLATE: &str = include_str!("../prompts/judge_v1.txt");

/// Compute a stable version string: "judge@v1#" + first 12 hex chars of SHA-256 of the template.
/// Per DEV-S15-04: any edit to judge_v1.txt automatically bumps this,
/// invalidating stale cache entries without manual intervention.
fn prompt_template_version_str() -> &'static str {
    use sha2::{Digest, Sha256};
    static VERSION: std::sync::OnceLock<String> = std::sync::OnceLock::new();
    VERSION.get_or_init(|| {
        let mut h = Sha256::new();
        h.update(SYSTEM_TEMPLATE.as_bytes());
        let hex = hex::encode(h.finalize());
        format!("judge@v1#{}", &hex[..12])
    })
}

pub struct AnthropicJudge {
    client: reqwest::Client,
    api_key: String,
    base_url: String,
    model: &'static str,
}

impl AnthropicJudge {
    pub const MODEL: &'static str = "claude-sonnet-4-6";

    /// Construct from the `ANTHROPIC_API_KEY` environment variable.
    /// Returns an error if the variable is not set.
    pub fn from_env() -> anyhow::Result<Self> {
        let key = std::env::var("ANTHROPIC_API_KEY")
            .map_err(|_| anyhow::anyhow!("ANTHROPIC_API_KEY not set"))?;
        Ok(Self {
            client: reqwest::Client::new(),
            api_key: key,
            base_url: ANTHROPIC_API_URL.to_string(),
            model: Self::MODEL,
        })
    }

    /// Construct with an explicit API key and base URL (for testing with mock servers).
    pub fn with_key_and_base_url(
        api_key: impl Into<String>,
        base_url: impl Into<String>,
        client: reqwest::Client,
    ) -> Self {
        Self {
            client,
            api_key: api_key.into(),
            base_url: base_url.into(),
            model: Self::MODEL,
        }
    }
}

#[async_trait::async_trait]
impl JudgeProvider for AnthropicJudge {
    async fn judge(&self, p: JudgePrompt) -> Result<JudgeVerdict, JudgeError> {
        let system_content = serde_json::json!([{
            "type": "text",
            "text": SYSTEM_TEMPLATE,
            "cache_control": { "type": "ephemeral" }
        }]);

        let user_content = serde_json::to_string(&p.evidence_projection)
            .unwrap_or_else(|_| "{}".to_string());

        let body = serde_json::json!({
            "model": self.model,
            "max_tokens": 512,
            "system": system_content,
            "messages": [
                { "role": "user", "content": user_content }
            ]
        });

        let resp = self
            .client
            .post(&self.base_url)
            .header("x-api-key", &self.api_key)
            .header("anthropic-version", "2023-06-01")
            .header("anthropic-beta", "prompt-caching-2024-07-31")
            .header("content-type", "application/json")
            .json(&body)
            .send()
            .await
            .map_err(|e| JudgeError::Transport(e.to_string()))?;

        if !resp.status().is_success() {
            let status = resp.status();
            let text = resp.text().await.unwrap_or_default();
            return Err(JudgeError::Transport(format!(
                "Anthropic API returned {status}: {text}"
            )));
        }

        let raw: serde_json::Value = resp
            .json()
            .await
            .map_err(|e| JudgeError::Schema(e.to_string()))?;

        // Extract text from content[0].text
        let text = raw
            .get("content")
            .and_then(|c| c.as_array())
            .and_then(|a| a.first())
            .and_then(|b| b.get("text"))
            .and_then(|t| t.as_str())
            .ok_or_else(|| JudgeError::Schema("unexpected response shape".to_string()))?;

        serde_json::from_str::<JudgeVerdict>(text)
            .map_err(|e| JudgeError::Schema(format!("verdict parse error: {e}")))
    }

    fn model_id(&self) -> &'static str {
        Self::MODEL
    }

    fn prompt_template_version(&self) -> &'static str {
        prompt_template_version_str()
    }
}
