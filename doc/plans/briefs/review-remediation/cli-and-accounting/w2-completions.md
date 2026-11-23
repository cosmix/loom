# cli-and-accounting / w2-completions: dynamic completion routing by exact command path

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/cli-tui-webserver.md`, section CR-56, sub-claims b, c, d, e. Sub-claim a is refuted: change nothing in `main.rs`.

## Findings covered
- CR-56(b): `route_completion` returns nothing for a command path of depth three except `stage output <sub>`.
- CR-56(c): `stage` subcommands with a stage-id argument and the nested groups `stage contracts` and `stage review` have no routing.
- CR-56(d): `-S` and `--owner-stage` get no stage-id completion.
- CR-56(e): `find_command_named` returns the first command with that name anywhere in the tree, so `loom review ` (a leaf) offers the subcommands of `stage review`.

## Files
Own (write):
- `loom/src/completions/dynamic/mod.rs`
- `loom/src/completions/dynamic/commands.rs`
- `loom/src/completions/dynamic/tests/tests_routing.rs`
- `loom/src/completions/dynamic/tests/tests_commands.rs`

Read only: `loom/src/completions/dynamic/stages.rs` (`complete_stage_ids`), `loom/src/cli/types_stage*.rs`, `loom/src/cli/types_memory.rs`, `loom/src/completions/mod.rs` (re-exports `complete_subcommands`; its call shape does not change for the re-export line).
Frozen (never edit): `loom/src/completions/dynamic/routing_contract_tests.rs` (new, written before your work).

## Changes

### 1. Path-based lookup (`commands.rs`)
- Delete `find_command_named`. `command_at_path(root, path)` (already in the file, walks an exact path with `command_matches`) is the only lookup.
- `complete_subcommands(parent: &[&str], prefix: &str) -> Result<Vec<String>>` and `has_subcommands(command: &[&str]) -> bool` take a path. A top-level command is `&["stage"]`. They look up `command_at_path(&root, path)`; unknown path gives an empty list / `false`.
- Callers: `complete_after_command` and the `("stage", "output")` arm in `mod.rs`, and the `use` list in `mod.rs` (line ~23, `complete_subcommands` is imported there and re-exported by `completions/mod.rs` at line ~10; the re-export line needs no edit).

### 2. Routing (`mod.rs`, `route_completion`, `complete_after_command`, `complete_after_subcommand`)
Replace the `match cmd_path` at the end of `route_completion` with:
```text
[] => complete_commands(prefix)
[cmd] => complete_after_command(cwd, prefix, cmd)
path with 2 or more segments, and has_subcommands(path) => complete_subcommands(path, prefix)
[cmd, sub] => complete_after_subcommand(cwd, prefix, cmd, sub)
["stage", "output", sub] => complete_after_subcommand(cwd, prefix, "output", sub)
["stage", "contracts" | "review", _leaf] => complete_stage_ids(cwd, prefix)
_ => Ok(Vec::new())
```
(`complete_stage_ids` is already imported in `mod.rs`.) Write it as a real `match` with guards; the block above is the order and the conditions. In `complete_after_command`, `commands::has_subcommands(&[cmd])` then `complete_subcommands(&[cmd], prefix)`.
Remove the `("stage", "output") => complete_subcommands("output", prefix)` arm from `complete_after_subcommand`: the generic depth-two group rule now returns the same list (`stage output` has subcommands). Keep `("output", "set" | "get" | "list" | "remove")`, which the depth-three `stage output <sub>` route still reaches.
In the `("stage", "human-review" | "dispute-criteria")` arm of `complete_after_subcommand` add `"admin-proof" | "amend" | "dispute-contract" | "dispute-findings" | "dispute-integrity"`. These take the stage id as the first positional argument (`AdminProofArgs.stage_id` is optional but positional, `AmendArgs`, `DisputeFindingsArgs`, `DisputeContractArgs`, `DisputeIntegrityArgs`). `stage adjudicate` takes `--stage`, already handled by the flag-value arm, so it gets no positional arm.

### 3. Flag values (`mod.rs`, `complete_flag_value`)
Change the `"--stage"` arm to `"--stage" | "--owner-stage" | "-S"` (same body, `complete_stage_ids`). `-S` is the short of `--stage` on nine `memory` subcommands; `--owner-stage` is on `stage dispute-criteria`. `finite_flag_choices` runs first and returns `None` for these (no possible values), so the arm is reached.

## Interfaces shared with other workers
None. The contract file calls `super::route_completion(cwd: &Path, prefix: &str, prev_word: &str, cmd_path: &[&str], cmdline: &str) -> Result<Vec<String>>` (unchanged signature) and `super::commands::complete_subcommands(&[&str], &str)`.

## Tests
- `tests_commands.rs`: every call `complete_subcommands("x", ..)` becomes `complete_subcommands(&["x"], ..)` and every `has_subcommands("x")` becomes `has_subcommands(&["x"])`. Assertions are unchanged; this edits the existing tests `test_complete_subcommands_*`, `test_has_subcommands`, the pressure case near line 333.
- `tests_commands.rs`, new test `a_leaf_command_never_borrows_subcommands_from_another_command`: `complete_subcommands(&["review"], "")` is empty and `!has_subcommands(&["review"])` (top-level `review` is a leaf; `stage review` is a group). Also `complete_subcommands(&["stage", "review"], "")` equals `["integrity", "status"]` and `complete_subcommands(&["stage", "contracts"], "")` equals `["freeze", "restore", "show"]`. Fails today (first assertion, and the path argument does not compile).
- `tests_routing.rs`: add rows to the `cases` table in `routed_candidate_domains` (label, prefix, previous word, path, expected): `("stage contracts", "", "contracts", &["stage","contracts"], &["freeze","restore","show"])`; `("stage review", "", "review", &["stage","review"], &["integrity","status"])`; `("contracts show", "core", "show", &["stage","contracts","show"], &["core-architecture"])`; `("review status", "math", "status", &["stage","review","status"], &["math-core"])`; `("stage amend", "ui", "amend", &["stage","amend"], &["ui-framework"])`; `("stage dispute-findings", "core", "dispute-findings", &["stage","dispute-findings"], &["core-architecture"])`; `("owner-stage flag", "core", "--owner-stage", &["stage","dispute-criteria"], &["core-architecture"])`; `("short stage flag", "core", "-S", &["memory","note"], &["core-architecture"])`; `("leaf review", "", "review", &["review"], &[])`. The fixture workspace (`routing_workspace`) has stages `core-architecture`, `math-core`, `ui-framework`. `cmdline` in the loop is built from the path, so `-S` rows still get a `memory` word in the command line. Each row fails today.
- Contract to turn green: `completions::dynamic::routing_contract_tests::routes_follow_the_exact_command_path`.

## Proof
From `loom/`: `cargo check --all-targets`; `cargo test --lib completions::dynamic::`.

## Traps
- Do not keep `find_command_named` "for the old callers": there are none after the change, and a leftover lookup reintroduces the first-match bug.
- The group rule must come before the `[cmd, sub]` arm, or `stage contracts` is treated as a leaf and returns stage ids instead of `freeze`, `restore`, `show`.
- `["plan", "hook", ..]` returns early at the top of `route_completion`; keep that.
- Hidden subcommands stay out of the list: `visible_subcommand_names` already filters them; do not bypass it.
- The `-S` arm must not shadow `finite_flag_choices`; leave the call order in `complete_flag_value` as it is.
