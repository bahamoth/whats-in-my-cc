//! Detector configuration (rule pack). Parameters only — predicate logic stays
//! in code (versioned, like redaction rule_pack). TOML format; missing
//! file/section/key falls back to per-detector code defaults (spec §10.4).
use std::collections::HashMap;

#[derive(Debug, Default, Clone)]
pub struct DetectorConfig {
    /// detector_id → params table
    sections: HashMap<String, toml::Table>,
}

impl DetectorConfig {
    /// Parse from a TOML string. Shape: `[detector.<id>] enabled = .. key = ..`.
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
        Self { sections }
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
