use sha2::{Digest, Sha256};
use ulid::Generator;

pub fn derive_node_id(kind: &str, keys: &[(&str, &str)]) -> String {
    let mut sorted: Vec<(&str, &str)> = keys.iter().copied().collect();
    sorted.sort_by(|a, b| a.0.cmp(b.0));
    let canonical = sorted.iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(";");
    let mut h = Sha256::new();
    h.update(kind.as_bytes());
    h.update(b"|");
    h.update(canonical.as_bytes());
    format!("nd_{}", hex::encode(&h.finalize()[..12]))
}

pub fn derive_edge_id(from_id: &str, to_id: &str, kind: &str) -> String {
    let mut h = Sha256::new();
    h.update(from_id.as_bytes());
    h.update(b">");
    h.update(to_id.as_bytes());
    h.update(b"#");
    h.update(kind.as_bytes());
    format!("eg_{}", hex::encode(&h.finalize()[..12]))
}

/// Single-task monotonic ULID generator. Must NOT be shared across tasks
/// without external synchronization — slice-1 keeps ingest single-task.
pub struct MonotonicUlidGen { inner: Generator }

impl MonotonicUlidGen {
    pub fn new() -> Self { Self { inner: Generator::new() } }
    pub fn next(&mut self) -> String {
        // unwrap acceptable: only fails on monotonic overflow within same ms (extremely unlikely).
        self.inner.generate().expect("ulid generator overflow").to_string()
    }
}

impl Default for MonotonicUlidGen { fn default() -> Self { Self::new() } }

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn derive_node_id_is_deterministic() {
        let a = derive_node_id("tool_call", &[("session_id","s1"),("tool_use_id","toolu_abc")]);
        let b = derive_node_id("tool_call", &[("tool_use_id","toolu_abc"),("session_id","s1")]);
        assert_eq!(a, b, "must be order-independent (sorted internally)");
        assert!(a.starts_with("nd_"));
        assert_eq!(a.len(), 3 + 24);
    }

    #[test]
    fn derive_node_id_differs_when_kind_differs() {
        let a = derive_node_id("tool_call",   &[("session_id","s1"),("tool_use_id","toolu_abc")]);
        let b = derive_node_id("tool_result", &[("session_id","s1"),("tool_use_id","toolu_abc")]);
        assert_ne!(a, b);
    }

    #[test]
    fn derive_edge_id_directional() {
        let n1 = derive_node_id("tool_call",   &[("session_id","s1"),("tool_use_id","t1")]);
        let n2 = derive_node_id("tool_result", &[("session_id","s1"),("tool_use_id","t1")]);
        let fwd = derive_edge_id(&n1, &n2, "tool_call_to_result");
        let back = derive_edge_id(&n2, &n1, "tool_call_to_result");
        assert_ne!(fwd, back);
        assert!(fwd.starts_with("eg_"));
    }

    #[test]
    fn event_ids_are_monotonic_in_one_thread() {
        let mut gen = MonotonicUlidGen::new();
        let a = gen.next();
        let b = gen.next();
        assert!(b > a, "ulid must be monotonic");
    }
}
