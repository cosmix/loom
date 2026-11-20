# wiring-hardening / W2 — definition-site exclusion for v2 wiring

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D11 (the definition-site bullet).
Knowledge: `mistakes/pinned-literals-ledgers-and-wiring.md` "Goal-Backward Wiring Checks Pin a
PATTERN to a PATH" and "Regex Wiring Checks Are a Permanent Verification Layer". Code:
`verify/goal_backward/wiring_v2.rs` (written by schema-v2: the v2 glob/literal path of
`verify_wiring`); `context/extract/mod.rs::extract_file`.

This is the `CountryLensTable` escape from `doc/verification-report.md`: a wiring pattern
matched the component's own file, so a table that was never mounted passed.

## Files you own

`loom/src/verify/goal_backward/wiring_v2.rs`,
`loom/src/verify/goal_backward/definition_sites.rs` (new),
`loom/src/verify/goal_backward/definition_sites_tests.rs` (new).

## Tasks

1. `definition_sites.rs`: `pub(super) fn definition_lines(path: &Path, bytes: &[u8]) -> Option<Vec<(usize, String)>>`
   returns (line, node name) for every node the extractor finds in the file, or `None` when the
   file's language has no extractor. Use single-file extraction; there is no need for the
   worktree graph here.
2. In `wiring_v2.rs`: after a file matches, collect the matched lines
   (`regex.find_iter` → line numbers). A match counts only when it is not on a definition line
   whose node name occurs in the matched text. When every match across every matched file is
   excluded ⇒ the D11 gap text. Files with `None` count as today and add a note to the gap
   evidence.
3. v1 wiring does not reach `wiring_v2.rs` at all (schema-v2 routes by `plan_version`). Keep it
   that way.

## Named tests (binding), in `definition_sites_tests.rs`

- `wiring_match_only_at_definition_is_a_gap`: Rust file `src/table.rs` with
  `pub fn country_lens_table() {}`; check `source: "src/**/*.rs"`, `pattern: "country_lens_table"`
  → gap `pattern matches only the definition of country_lens_table`.
- `wiring_match_at_consumer_passes`: add `src/app.rs` calling `country_lens_table()` → no gap.
- `v1_wiring_ignores_definition_sites`: the first case through `verify_wiring(.., plan_version = 1)`
  with `source: "src/table.rs"` → no gap, as today.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib verify::goal_backward::definition_sites`

## Report

Files changed; `wiring_v2.rs` line count; the proof result.
