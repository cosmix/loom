# schema-v2 / W4 — `plan verify` lints

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D4 (every row except the three
marked stage `contract-phase`). Knowledge: `architecture/plan-lifecycle-and-fields.md` "What
`loom plan verify` Rejects Before a Stage Runs (2026-09-19)";
`mistakes/sandbox-tooling-and-network.md` "The Sandbox's AF_UNIX Denial Also Kills sccache,
Breaking Every Cargo Command (2026-09-04)".

Wave 2: W1's types exist. W3 declares your module (`pub(crate) mod v2_lints;` in
`validation.rs`) and your test file (`mod v2_lint_tests;` in `plan/schema/tests/mod.rs`), and
calls your `run`.

## Files you own

`loom/src/plan/schema/validation/v2_lints/{mod,loom_subcommands,regex_patterns,sandbox_capability,knowledge_check,rust_filters}.rs`
(all new), `loom/src/plan/schema/tests/v2_lint_tests.rs` (new).

## Interface (pinned, DESIGN D4)

```rust
pub(crate) struct LintContext<'a> { pub metadata: &'a LoomMetadata, pub repo_root: Option<&'a Path> }
pub(crate) struct LintFinding { pub stage_id: Option<String>, pub message: String, pub error_in_v2: bool }
pub(crate) fn run(ctx: &LintContext<'_>) -> Vec<LintFinding>;
```

`run` calls one function per module; each module exposes
`pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>)`. Commands come from every
field listed in D4; lex them with `super::shell_lex` (`simple_commands`), never by splitting
strings. Reuse `criterion_hazards` detection where D4 says "existing set"
(`criterion_hazards.rs:149-165` `criterion_needs_ungrantable_resource`, `:277-289` network
binaries); do not duplicate its tables. If a helper there is private, make it `pub(super)`.

## Module notes

- `loom_subcommands.rs`: walk `crate::cli::Cli::command()` (`clap::CommandFactory`, already used
  in `completions/dynamic/commands.rs`). Consume leading words that are not flags; for each word,
  `find_subcommand`; stop at the first flag or at a subcommand with no subcommands. An unknown
  word where the current command has subcommands and takes no positional arguments is a finding.
  argv[0] matches `loom` or any path ending in `/loom`.
- `regex_patterns.rs`: the four regex rows. Compile wiring patterns with the same builder as
  `verify/goal_backward/wiring.rs:59-62` (`RegexBuilder::new(p).size_limit(1 << 20)`). Skip
  wiring entries with `literal: true`. For `rg`/`grep`, the pattern is the first non-flag
  argument unless `-e`/`--regexp` supplies it; with `-F`/`--fixed-strings` no compile check.
- `sandbox_capability.rs`: network-without-domain (effective domains: the stage's sandbox
  override, else the plan's), ungrantable resources, the rustc-wrapper warning
  (`orchestrator/terminal/native/build_cache.rs::find_sccache_path()` and `LOOM_SCCACHE`).
- `knowledge_check.rs`: for `knowledge`/`knowledge-distill` stages, a criterion running
  `loom knowledge check` with `--strict` and without `--baseline`, while the repository's
  knowledge tree has structural issues now. Call the check's in-process function read-only
  (find it under `commands/knowledge/` or `fs/knowledge/`); no writes. No `repo_root` ⇒ skip.
- `rust_filters.rs` (G5): D4 row "Rust filter matches nothing". Load the base layer for HEAD
  with `context::graph_store::GraphStore::load_base` (never `ensure_snapshot`, never a write).
  Match the filter's module path against node paths the way `loom map --find-all` resolves
  names (`map/views/mod.rs:200-223` `find_symbol_matches`). Match against the stage's `files:`
  and `artifacts:` by the D4 path rule. Warning only.

## Named tests (binding), in `plan/schema/tests/v2_lint_tests.rs`

- `unknown_loom_subcommand_is_error_in_v2`: v2 plan whose knowledge-distill acceptance runs
  `loom knowledge verify` → a finding with `error_in_v2 == true`, mapped to an error by the v2
  path.
- `dash_leading_rg_pattern_is_flagged`: `rg -qF "--out" src/x.rs` → finding.
- `rust_filter_matching_no_module_warns`: a `cargo test --lib nonexistent_mod::` criterion in a
  temp repository with a base layer written for its HEAD → warning finding. Build the base layer
  in the test through the public graph store API.
- `knowledge_strict_check_without_baseline_is_flagged`: temp repository whose knowledge tree has
  a structural issue → finding.
- `network_binary_without_domains_is_error_in_v2`: `curl https://x` with empty
  `allowed_domains` → `error_in_v2 == true`.

Every lint also gets a negative test (the clean form produces no finding) in the same file.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib plan::schema::tests::v2_lint_tests`

## Report

Files created; any `criterion_hazards` item you made `pub(super)`; the proof result.
