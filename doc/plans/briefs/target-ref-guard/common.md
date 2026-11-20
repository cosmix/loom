# Common rules for every worker of PLAN-target-ref-guard

Read this file and your own brief in full before anything else. The plan is
`doc/plans/PLAN-target-ref-guard.md` (the lifecycle may rename it to `IN_PROGRESS-PLAN-...`);
its YAML is authoritative where a brief and the plan differ. Section 3 of the plan ("Design")
is the behaviour you implement; the "Shared interface" section below is the exact surface
other workers and the frozen contract tests compile against. Never rename anything in it.

## What you own

- You write only the files your row of the stage's worker table lists. A file you need to
  change that no row lists is an ownership gap: stop, record it with
  `loom memory note "found: <file> needs <change> for <reason>"`, and name it in your report.
  Never edit another worker's file.
- Frozen contract files and harness files (listed in your brief) are read-only. Your code must
  make them pass as written. A contract you believe is wrong goes in your report with the
  reason; the main agent disputes it.
- You never run git against this repository (no add, commit, stash, checkout, reset), never
  spawn subagents, never write `.loom/` or `doc/loom/knowledge/`, never run
  `loom stage complete`, and never use Claude Code auto-memory.
- Verification is the main agent's job. Run at most one check, once: the command your brief
  names. A compile error in a file you do not own is not yours: report it with its file:line.
- The stage's working directory is `loom/`: run every command from there (never `cd loom`), and
  read paths in briefs as relative to `loom/` unless they start with `doc/` or `loom-hooks/`.
- Keep every line you write under 100 columns; the main agent runs `cargo fmt --all` and the
  size counts below are taken after formatting.

## Anchors

Line numbers in the briefs were read at `066e7667` and are advisory. Locate every edit by
symbol (`loom map --outline <file>`, `rg -n '<symbol>'`) and read the surrounding code first.

## Size limits and the maintainability ledger

Files stay at or under 400 lines and functions at or under 50, counted after `cargo fmt`.
`loom/maintainability-baseline.txt` records exact counts for items already over a limit, and
`cargo test --test maintainability` fails when a recorded entry grows OR shrinks.

- A recorded item may never grow. Make room by extracting a helper, never by reformatting or by
  deleting comments that carry meaning.
- A recorded item that shrinks gets its ledger line lowered to the new exact count (removed when
  it drops to its limit or below). Touch no other ledger line. The ledger is a plan
  `ratchet_files` entry, so any change to it raises `TI-ratchet-loom/maintainability-baseline.txt`;
  the main agent disputes it once after the final review round. Name every ledger line you
  changed in your report.
- Recorded items this plan touches: `src/orchestrator/core/recovery.rs` (file 1462;
  `sync_graph_with_stage_files` 508; `verify_merged_true_or_revert` 56),
  `src/orchestrator/core/merge_handler.rs` (file 616),
  `src/orchestrator/core/stage_executor.rs` (file 597; `start_stage` 149; `start_ready_stages` 65;
  `start_knowledge_stage` 99), `src/git/worktree/operations.rs` `create_worktree` 115,
  `src/cli/dispatch.rs` `dispatch` 86, `src/commands/init/execute.rs` `execute` 130. Test
  functions and test files are measured too (`tests/*.rs`, `*_tests.rs`): a new test fn stays at
  or under 50 lines.

## Tests and integrity

- Never edit an existing assertion line (`assert!`, `assert_eq!`, `assert_ne!`,
  `assert!(matches!(..))`) in a test file that exists at the stage's base: the integrity gate
  raises a `TI-edit` event. Add tests, or add setup lines before an existing assertion. When an
  existing assertion encodes behaviour this plan deliberately changes, leave it failing, name
  the test and the reason in your report, and stop: the main agent decides.
- Test names given in a brief are exact.
- Git in tests runs with ambient configuration shut out, as
  `loom/src/orchestrator/core/merge_lifecycle_e2e_tests_support.rs::git_output` does:
  `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM` pointed at missing files, `GIT_CONFIG_NOSYSTEM=1`,
  author and committer name and email set. Tests that spawn the loom binary go through
  `tests/integration/helpers.rs::loom_cmd()` (declared with
  `#[path = "integration/helpers.rs"] #[allow(dead_code)] mod helpers;`, as `tests/map_cli.rs`
  does); `tests/integration/binary_spawn_guard.rs` fails any other direct spawn.
- `attestation_mode` reads `core.hooksPath` at worktree, global and system scope too (with
  `--includes`, after guard-core's W2). A test that needs
  attestation on asserts `AttestationMode::Active` first, so a machine with a global
  `core.hooksPath` fails it with that reason instead of failing somewhere obscure.
- A test whose outcome depends on the hook's session branch (an unwritable ledger) must control
  `LOOM_SESSION_ID` itself (`env_remove` or set it), because `cargo test` runs inside a loom
  session in a stage (the session wrapper exports it). With a writable TempDir ledger the hook
  attests whatever `LOOM_SESSION_ID` holds, so those tests need no env handling.
- In-process code that refuses inside a session (`loom target accept`) is tested under
  `#[serial]` with `std::env::remove_var("LOOM_SESSION_ID")` first, as
  `commands/stage/tests/admin_proof.rs` does; `helpers::loom_cmd()` already clears it for the
  binary.
- A crafted commit in a test is a child of the current tip (`git commit-tree <tree> -p <tip>`)
  unless the test is about a rewrite.

## Production code rules

- Every git call in production code goes through `crate::git::runner` (`run_git`,
  `run_git_checked`, `run_git_bool`, `is_ancestor_of`), which prepends `-c core.hooksPath=/dev/null
  -c core.fsmonitor=false` and (after guard-core) sets `GIT_NO_REPLACE_OBJECTS=1` and
  `GIT_GRAFT_FILE=/dev/null`: loom's own git never runs repository hooks and ignores replace refs
  and grafts a session may have planted. Resolve branch names through
  `crate::git::branch::branch_ref` (a tag named like a branch must not win).
- Abbreviate object ids with `id.get(..12).unwrap_or(id)` (or `..7`), never `&id[..12]`: a hold
  built from an unreadable record carries an empty `accepted`.
- Errors: `anyhow::Result` with `.context(..)`; no `unwrap`/`expect` outside tests. Logging:
  `tracing::{info,warn}` plus `eprintln!` where the surrounding daemon code prints to the
  operator.
- Match the surrounding code's comment density and naming. Doc comments state what the code
  does now. No TODO, no stub, no `unimplemented!`. Operator-facing text is plain sentences.

## Shared interface (exact; other workers and frozen contracts depend on it)

State files, all in the state directory `W` (`.loom/work`), written only by loom on the host:

| File | Writer | Content |
| --- | --- | --- |
| `W/target-guard.json` | `git::target_guard` | `{"targets": {"<branch>": {"accepted": "<oid>", "hold": null or Hold, "attestation": true or false}}}` (`attestation` is the latch of plan section 3.2) |
| `W/target-guard.refs` | `git::target_guard`, rewritten with every record write | `# written by loom; read by .git/hooks/reference-transaction`, then one `ref refs/heads/<branch>` line per guarded target, then `allow doc/loom/knowledge/` |
| `W/target-guard.ledger` | the hook (host only), `target_guard::append_attestation` | append-only lines `attest <from> <to> <ref>` and `abort <from> <to> <ref>`; full object ids; the all-zero id for an absent value |

`loom::git::target_guard` (module `loom/src/git/target_guard/`):

    pub const RECORD_FILE: &str = "target-guard.json";
    pub const REFS_FILE: &str = "target-guard.refs";
    pub const LEDGER_FILE: &str = "target-guard.ledger";
    pub const HOOK_MARKER: &str = "LOOM_REFERENCE_TRANSACTION_HOOK";
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    pub struct Hold { pub accepted: String, pub observed: String,
                      pub reasons: Vec<HoldReason>, pub since: chrono::DateTime<chrono::Utc> }
    #[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
    #[serde(tag = "kind", rename_all = "snake_case")]
    pub enum HoldReason { NotFastForward, ControlPaths { paths: Vec<String> },
        Unattested { from: String, to: String, paths: Vec<String> },
        StageWork { branches: Vec<String> }, Unevaluable { error: String } }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum GuardState { Clear { accepted: String }, Held(Hold) }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub enum AttestationMode { Active, Off { reason: String } }
    #[derive(Debug, Clone, PartialEq, Eq)]
    pub struct Accepted { pub from: String, pub to: String }
    pub fn attestation_mode(repo_root: &Path, work_dir: &Path) -> AttestationMode;
    /// The record's latch for `target`: `true` once an evaluation found the mode `Active`,
    /// until `accept` stores the then-current mode. `false` when the entry is missing.
    pub fn attestation_latched(work_dir: &Path, target: &str) -> Result<bool>;
    pub fn check(repo_root: &Path, work_dir: &Path, target: &str) -> Result<Option<GuardState>>;
    pub fn check_locked(repo_root: &Path, work_dir: &Path, target: &str, tip: &str)
        -> Result<GuardState>;
    pub fn pending_hold(repo_root: &Path, work_dir: &Path, target: &str) -> Result<Option<Hold>>;
    pub fn record_advance(work_dir: &Path, target: &str, from: &str, to: &str) -> Result<()>;
    pub fn accepted_tip(work_dir: &Path, target: &str) -> Result<Option<String>>;
    pub fn recorded_hold(work_dir: &Path, target: &str) -> Result<Option<Hold>>;
    pub fn recorded_holds(work_dir: &Path) -> Result<Vec<(String, Hold)>>;
    pub fn merged_into_accepted(repo_root: &Path, work_dir: &Path, target: &str, commit: &str)
        -> Result<bool>;
    pub fn accept(repo_root: &Path, work_dir: &Path, target: &str, expected: &str)
        -> Result<Accepted>;
    pub fn guarded_refs(work_dir: &Path) -> Result<Vec<String>>;
    pub fn append_attestation(work_dir: &Path, reference: &str, from: &str, to: &str)
        -> Result<()>;
    pub fn hold_alert(target: &str, hold: &Hold) -> String;
    pub fn accept_command(target: &str, hold: &Hold) -> String;
    pub fn restore_commands(repo_root: &Path, target: &str, hold: &Hold) -> Result<Vec<String>>;

`StageWork` branch names are short (`loom/s`); `ControlPaths` and `Unattested` paths are
repository-relative as `git diff --name-only` prints them. Every function taking `target`
strips a leading `refs/heads/` first (an existing caller passes `"refs/heads/main"`), so record
keys are short branch names and each refs-file line is `ref <branch_ref(key)>`.

`loom::git::merge::MergeBlock::TargetHeld { target: String, accepted: String, observed: String }`
(serde `kind: target_held`).

`loom::git::hooks` (module `loom/src/git/hooks.rs`, already `pub mod hooks`):

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub enum HookInstall { Installed, UpToDate, ForeignHookPresent }
    pub fn install_reference_transaction_hook(repo_root: &Path) -> Result<HookInstall>;
    pub fn is_reference_transaction_hook_installed(repo_root: &Path) -> bool;

`crate::orchestrator::core::Orchestrator` (stage 2):

    pub(in crate::orchestrator::core) fn check_target_guard(&mut self)
        -> Option<crate::git::target_guard::Hold>;

CLI (stage 2): `loom target status` and `loom target accept --to <commit>`.

## Memory

Record mistakes, non-obvious decisions and surprises the moment they happen:

    loom memory note "mistake: ... Why: ... Prevention: ..."
    loom memory decision "chose X over Y" --context "because Z"
    loom memory note "found/gotcha: ... in <file>:<line>"

## Report

End with: files changed, every maintainability ledger line you changed, the check you ran and
its result, assumptions you made, anything unresolved. Nothing else.
