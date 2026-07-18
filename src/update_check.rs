//! 새 버전 확인 — GitHub Releases 메타데이터 조회.
//! wimcc의 유일한 outbound 호출이다(스펙 §4). 실패는 조용히 무시하고 다음 주기에 재시도.

use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::RwLock;
use tokio_util::sync::CancellationToken;

/// 기본 조회처. 테스트·스모크는 `WIMCC_UPDATE_CHECK_URL`로 대체한다(코드 주석만, 문서화하지 않는 테스트용 노브).
pub const DEFAULT_LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/bahamoth/whats-in-my-cc/releases/latest";

/// 미조회/실패 = `latest: None` — 미측정을 0/거짓 양성으로 뭉개지 않는다(표기 원칙).
#[derive(Debug, Clone, Default)]
pub struct UpdateStatus {
    pub latest: Option<String>,
    pub update_available: bool,
}

pub type SharedUpdateStatus = Arc<RwLock<UpdateStatus>>;

#[derive(Debug, Deserialize)]
struct LatestRelease {
    tag_name: String,
}

pub fn parse_latest_tag(body: &str) -> Option<String> {
    serde_json::from_str::<LatestRelease>(body)
        .ok()
        .map(|r| r.tag_name)
}

pub fn is_newer(current: &str, latest_tag: &str) -> bool {
    let cur = semver::Version::parse(current.trim_start_matches('v'));
    let lat = semver::Version::parse(latest_tag.trim_start_matches('v'));
    match (cur, lat) {
        (Ok(c), Ok(l)) => l > c,
        _ => false,
    }
}

async fn fetch_latest(client: &reqwest::Client, url: &str) -> Option<String> {
    let resp = client
        .get(url)
        .header("user-agent", concat!("wimcc/", env!("CARGO_PKG_VERSION")))
        .header("accept", "application/vnd.github+json")
        .send()
        .await
        .ok()?;
    if !resp.status().is_success() {
        return None;
    }
    parse_latest_tag(&resp.text().await.ok()?)
}

/// serve 기동 시 spawn. tokio interval의 첫 tick은 즉시 발화하므로
/// "시작 시 + 24h 주기" 계약(스펙 §4)을 이 루프 하나로 충족한다.
pub async fn run_update_check_loop(
    status: SharedUpdateStatus,
    url: String,
    shutdown: CancellationToken,
) {
    // 행 걸린 응답이 다음 tick까지 루프 전체를 막지 않도록 요청에 상한을 둔다.
    // 계약 유지: build 실패는 "실패는 조용히 무시" 원칙에 따라 기본 Client로
    // 폴백한다(타임아웃 없이 이전 동작과 동일 — outbound 자체를 막지 않는다).
    let client = reqwest::Client::builder()
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .unwrap_or_default();
    let mut interval = tokio::time::interval(std::time::Duration::from_secs(60 * 60 * 24));
    loop {
        tokio::select! {
            _ = shutdown.cancelled() => break,
            _ = interval.tick() => {
                if let Some(tag) = fetch_latest(&client, &url).await {
                    let newer = is_newer(env!("CARGO_PKG_VERSION"), &tag);
                    if newer {
                        tracing::info!(current = env!("CARGO_PKG_VERSION"), latest = %tag, "wimcc update available");
                    }
                    let mut s = status.write().await;
                    s.latest = Some(tag);
                    s.update_available = newer;
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn newer_patch_is_detected() {
        assert!(is_newer("1.3.0", "v1.3.1"));
        assert!(is_newer("1.3.0", "v2.0.0"));
    }

    #[test]
    fn same_or_older_is_not_newer() {
        assert!(!is_newer("1.3.0", "v1.3.0"));
        assert!(!is_newer("1.3.0", "v1.2.9"));
    }

    #[test]
    fn garbage_tag_is_not_newer() {
        assert!(!is_newer("1.3.0", "not-a-version"));
    }

    /// real fixture invariant: 태그는 v접두 semver (2026-07-18 채취).
    #[test]
    fn real_fixture_parses_v_prefixed_semver_tag() {
        let body = include_str!("../tests/fixtures/update_check/real/releases_latest.json");
        let tag = parse_latest_tag(body).expect("real payload has tag_name");
        assert!(tag.starts_with('v'), "tag was: {tag}");
        assert!(semver::Version::parse(tag.trim_start_matches('v')).is_ok());
    }
}
