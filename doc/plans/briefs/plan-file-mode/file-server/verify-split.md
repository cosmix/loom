# file-server / verify-split: `loom plan verify` as a function over text

Tier: opus. Wave 0, parallel with `wire-files` (disjoint files). Read
`doc/plans/briefs/plan-file-mode/common.md` first. Spec: `doc/product/section_06.md` "Plan validation".

## Files you own

`loom/src/commands/plan/verify.rs`, `loom/src/commands/plan/verify/{report.rs, human.rs, tests.rs}`
(new), `loom/src/commands/plan/verify/checks.rs`, `loom/src/commands/plan/verify/inputs.rs`,
`loom/src/plan/inputs.rs`, `loom/maintainability-baseline.txt`.

## Today (re-confirm at your base; plan 1 edited `plan/inputs.rs`)

`verify.rs` is 504 lines (ledger rows `file src/commands/plan/verify.rs 504`,
`function ... execute 124`, `function ... print_human 98`). Private JSON types `JsonPlan`:23,
`JsonError`:30 (`pub(super)`, `From<ValidationError>`), `JsonWarnings`:45, `JsonStageLevel`:71,
`JsonOutput`:79; `emit_json`:143; `early_failure`:153 (JSON envelope + `process::exit(1)`);
`execute`:289-414 reads the file with `std::fs::read_to_string`, runs `extract_plan_name`,
`extract_yaml_metadata`, `crate::yaml::from_str::<LoomMetadata>`, `find_repo_root`,
`plan::schema::validate`, `checks::soft_checks` (`verify/checks.rs:14`, `pub(super)`),
`inputs::input_findings(root, plan)` (`verify/inputs.rs`, which calls
`plan::inputs::collect_inputs(root, plan)` at `plan/inputs.rs:89`; `visit_file`:148 reads every
Markdown input from disk, the plan included), `base_tree::check_base_tree(stages, repo_root)`,
then `emit_json` + `process::exit(failed as i32)` or `print_human`. Inline tests :417-504.

## Target

- `verify/report.rs`:
  `pub struct VerifyReport` = today's `JsonOutput`, renamed, `pub`, same `Serialize` output byte for
  byte (field order, names, pretty printing), plus two fields with `#[serde(skip)]`:
  `pub yaml_location: Option<YamlLocation>` and `pub metadata: MetadataPresence`.
  `pub struct YamlLocation { pub line: u64 /* 1-based, in the whole file */, pub column: u64 }`;
  `pub enum MetadataPresence { Present, Missing }`. Rename `JsonError` -> `pub VerifyError { pub stage_id, pub message }`,
  `JsonWarnings` -> `pub VerifyWarnings`, `JsonStageLevel` -> `pub VerifyStageLevel`, `JsonPlan` -> `pub VerifyPlan`; keep serde names.
- `pub fn verify_text(repo_root: Option<&Path>, plan_path: &Path, text: &str) -> VerifyReport`:
  every step of today's body after the read, over `text`; never prints, never exits. A missing
  metadata block gives `metadata: Missing` and the same error today's `early_failure` reports
  (so `--json` stays identical). A YAML parse error fills `yaml_location` from
  `serde_saphyr::Error::location()` (`Location::line()`/`column()`, 1-based, relative to the YAML
  string), offset to file lines with `plan::parser::extraction::extract_yaml_metadata_with_ranges`
  (`:72`, byte ranges: count `\n` before `yaml_fence_range.start`, add the fence line and the
  leading lines `trim()` removed).
- `pub fn verify_path(path: &Path) -> Result<VerifyReport, EarlyFailure>`: today's existence, size
  (`MAX_FILE_BYTES`), UTF-8 checks, then `verify_text`. `pub struct EarlyFailure { pub source: String, pub message: String }`.
- Input checks over proposed text: add `pub(crate) fn collect_inputs_with_text(root: &Path, plan: &Path, plan_text: Option<&str>) -> Result<Inputs>`
  in `plan/inputs.rs`; `collect_inputs(root, plan)` becomes `collect_inputs_with_text(root, plan, None)`;
  `visit_file` uses `plan_text` for the plan's own path instead of reading it. `input_findings`
  gains the same optional text.
- `execute` keeps its signature and behaviour: `verify_path`, then `emit_json` + exit, or
  `print_human` (moved to `verify/human.rs`). `soft_checks` stays where it is; widen visibility
  only as far as `report.rs` needs.
- Move the inline tests to `verify/tests.rs` unchanged (a moved assertion is not an edit).
- `verify.rs` ends under 400 lines and `execute` under 50: delete the `file ... verify.rs 504` and
  `function ... execute 124` / `print_human 98` rows from `loom/maintainability-baseline.txt` (or
  lower them to the new counts if still over the limit). This changes a ratchet file: the stage
  orchestrator files the expected `dispute-integrity` for `TI-ratchet-loom/maintainability-baseline.txt`.

## Tests (in `verify/tests.rs`, beside the moved ones)

`verify_text` over each fixture plan string in `loom/tests/integration/plan_verify.rs`
(`minimal_valid_plan`:12 and the `*_plan()` builders; copy what you need) equals, as
`serde_json::Value`, what `verify_path` gives for the same text written to a temp file; a YAML
error's `yaml_location.line` points at the bad line of the file (build a plan whose fifth YAML
line is broken and assert the absolute line); a plan whose proposed text adds a missing brief
reference reports the input error although the file on disk does not have it.

## Done when

`cargo test --manifest-path loom/Cargo.toml --lib commands::plan::` and
`cargo test --manifest-path loom/Cargo.toml --test integration plan_verify` (28 passed at
b0f91c00) and `--test maintainability` pass.
