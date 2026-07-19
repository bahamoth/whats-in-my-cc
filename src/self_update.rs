//! `wimcc self-update` — axoupdater 통합(스펙 2026-07-17 §3).
//! 불변: 어떤 경로에서도 실행 중 serve를 자동 재시작하지 않는다 —
//! serve 재기동은 라이브 CC 세션 관측을 중단시킬 수 있다.

use axoupdater::{AxoUpdater, ReleaseSource, ReleaseSourceType};

#[derive(Debug, PartialEq, Eq)]
pub enum Plan {
    /// shell installer 설치본(receipt 일치) — 실제 교체 수행.
    RunUpdate,
    /// 패키지 매니저 설치본 — 매니저 파일을 임의 교체하면 매니저 상태가 깨지므로 안내만.
    ManagedElsewhere,
}

pub fn decide(receipt_loaded: bool, receipt_is_for_this_exe: bool) -> Plan {
    if receipt_loaded && receipt_is_for_this_exe {
        Plan::RunUpdate
    } else {
        Plan::ManagedElsewhere
    }
}

/// serve auto-update용 채널 판별 — health `version.install_channel`의 값.
/// "shell" = shell installer 설치본(자동 교체 대상), "managed" = 패키지
/// 매니저/dev 빌드(안내만).
pub fn detect_channel() -> &'static str {
    let mut updater = AxoUpdater::new_for("wimcc");
    let receipt_loaded = updater.load_receipt().is_ok();
    let receipt_matches = receipt_loaded
        && updater
            .check_receipt_is_for_this_executable()
            .unwrap_or(false);
    match decide(receipt_loaded, receipt_matches) {
        Plan::RunUpdate => "shell",
        Plan::ManagedElsewhere => "managed",
    }
}

/// 백그라운드 다운로드+교체(shell 채널 전용 — 호출부가 `should_download`로
/// 게이트). 교체가 일어나면 새 버전 태그를 돌려준다. 실행 중인 프로세스는
/// 구 바이너리(inode)로 계속 돈다.
pub async fn download_swap() -> anyhow::Result<Option<String>> {
    let mut updater = AxoUpdater::new_for("wimcc");
    updater.load_receipt()?;
    Ok(updater.run().await?.map(|r| r.new_version_tag))
}

pub async fn run(check_only: bool) -> anyhow::Result<()> {
    let mut updater = AxoUpdater::new_for("wimcc");
    let receipt_loaded = updater.load_receipt().is_ok();
    let receipt_matches = receipt_loaded
        && updater
            .check_receipt_is_for_this_executable()
            .unwrap_or(false);

    if !receipt_loaded {
        // receipt가 없으면 source가 비어 --check도 못 하므로 GitHub를 직접 지정
        updater.set_release_source(ReleaseSource {
            release_type: ReleaseSourceType::GitHub,
            owner: "bahamoth".to_owned(),
            name: "whats-in-my-cc".to_owned(),
            app_name: "wimcc".to_owned(),
        });
        updater.set_current_version(env!("CARGO_PKG_VERSION").parse()?)?;
    }

    if check_only {
        // `AxoUpdater::is_update_needed` internally calls
        // `check_receipt_is_for_this_executable`, which requires `install_prefix`
        // to be set — but that field is only populated by `load_receipt`. A
        // no-receipt build (dev build, or a brew/cargo install) leaves it
        // `None`, so `is_update_needed` errors with `NotConfigured` instead of
        // answering the question. `--check` only wants "is there a newer
        // release", independent of update *eligibility*, so query the release
        // directly and compare versions ourselves.
        let current: axoupdater::Version = env!("CARGO_PKG_VERSION").parse()?;
        // A query failure (e.g. no GitHub release has shipped an installer
        // asset yet) is not fatal for `--check` — report it and exit 0 rather
        // than crashing the whole command over a status query.
        match updater.query_new_version().await {
            Ok(Some(v)) if *v > current => {
                println!("update available — current v{current} -> latest v{v}");
            }
            Ok(_) => {
                println!("up to date — v{current}");
            }
            Err(e) => {
                println!("update check failed ({e}) — current v{current}");
            }
        }
        return Ok(());
    }

    match decide(receipt_loaded, receipt_matches) {
        Plan::ManagedElsewhere => {
            println!(
                "this wimcc appears to be a package-manager install; update with that manager:"
            );
            println!("  brew upgrade wimcc | cargo install wimcc");
            Ok(())
        }
        Plan::RunUpdate => {
            match updater.run().await? {
                Some(result) => {
                    println!("updated to {}", result.new_version_tag);
                    println!("a running serve keeps using the old binary.");
                    println!("restart when no live Claude Code session is being observed: wimcc service restart (or restart manually)");
                }
                None => println!("already up to date — v{}", env!("CARGO_PKG_VERSION")),
            }
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// 스펙 §3 분기 계약: receipt가 있고 이 실행 파일의 것일 때만 실제 업데이트.
    #[test]
    fn shell_install_runs_update() {
        assert_eq!(decide(true, true), Plan::RunUpdate);
    }

    /// receipt 없음 = brew/cargo 설치본 — 매니저 안내만.
    #[test]
    fn package_manager_install_is_guided() {
        assert_eq!(decide(false, false), Plan::ManagedElsewhere);
    }

    /// receipt는 있으나 다른 사본의 것(shell 설치본 receipt + brew 실행 파일).
    #[test]
    fn foreign_receipt_is_guided() {
        assert_eq!(decide(true, false), Plan::ManagedElsewhere);
    }
}
