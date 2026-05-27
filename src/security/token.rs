//! Slice-19 — Bearer token generation, persistence, and rotation.
//!
//! Token file location: `$WITMCC_CONFIG_DIR/token` (POSIX mode 0600).
//! If `WITMCC_CONFIG_DIR` is not set, falls back to `~/.config/witmcc/token`.
//!
//! Token format: `witmcc_<43-char base64url>` (~50 chars total).

use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use base64::Engine as _;

/// Return the path to the token file.
/// Uses `WITMCC_CONFIG_DIR` env var for test overriding; otherwise
/// `~/.config/witmcc/token`.
pub fn token_file_path() -> Result<PathBuf> {
    let dir = if let Ok(v) = std::env::var("WITMCC_CONFIG_DIR") {
        PathBuf::from(v)
    } else {
        dirs::config_dir()
            .map(|d| d.join("witmcc"))
            .ok_or_else(|| anyhow::anyhow!("cannot determine config directory"))?
    };
    Ok(dir.join("token"))
}

/// Generate a new cryptographically random token.
pub fn generate_token() -> String {
    let mut bytes = [0u8; 32];
    rand::Fill::fill(&mut bytes, &mut rand::rng());
    let encoded = base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    format!("witmcc_{encoded}")
}

/// Load the token from the token file, checking permissions on POSIX.
/// Returns an error if the file has overpermissive permissions (mode != 0600).
pub fn load_token_or_err() -> Result<String> {
    let path = token_file_path()?;
    if !path.exists() {
        bail!("token file does not exist at {}", path.display());
    }

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let meta = std::fs::metadata(&path)
            .with_context(|| format!("failed to stat token file {}", path.display()))?;
        let mode = meta.permissions().mode() & 0o777;
        if mode != 0o600 {
            bail!(
                "token file {} has overpermissive mode {:o} (expected 0600). \
                 Fix with: chmod 600 {}",
                path.display(),
                mode,
                path.display()
            );
        }
    }

    let content = std::fs::read_to_string(&path)
        .with_context(|| format!("failed to read token file {}", path.display()))?;
    Ok(content.trim().to_string())
}

/// Ensure a token exists: load it if present, generate + persist if absent.
/// On first call, the token file is created at mode 0600 (POSIX).
pub fn ensure_token() -> Result<String> {
    let path = token_file_path()?;
    if path.exists() {
        return load_token_or_err();
    }
    // Generate and persist.
    let token = generate_token();
    write_token_file(&path, &token)?;
    Ok(token)
}

/// Rotate: generate a new token, atomically overwrite, return new token.
pub fn rotate_token() -> Result<String> {
    let path = token_file_path()?;
    let new_token = generate_token();
    write_token_file(&path, &new_token)?;
    Ok(new_token)
}

/// Write a token to `path` atomically (via tmp-file + rename) at mode 0600.
fn write_token_file(path: &std::path::Path, token: &str) -> Result<()> {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent)
            .with_context(|| format!("failed to create config dir {}", parent.display()))?;
    }
    let tmp = path.with_extension("tmp");
    std::fs::write(&tmp, token)
        .with_context(|| format!("failed to write token tmp file {}", tmp.display()))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600))
            .with_context(|| format!("failed to chmod token tmp file {}", tmp.display()))?;
    }

    std::fs::rename(&tmp, path)
        .with_context(|| format!("failed to rename token file {}", path.display()))?;
    Ok(())
}
