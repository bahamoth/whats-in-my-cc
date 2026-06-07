use wimcc::model::observed::EventKind;
use strum::IntoEnumIterator;

#[test]
fn diffhunk_variant_removed() {
    // DiffHunk is side-table-only — must not be an EventKind variant (spec §10.3).
    assert!(EventKind::iter().all(|k| k.as_str() != "diff_hunk"));
}
