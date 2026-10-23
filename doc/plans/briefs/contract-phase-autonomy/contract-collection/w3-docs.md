# W3: plan-writer guidance and README

Tier: haiku (loom-software-engineer, model haiku). Read `common.md` first. Docs only; no code.

## Files you own

`../skills/loom-plan-writer/references/v2-contracts.md`, `../skills/loom-plan-writer/SKILL.md`, `../README.md`.

## Edits

1. `v2-contracts.md` line 68, the `harness` bullet. Replace the sentence listing "a file that exists only to declare the contract test module" and the closing "Prefer a contract location that needs no new declaration ..." sentence with:
   - Harness lists test-only files the contract session may write: fixtures, test helpers, test data. Every existing match is frozen.
   - Module declarations are not harness. For a Rust contract (`cargo-test`, `cargo-nextest`), loom derives the scaffold: the `mod` lines from the crate root to the contract file and a declaration-only module root when the module is new. The writer adds exactly those; they are not frozen, so the implementer extends them. A contract may therefore live in a new module of a binary crate (`src/device_lock/contract_tests.rs` with test `device_lock::contract_tests::<fn>`).
   - At freeze and at completion loom proves each Rust contract's test path resolves through plain `mod` items from a libtest test target to the frozen file; `#[path]`, `cfg_attr`, any `cfg` other than `test` (on a `mod` item or as an inner `#![cfg(..)]` at the top of a chain file), and a target with `test = false`, `harness = false` or `required-features` fail both.
   - A contract file that is itself a test target (a new `tests/<name>.rs`) needs no scaffold.
   - Keep the existing "Never harness a production source file ..." and 500-file sentences.
2. `v2-contracts.md` line 73, sequence step 2: "refuses unless only contract and harness files changed" becomes "refuses unless only contract files, harness files and the derived scaffold edits changed, the scaffold edits add only declarations".
3. `v2-contracts.md` Section 5 table, "Contract check" row: add "a contract's module chain uses `#[path]`, `cfg_attr` or a non-test `cfg` (outer or inner), or no libtest test target compiles its file" to "Fails when".
4. `SKILL.md` line 278 (the `contracts (v2)` row): append "; module declarations are derived as scaffold, never harnessed".
5. `../README.md` line 823 (the `harness` row): add "Module declarations a Rust contract needs are derived by loom (scaffold) and are not frozen." Do not touch other rows.

Re-read each edited paragraph once for agreement with common.md. Use `rg -n` to locate lines first; the numbers above are from the plan's grounding and may have shifted. Run no cargo command. Your one check: `rg -n 'scaffold' ../skills/loom-plan-writer/references/v2-contracts.md ../skills/loom-plan-writer/SKILL.md ../README.md` lists each edit.
