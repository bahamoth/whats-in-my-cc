//! `wimcc service` — serve를 OS 사용자 서비스로 등록(스펙 2026-07-17 §5).
//! macOS launchd(gui 도메인) + Linux systemd user unit. 실행 주체는 항상 사용자.

use anyhow::{bail, Context};
use std::path::{Path, PathBuf};
use std::process::Command as Proc;

pub const SERVICE_LABEL: &str = "com.bahamoth.wimcc";

/// plist(XML)의 `<string>` 내용에 안전하게 넣기 위한 최소 이스케이프.
/// 순서 무관(각 치환 결과 문자가 나머지 두 패턴을 새로 만들지 않는다: `&`→`&amp;`엔 `<`/`>`가 없다).
fn xml_escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
}

/// argv[0]=실행 파일 절대경로, 이후 전체 인자. 서비스는 CWD 보장이 없으므로
/// 경로 인자는 호출부에서 절대경로로 만들어 전달한다.
pub fn launchd_plist(argv: &[String]) -> String {
    let items: String = argv
        .iter()
        .map(|a| format!("    <string>{}</string>\n", xml_escape(a)))
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
        .with_context(|| format!("failed to run {program}"))?;
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
    // R-2026-07-18: serve(main.rs)는 is_loopback()이 아니면 즉시 Err로 기동을
    // 거부한다. 그 기준을 여기서 미러링하지 않으면 non-loopback bind로 등록된
    // 서비스가 plist KeepAlive=true / unit Restart=on-failure에 의해 무한
    // 재시작(crash-loop)된다 — install 시점엔 "등록 완료"만 보이고 실패는
    // 로그에서만 드러난다. 파일 쓰기·launchctl/systemctl 호출보다 반드시 먼저
    // 검증해 실패 시 아무 부수효과도 남기지 않는다.
    let ip: std::net::IpAddr = bind
        .parse()
        .with_context(|| format!("failed to parse bind address: {bind}"))?;
    if !ip.is_loopback() {
        bail!(
            "serve only accepts loopback binds (got {bind}) — a non-loopback service would \
             crash-loop on start (KeepAlive/Restart), so registration is aborted"
        );
    }
    let argv = build_argv(db_path, bind, port, auto_migrate)?;
    if cfg!(target_os = "macos") {
        let path = plist_path()?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, launchd_plist(&argv))?;
        let uid = current_uid()?;
        let ok = run_cmd(
            "launchctl",
            &["bootstrap", &format!("gui/{uid}"), &path.to_string_lossy()],
        )?;
        if !ok {
            bail!(
                "launchctl bootstrap failed — plist was written: {}. Check `launchctl print gui/{uid}/{SERVICE_LABEL}` or the launchctl output (it may already be registered)",
                path.display()
            );
        }
        println!("registered: {}", path.display());
    } else if cfg!(target_os = "linux") {
        let path = unit_path()?;
        std::fs::create_dir_all(path.parent().unwrap())?;
        std::fs::write(&path, systemd_unit(&argv))?;
        let reload_ok = run_cmd("systemctl", &["--user", "daemon-reload"])?;
        let enable_ok = run_cmd("systemctl", &["--user", "enable", "--now", "wimcc"])?;
        if !reload_ok || !enable_ok {
            bail!(
                "systemctl registration failed — unit was written: {}. Check `systemctl --user status wimcc` or the systemctl output",
                path.display()
            );
        }
        println!("registered: {}", path.display());
    } else {
        bail!("unsupported OS — macOS (launchd) and Linux (systemd) only");
    }
    println!("serve will start on login. Remove with: wimcc service uninstall");
    Ok(())
}

pub fn uninstall() -> anyhow::Result<()> {
    if cfg!(target_os = "macos") {
        let uid = current_uid()?;
        let ok = run_cmd(
            "launchctl",
            &["bootout", &format!("gui/{uid}/{SERVICE_LABEL}")],
        )?;
        if !ok {
            println!("unregister command failed (it may not have been registered)");
        }
        let path = plist_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
    } else if cfg!(target_os = "linux") {
        let ok = run_cmd("systemctl", &["--user", "disable", "--now", "wimcc"])?;
        if !ok {
            println!("unregister command failed (it may not have been registered)");
        }
        let path = unit_path()?;
        if path.exists() {
            std::fs::remove_file(&path)?;
        }
        run_cmd("systemctl", &["--user", "daemon-reload"])?;
    } else {
        bail!("unsupported OS");
    }
    println!("unregistered");
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
        bail!("unsupported OS")
    };
    println!(
        "{}",
        if ok {
            "restarted"
        } else {
            "restart failed — check: wimcc service status"
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
        bail!("unsupported OS")
    };
    println!(
        "{}",
        if ok {
            "registered/running"
        } else {
            "not registered or stopped"
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
    fn launchd_plist_escapes_xml_special_chars() {
        let argv = vec![
            "/usr/local/bin/wimcc".to_string(),
            "--label".to_string(),
            "A&B<C>D".to_string(),
        ];
        let out = launchd_plist(&argv);
        assert!(
            out.contains("<string>A&amp;B&lt;C&gt;D</string>"),
            "expected escaped form in output, got:\n{out}"
        );
        assert!(
            !out.contains("A&B<C>D"),
            "raw unescaped argv leaked into plist output:\n{out}"
        );
    }

    #[test]
    fn systemd_unit_snapshot() {
        insta::assert_snapshot!(systemd_unit(&argv()));
    }

    /// R-2026-07-18: `wimcc service install --bind 0.0.0.0`이 그대로 등록되면
    /// serve의 loopback-only 강제(main.rs)에 걸려 기동이 항상 실패하고,
    /// plist `KeepAlive=true`/unit `Restart=on-failure`가 무한 재시작(crash-loop)한다.
    /// install()은 파일을 쓰거나 launchctl/systemctl을 부르기 전에 non-loopback bind를
    /// 거부해야 한다. 이 테스트는 실 서비스 파일 경로(홈 디렉터리 하위, 고정 위치)를
    /// 건드리지 않기 위해 "쓰기 전/후 상태 불변"을 확인한다 — 실제 write가 있었다면
    /// mtime/존재 여부가 바뀌었을 것이다.
    #[test]
    fn install_rejects_non_loopback_bind_before_any_file_write() {
        let target_path = if cfg!(target_os = "macos") {
            plist_path().expect("plist path")
        } else if cfg!(target_os = "linux") {
            unit_path().expect("unit path")
        } else {
            // 지원하지 않는 OS — install()은 이후 분기에서 bail하므로 여기서는
            // loopback 거부만 확인한다(파일 부재 확인은 생략).
            let result = install(
                std::path::Path::new("/tmp/wimcc-install-reject-test.sqlite"),
                "0.0.0.0",
                7878,
                true,
            );
            assert!(result.is_err(), "non-loopback bind must be rejected");
            return;
        };
        let existed_before = target_path.exists();
        let mtime_before = std::fs::metadata(&target_path)
            .and_then(|m| m.modified())
            .ok();

        let result = install(
            std::path::Path::new("/tmp/wimcc-install-reject-test.sqlite"),
            "0.0.0.0",
            7878,
            true,
        );

        let err = result.expect_err("non-loopback bind must be rejected");
        let msg = err.to_string();
        assert!(
            msg.contains("loopback"),
            "error should mention loopback, got: {msg}"
        );

        let existed_after = target_path.exists();
        assert_eq!(
            existed_before, existed_after,
            "install must not create/remove the service file for a rejected bind"
        );
        if let Some(before) = mtime_before {
            let after = std::fs::metadata(&target_path)
                .expect("file should still exist")
                .modified()
                .expect("mtime");
            assert_eq!(
                before, after,
                "existing service file must not be touched on a rejected bind"
            );
        }
    }

    #[test]
    fn install_rejects_unparseable_bind() {
        let result = install(
            std::path::Path::new("/tmp/wimcc-install-reject-test2.sqlite"),
            "not-an-ip",
            7878,
            true,
        );
        assert!(result.is_err(), "unparseable bind must be rejected");
    }
}
