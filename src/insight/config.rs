//! Detector configuration (rule pack). Parameters only — predicate logic stays
//! in code (versioned, like redaction rule_pack). TOML format; missing
//! file/section/key falls back to per-detector code defaults.
//!
//! File location follows the token convention: `$WIMCC_CONFIG_DIR/detectors.toml`,
//! else `~/.config/wimcc/detectors.toml`. Loaded fresh on each `run_detectors`
//! pass (one small file read per ingest batch) so tuning needs no serve restart.
//! 03 spec: provenance.rule_pack is null for code defaults, the pack's id when
//! a TOML pack is loaded — the top-level `id` key carries that id.
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct DetectorConfig {
    /// detector_id → params table
    sections: HashMap<String, toml::Table>,
    /// Top-level `id` of the loaded TOML pack — stamped into
    /// `provenance.rule_pack` (03 spec). `None` for code defaults or an
    /// id-less file (params still apply; the pack is just unnamed).
    pack_id: Option<String>,
}

impl DetectorConfig {
    /// Parse from a TOML string. Shape: `id = ".." [detector.<id>] enabled = .. key = ..`.
    /// An empty/invalid string yields all-defaults.
    pub fn from_toml_str(s: &str) -> Self {
        let root: toml::Table = toml::from_str(s).unwrap_or_default();
        let mut sections = HashMap::new();
        if let Some(toml::Value::Table(dets)) = root.get("detector") {
            for (id, v) in dets {
                if let toml::Value::Table(t) = v {
                    sections.insert(id.clone(), t.clone());
                }
            }
        }
        let pack_id = root
            .get("id")
            .and_then(|v| v.as_str())
            .map(|s| s.to_string());
        Self { sections, pack_id }
    }

    /// Resolve the config file path — same convention as the token file:
    /// `$WIMCC_CONFIG_DIR/detectors.toml`, else `~/.config/wimcc/detectors.toml`.
    fn file_path() -> Option<std::path::PathBuf> {
        if let Ok(v) = std::env::var("WIMCC_CONFIG_DIR") {
            return Some(std::path::PathBuf::from(v).join("detectors.toml"));
        }
        dirs::config_dir().map(|d| d.join("wimcc").join("detectors.toml"))
    }

    /// Load from the conventional file location. Missing file → all-defaults
    /// (the documented contract). An unreadable or invalid file also degrades
    /// to defaults, with a warn — config must never break ingest.
    pub fn load() -> Self {
        let Some(path) = Self::file_path() else {
            return Self::default();
        };
        match std::fs::read_to_string(&path) {
            Ok(s) => {
                let cfg = Self::from_toml_str(&s);
                if cfg.sections.is_empty() && cfg.pack_id.is_none() && !s.trim().is_empty() {
                    tracing::warn!(path = %path.display(), "detectors.toml parsed to nothing — falling back to code defaults");
                }
                cfg
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(e) => {
                tracing::warn!(path = %path.display(), err = %e, "detectors.toml unreadable — falling back to code defaults");
                Self::default()
            }
        }
    }

    /// Top-level pack id of the loaded TOML, if any.
    pub fn pack_id(&self) -> Option<&str> {
        self.pack_id.as_deref()
    }

    /// Enabled unless explicitly `enabled = false`. Missing detector → true.
    pub fn enabled(&self, detector: &str) -> bool {
        self.sections
            .get(detector)
            .and_then(|t| t.get("enabled"))
            .and_then(|v| v.as_bool())
            .unwrap_or(true)
    }

    /// usize param with code-supplied fallback.
    pub fn usize_param(&self, detector: &str, key: &str, fallback: usize) -> usize {
        self.sections
            .get(detector)
            .and_then(|t| t.get(key))
            .and_then(|v| v.as_integer())
            .map(|i| i.max(0) as usize)
            .unwrap_or(fallback)
    }
}
