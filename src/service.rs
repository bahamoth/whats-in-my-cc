//! `wimcc service` — serve를 OS 사용자 서비스로 등록(스펙 2026-07-17 §5).
//! macOS launchd(gui 도메인) + Linux systemd user unit. 실행 주체는 항상 사용자.

use anyhow::{bail, Context};
use std::path::{Path, PathBuf};
use std::process::Command as Proc;

pub const SERVICE_LABEL: &str = "com.bahamoth.wimcc";

/// argv[0]=실행 파일 절대경로, 이후 전체 인자. 서비스는 CWD 보장이 없으므로
/// 경로 인자는 호출부에서 절대경로로 만들어 전달한다.
pub fn launchd_plist(argv: &[String]) -> String {
    let items: String = argv
        .iter()
        .map(|a| format!("    <string>{a}</string>\n"))
        .collect();
    format!(
        r#"<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>Label</key>
  <string>{SERVICE_LABEL}</string>
  <key>ProgramArguments</key>
  <array>
{items}  </array>
  <key>RunAtLoad</key>
  <true/>
  <key>KeepAlive</key>
  <true/>
</dict>
</plist>
"#
    )
}

pub fn systemd_unit(argv: &[String]) -> String {
    let exec: String = argv
        .iter()
        .map(|a| format!("\"{a}\""))
        .collect::<Vec<_>>()
        .join(" ");
    format!(
        r#"[Unit]
Description=wimcc serve — local Claude Code observation

[Service]
ExecStart={exec}
Restart=on-failure

[Install]
WantedBy=default.target
"#
    )
}

fn plist_path() -> anyhow::Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("no home dir")?
        .join("Library/LaunchAgents")
        .join(format!("{SERVICE_LABEL}.plist")))
}

fn unit_path() -> anyhow::Result<PathBuf> {
    Ok(dirs::home_dir()
        .context("no home dir")?
        .join(".config/systemd/user/wimcc.service"))
}

fn current_uid() -> anyhow::Result<String> {
    let out = Proc::new("id").arg("-u").output().context("id -u")?;
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

fn run_cmd(program: &str, args: &[&str]) -> anyhow::Result<bool> {
    let status = Proc::new(program)
        .args(args)
        .status()
        .with_context(|| format!("{program} 실행 실패"))?;
    Ok(status.success())
}

fn build_argv(
    db_path: &Path,
    bind: &str,
    port: u16,
    auto_migrate: bool,
) -> anyhow::Result<Vec<String>> {
    let exe = std::env::current_exe().context("current_exe")?;
    // 서비스는 홈 디렉터리 CWD로 돌므로 상대 db 경로는 절대화한다
    let db_abs = if db_path.is_absolute() {
        db_path.to_path_buf()
    } else {
        std::env::current_dir()?.join(db_path)
    };
    let mut argv = vec![
        exe.to_string_lossy().into_owned(),
        "--db-path".into(),
        db_abs.to_string_lossy().into_owned(),
        "serve".into(),
        "--bind".into(),
        bind.into(),
        "--port".into(),
        port.to_string(),
    ];
    if auto_migrate {
        argv.push("--auto-migrate".into());
    }
    Ok(argv)
}

pub fn install(db_path: &Path, bind: &str, port: u16, auto_migrate: bool) -> anyhow::Result<()> {
    let argv = build_argv(db_path, bind, port, auto_migrate)?;
    if cfg!(target_os = "macos") {
        let path = plist_path()?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, launchd_plist(&argv))?;
        let uid = current_uid()?;
        run_cmd(
            "launchctl",
            &["bootstrap", &format!("gui/{uid}"), &path.to_string_lossy()],
        )?;
        println!("등록 완료: {}", path.display());
    } else if cfg!(target_os = "linux") {
        let path = unit_path()?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, systemd_unit(&argv))?;
        run_cmd("systemctl", &["--user", "daemon-reload"])?;
        run_cmd("systemctl", &["--user", "enable", "--now", "wimcc"])?;
        println!("등록 완료: {}", path.display());
    } else {
        bail!("지원하지 않는 OS — macOS(launchd)·Linux(systemd)만");
    }
    println!("로그인 시 serve가 자동 시작됩니다. 해제: wimcc service uninstall");
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    if cfg!(target_os = "macos") {
        let uid = current_uid()?;
        run_cmd(
            "launchctl",
            &["bootout", &format!("gui/{uid}/{SERVICE_LABEL}")],
        )?;
        let path = plist_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    } else if cfg!(target_os = "linux") {
        run_cmd("systemctl", &["--user", "disable", "--now", "wimcc"])?;
        let path = unit_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        run_cmd("systemctl", &["--user", "daemon-reload"])?;
    } else {
        bail!("지원하지 않는 OS");
    }
    println!("해제 완료");
    Ok(())
}

pub fn restart() -> anyhow::Result<()> {
    let ok = if cfg!(target_os = "macos") {
        let uid = current_uid()?;
        run_cmd(
            "launchctl",
            &["kickstart", "-k", &format!("gui/{uid}/{SERVICE_LABEL}")],
        )?
    } else if cfg!(target_os = "linux") {
        run_cmd("systemctl", &["--user", "restart", "wimcc"])?
    } else {
        bail!("지원하지 않는 OS")
    };
    println!(
        "{}",
        if ok {
            "재시작 완료"
        } else {
            "재시작 실패 — status로 확인"
        }
    );
    Ok(())
}

pub fn status() -> anyhow::Result<()> {
    let ok = if cfg!(target_os = "macos") {
        let uid = current_uid()?;
        run_cmd(
            "launchctl",
            &["print", &format!("gui/{uid}/{SERVICE_LABEL}")],
        )?
    } else if cfg!(target_os = "linux") {
        run_cmd("systemctl", &["--user", "is-active", "wimcc"])?
    } else {
        bail!("지원하지 않는 OS")
    };
    println!(
        "{}",
        if ok {
            "등록됨/실행 중"
        } else {
            "미등록 또는 정지"
        }
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn argv() -> Vec<String> {
        [
            "/usr/local/bin/wimcc",
            "--db-path",
            "/data/wimcc.sqlite",
            "serve",
            "--bind",
            "127.0.0.1",
            "--port",
            "7878",
            "--auto-migrate",
        ]
        .map(String::from)
        .to_vec()
    }

    #[test]
    fn launchd_plist_snapshot() {
        insta::assert_snapshot!(launchd_plist(&argv()));
    }

    #[test]
    fn systemd_unit_snapshot() {
        insta::assert_snapshot!(systemd_unit(&argv()));
    }
}
