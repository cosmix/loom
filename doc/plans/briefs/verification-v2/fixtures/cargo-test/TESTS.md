# cargo-test fixture

Source project: scratch `runner-projects/cargo-test/` (crate `fixture_cargo`).

- `src/lib.rs` — `#[cfg(test)] mod tests` with the three required tests, full paths:
  - `tests::alpha_passes` (passes)
  - `tests::beta_fails` (fails: `assert_eq!(add(2, 2), 5, ...)`)
  - `tests::gamma_passes` (passes)
- `tests/integration.rs` — a second test binary with one unrelated test
  (`smoke_integration`), present only so the crate has two test binaries as
  required by the brief. Not targeted by any filter scenario.

Filter command form: `cargo test <full::path::name> -- --exact`

No-color form: `cargo test <full::path::name> --color=never -- --exact --color=never`
(cargo's own coloring and libtest's coloring are separate flags).

Notable: `no-match` (filter `tests::delta_missing`, which does not exist)
exits **0** — cargo test treats "ran 0 matching tests" as success.
