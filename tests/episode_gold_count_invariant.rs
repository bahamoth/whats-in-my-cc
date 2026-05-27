//! Slice-12 — asserts that the number of golden files in
//! `tests/fixtures/episode_gold/` matches a known constant.
//!
//! Adding or removing a golden file requires updating `EXPECTED_GOLDEN_COUNT`
//! in the same commit. This prevents silent drift between the golden files and
//! the test suite.

/// Number of `*.json` files expected in `tests/fixtures/episode_gold/`.
/// Increment/decrement this in the same commit as the golden file change.
const EXPECTED_GOLDEN_COUNT: usize = 2;

#[test]
fn golden_file_count_matches_constant() {
    let count = std::fs::read_dir("tests/fixtures/episode_gold")
        .expect("cannot read tests/fixtures/episode_gold/")
        .filter_map(|e| e.ok())
        .filter(|e| {
            e.path()
                .extension()
                .and_then(|x| x.to_str())
                .map(|x| x == "json")
                .unwrap_or(false)
        })
        .count();
    assert_eq!(
        count,
        EXPECTED_GOLDEN_COUNT,
        "golden file count changed: found {count}, expected {EXPECTED_GOLDEN_COUNT}. \
         Update EXPECTED_GOLDEN_COUNT in this file in the same commit as adding/removing golden files."
    );
}
