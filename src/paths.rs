use std::path::PathBuf;

pub fn default_transcripts_root() -> Option<PathBuf> {
    dirs::home_dir().map(|h| h.join(".claude").join("projects"))
}
