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
    /// serve 기동 시 1회 판별 — "shell"(self-update 대상) | "managed"(패키지
    /// 매니저 소유, 안내만). None = 미판별(구버전 클라이언트 호환).
    pub install_channel: Option<&'static str>,
    /// auto-update가 디스크에 교체해 둔 버전 태그 — 다음 재시작 때 적용된다.
    pub downloaded: Option<String>,
}

pub type SharedUpdateStatus = Arc<RwLock<UpdateStatus>>;

/// auto-update 다운로드 결정 — 2026-07-19 사용자 결정(다운로드까지 자동,
/// 재시작은 수동). shell 채널 + opt-in + 신규 버전 + 미다운로드일 때만 참.
pub fn should_download(
    auto_update: bool,
    channel: Option<&str>,
    current: &str,
    latest: &str,
    already_downloaded: Option<&str>,
) -> bool {
    auto_update
        && channel == Some("shell")
        && is_newer(current, latest)
        && already_downloaded != Some(latest)
}

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
///
/// `auto_update`가 참이면(2026-07-19 사용자 결정) 새 릴리스 관측 시 shell
/// 채널에 한해 바이너리를 백그라운드에서 미리 교체한다 — 실행 중인 serve는
/// 구 바이너리로 계속 돌고, 다음 재시작 때 적용된다(라이브 세션 관측을 끊는
/// 자동 재시작은 하지 않는다).
pub async fn run_update_check_loop(
    status: SharedUpdateStatus,
    url: String,
    auto_update: bool,
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
                    let (channel, downloaded) = {
                        let mut s = status.write().await;
                        s.latest = Some(tag.clone());
                        s.update_available = newer;
                        (s.install_channel, s.downloaded.clone())
                    };
                    if should_download(
                        auto_update,
                        channel,
                        env!("CARGO_PKG_VERSION"),
                        &tag,
                        downloaded.as_deref(),
                    ) {
                        match crate::self_update::download_swap().await {
                            Ok(Some(swapped)) => {
                                tracing::info!(version = %swapped, "auto-update: binary swapped; applies on next restart");
                                status.write().await.downloaded = Some(swapped);
                            }
                            Ok(None) => {}
                            Err(e) => {
                                tracing::warn!(error = ?e, "auto-update download failed; will retry next cycle");
                            }
                        }
                    }
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

    /// 다운로드는 네 조건의 교집합에서만 — opt-in·shell 채널·신규 버전·
    /// 미다운로드. 하나라도 빠지면 부수효과(바이너리 교체) 금지.
    #[test]
    fn download_requires_optin_shell_newer_and_not_yet_downloaded() {
        assert!(should_download(true, Some("shell"), "1.4.0", "v1.5.0", None));
        // opt-out
        assert!(!should_download(false, Some("shell"), "1.4.0", "v1.5.0", None));
        // managed 채널(brew/cargo)은 바이너리 소유권이 매니저에 있다
        assert!(!should_download(true, Some("managed"), "1.4.0", "v1.5.0", None));
        assert!(!should_download(true, None, "1.4.0", "v1.5.0", None));
        // 신규 아님
        assert!(!should_download(true, Some("shell"), "1.5.0", "v1.5.0", None));
        // 같은 태그 재다운로드 금지(24h 주기마다 반복 방지)
        assert!(!should_download(
            true,
            Some("shell"),
            "1.4.0",
            "v1.5.0",
            Some("v1.5.0")
        ));
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
