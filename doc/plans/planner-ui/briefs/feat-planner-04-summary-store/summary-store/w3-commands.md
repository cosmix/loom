# W3: `seal`, `brief`, `status`, `release` and the CLI

Tier: sonnet (`loom-software-engineer`). Read `doc/plans/briefs/feat-planner-04-summary-store/common.md`
first. You start after W2 has returned.

## Files you own

`loom/src/planning/summary/seal.rs`, `seal/tests.rs`, `loom/src/commands/plan/mod.rs`,
`loom/src/cli/types_ops.rs`,
`loom/src/cli/dispatch.rs`, `loom/src/completions/dynamic/tests/tests_commands.rs`, and nothing else. The sections "brief.rs", "status.rs", "CLI" (the `commands/plan/summary.rs`
module) and "Docs" below are the rules codex units C3, C4, C5 and C6 implement in your wave; you
write `seal.rs`, the `SummaryCommands` enum and `PlanCommands::Summary` in types_ops.rs exactly as
`c5-command.md` pins them, `pub mod summary;` in commands/plan/mod.rs, the dispatch arm, and the
completions assertion.

## `seal.rs`

`seal(root, paths, id) -> Result<SealOutcome { sentences: usize, path: PathBuf }, SealError>`:

1. `store::validate_id`; `Project::load(root)`; the id must be in `store::expected_ids` (else
   `Rejected` "no such summary in this project").
2. Read `drafts/<id>.md` with `fs::safe_read::read_to_string_bounded(paths.dir, "drafts/<id>.md", 256 KiB)`
   (no-follow; a symlink fails).
3. Claim check (section 04 "Sealing a summary": `seal` deletes the claim of the summary it writes,
   and only the claimant rebuilds): read the claim for the id. A claim whose `session` differs from
   `$LOOM_PLAN_SESSION` (both set) -> `Rejected` "claimed by <session>". With `LOOM_PLAN_SESSION`
   unset (a terminal), any claim -> `Rejected` "claimed by <session or 'a terminal'>; pass --force"
   unless `force`. `seal(root, paths, id, force: bool)`.
4. `format::parse_draft`; then, INSIDE `with_store_lock` (W2's `write_summary_if` closure, so a member
   reseal cannot land between the check and the write), link-target rules 7-8 of common.md against
   the project and, for overviews, `evaluate::evaluate` of the members read under the lock (a member
   sentence must be Current). Every problem becomes a `FormatError` with its draft line; collect all,
   sorted by line.
5. Build `FrontMatter` (`file`, `cites`, `members` with `sections::normalized_hash`), `format::render`,
   write.
6. Delete the draft (`safe_remove_in_workdir` on a dirfd of `paths.dir`) and the claim only when the
   caller owns it (same session) or `force` was given. Missing ones are fine.

## `brief.rs`

`brief(root, paths, ids, note)`: validate every id; create every claim first (`claims::create_claim`);
if any is already claimed, release the ones this call created and fail naming the holder session.
Then print, per id: the id and kind; the source paths (document: its source; overview: the vision or
metaplans plus every member summary path); the current summary text (if any) with each sentence
prefixed `[stale] ` or `[keep] ` from `evaluate`; the note beside the sentence it names (match the note
against sentence text or id; otherwise print it at the top); the body rules of common.md as a short
list; the draft path (`paths.drafts/<id>.md`, absolute) and the exact `loom plan summary seal <id>`
command. Every line of document text goes through `crate::context::untrusted::inline_safe`
(`context/untrusted.rs:123`).

## `status.rs`

`status_report(root, paths) -> anyhow::Result<StatusReport>` (serde) with the JSON of common.md
("Commands"), summaries in `expected_ids` order then orphans; human output one line per summary
(`<state> <id>  <n stale>/<n> sentences  [claimed by <session>]`).

## CLI

`loom/src/cli/types_ops.rs`: add `PlanCommands::Summary { #[command(subcommand)] command: SummaryCommands }`
and the `SummaryCommands` enum of common.md with doc comments (`seal <SUMMARY_ID>`,
`brief <SUMMARY_ID>... [--note <TEXT>]`, `status [--json]`, `release <SUMMARY_ID>...`).
`dispatch.rs::dispatch_plan` (`:104`): one arm `PlanCommands::Summary { command } => plan::summary::execute(command),`
(the file is under 400 lines; plan 1 added the Check arm). `commands/plan/summary.rs::execute`:
resolve the root with `git rev-parse --show-toplevel` (as `commands/plan/check.rs` does), the paths
with `runtime::runtime_paths(&root)` (plan 1: honours `LOOM_PLAN_STATE`, which plan 7 sets inside
every planning session while dropping `LOOM_HOME`; contract `summary-cli-uses-session-state`), run
the subcommand. `Seal` gains `#[arg(long)] force: bool`. Exit codes: 0 success; 1 a rejected seal
(print `<absolute draft path>:<line>: <message>` per problem to stderr), a claimed brief, or a refused
release; 2 any other error. `commands/plan/mod.rs`: `pub mod summary;`.

## Completions

Completions derive from clap. `test_complete_subcommands_plan` in
`loom/src/completions/dynamic/tests/tests_commands.rs` asserts the exact list (plan 1 made it
`["verify", "check"]`): change it to `["verify", "check", "summary"]` and add a case
`complete_subcommands`-style for `plan summary` listing `seal`, `brief`, `status`, `release`. The
assertion change is a declared TI-edit event (stage description).

## Docs

`README.md` "Plan Commands": the four subcommands with one line each, the draft and claim locations
under `~/.loom/plan/<project-key>/`, that they need no server, and the exit codes.
`skills/loom-usage/SKILL.md`: a short paragraph next to the plan check text. Plain prose
(CLAUDE.md Rule 19).

## Tests (yours, `seal/tests.rs`)

Overview members computed in id order; a cite of a preamble (no fragment); seal of an archived plan
named by a metaplan; brief output contains `[stale]` for a stale sentence and the note.

## Proof

`cargo test --lib planning::summary::`, `cargo test --test planning_summary_cli_contracts`,
`cargo test --lib completions::`, `cargo run -q -- plan summary --help`, `cargo clippy --all-targets -- -D warnings`.
