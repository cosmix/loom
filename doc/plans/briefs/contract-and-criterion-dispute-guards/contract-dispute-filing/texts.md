# Brief: texts (stage contract-dispute-filing)

## Goal

Everything a judge, a stage agent or an operator reads about contract disputes describes the new shape, and `loom stage amend` refuses a stage session.

## Files you own

- `loom/src/orchestrator/adjudication/prompt.rs`, `loom/src/orchestrator/adjudication/prompt/**`
- `loom/src/orchestrator/signals/v2_section.rs`, `v2_section_tests.rs`, `helpers.rs`, `tests_doctrine_v2.rs`
- `loom/src/stage_service/control_block.rs`, `control_block_agent_tests.rs`
- `loom/src/commands/stage/amend.rs`, `loom/src/cli/types_stage.rs`
- `loom/tests/fixtures/signal_replay/plan.md`, `loom-hooks/tests/**`
- `README.md`, `skills/**`

Frozen contract files are read-only. `prompt/contract.rs` is 385 lines and ledgered exactly in `loom/maintainability-baseline.txt`: move text into a new file under `prompt/` when it grows, and report every ledger row that must change.

## Model you compile against (the `filing` worker writes it)

```rust
DisputeKind::Contract { contract_ids: Vec<String>, replacements: Vec<FileReplacement> }
FileReplacement { file: String, content: String, base_sha256: Option<String> }
```

Verdict the judge writes: `{"verdict":"contract-rulings","rulings":[{"contract":"<id>","ruling":"accept"|"reject","reasoning":"..","citations":[..]}],"plan_patch":{"field":"contracts","patches":[..]}}`, or `needs-more-evidence` as today.

## Judge prompt (`prompt/contract.rs`, `contract_replacement.rs`, `contract_frozen_set.rs`, `prompt.rs:174`)

- `build_instructions` (`contract.rs:63-110`) covers every disputed contract; drop "ONE disputed frozen contract" and "the changed test". Admission now refuses a filing over an edited frozen file, so a change reaches the judge only as a replacement or as a requested spec patch.
- `first_step` (`:112-146`) and `build_evidence` (`:260`) loop over the contracts: spec, frozen content and run command for each; then one "## Proposed replacement" section and diff per replaced file (`contract_replacement.rs::sections`, `:59`), naming the contracts that live in it.
- Verdict semantics (`:77-90`) and `verdict_schema()` (`:238-258`):
  - rule on each contract; `accept` when its replacement, or the patch you give for it, still sets up its `scenario` and still fails on its `rejects` implementation;
  - an accept must change something: its file has a replacement or your `plan_patch` targets it. A ruling that changes nothing is refused when you record it;
  - a failure caused by where completion runs (no network, no nested OS sandbox, a write outside the tree) is not a reason to accept as is: accept a replacement that tests the same scenario in a form that runs there, or, when none can exist, delete the contract with a `plan_patch` and say what it leaves untested;
  - your own run of a contract that starts an OS sandbox proves nothing about the contract when your session is itself sandboxed;
  - a file holding several contracts is written only when all of them are accepted.
- Keep the 100,000-byte cap behaviour (`prompt.rs:61`, `truncate.rs`): replacement sections are truncated before specs.
- Tests pinning this text: `prompt/contract_tests.rs:256-318`, `contract_replacement_tests.rs:80`, `:94`, `:117`, `contract_choice_tests.rs`, `contract_no_adapter_tests.rs`, `contract_frozen_set_tests.rs`, `prompt/tests_kinds.rs:76`. Update them to the new wording; keep what each one proves.

## Stage-facing text

- `signals/v2_section.rs::append_frozen_contracts` (`:32-85`): the dispute bullets (`:72-78`) show `--contract <id> [--contract <id> ...]` and `--replacement <frozen-file>=<path>`; state that one filing names every contract with the same cause, that a filing is refused while a frozen file is edited, and that every change to a frozen file travels as a replacement. `v2_section_tests.rs:79`, `:92` pin the old strings.
- `signals/helpers.rs:277` (`STAGE_EXIT_RULES`) and its twin in `tests_doctrine_v2.rs:43`; `stage_service/control_block.rs:114` and `control_block_agent_tests.rs:105`.
- `README.md:584`, `:1055`; `skills/loom-plan-writer/SKILL.md` (Section 9), `skills/loom-plan-writer/references/v2-contracts.md` (Sections 3 and 5), `skills/loom-orchestration/SKILL.md` ("Plan version 2 stages"). Find them with `rg -n 'dispute-contract' README.md skills`.
- `loom/tests/fixtures/signal_replay/plan.md` and `loom-hooks/tests/loom-relay-kinds.sh`, `loom-relay-guard.sh`: add a case with two `--contract` flags; the classifier (`loom-hooks/_relay_kinds.sh:53`) matches by token and needs no change.

## Amend guard (`commands/stage/amend.rs`)

```rust
pub fn refuse_stage_session(worktree_path: Option<&str>) -> anyhow::Result<()>
```

`Ok` for `None` and a blank value; otherwise an error: "This session runs inside the stage worktree at {path}, so it cannot amend the plan: a stage may not change its own checks. `loom stage amend` is an operator command; dispute a wrong check instead." Model it on `refuse_worktree_session` (`commands/stage/adjudicate.rs:59-68`). `amend()` (`amend.rs:30`) calls it first with `std::env::var("LOOM_WORKTREE_PATH").ok().as_deref()`. Say so in the command's help (`cli/types_stage.rs:219`) and at `README.md:592`.

## Acceptance

- `cargo test --manifest-path loom/Cargo.toml --lib orchestrator::adjudication::prompt`, `--lib orchestrator::signals::`, `--lib stage_service::control_block` and `--lib commands::stage::amend` pass.
- `rg -n -- '--contract <contract-id> --reason' loom/src README.md skills` finds no single-contract form left.
- Report every assertion you changed in existing tests.
