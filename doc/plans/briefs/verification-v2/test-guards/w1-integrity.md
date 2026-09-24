# test-guards / W1 — test-integrity events, gate, `loom stage review integrity`

Read first: `doc/plans/briefs/verification-v2/DESIGN.md` D0, D6, D13. Knowledge:
`mistakes/tests-that-cannot-fail.md` "The Common Root Cause";
`mistakes/pinned-literals-ledgers-and-wiring.md` "Maintainability Ratchet Fails on Shrinkage,
Not Just Growth". Code: `testrun/languages.rs` (profiles: `for_path`, `test_file_globs`,
`test_declaration`, `assertion`); `verify/review/fingerprint.rs` (base = merge-base, change
listing, scaffold exclusion; reuse its helpers rather than re-list changes).

## Files you own

`loom/src/verify/integrity/{mod,count,edits,gate,tests}.rs` (new),
`loom/src/cli/types_stage_review.rs` (add `Integrity` to `ReviewCommands`),
`loom/src/commands/stage/integrity_status.rs` (new), `loom/src/commands/stage/mod.rs`,
`loom/src/cli/dispatch_stage.rs` (`dispatch_stage` 51 ledgered).

## API you publish (pinned for W3 and for dispute-kinds)

```rust
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct IntegrityEvent { pub id: String, pub kind: EventKind, pub language: Option<String>, pub path: Option<String>, pub base: Option<u64>, pub current: Option<u64>, pub current_sha256: Option<String>, pub detail: Vec<String> }
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EventKind { DeclTotal, AssertTotal, AssertionEdit, Ratchet }
pub fn current_events(worktree: &Path, target_branch: &str, ratchet_files: &[String]) -> Result<Vec<IntegrityEvent>>;
pub fn load_accepted(work_dir: &Path, stage_id: &str) -> Result<Accepted>;        // reviews/<stage>/integrity.json, D13 shape
pub fn check(stage: &Stage, work_dir: &Path, worktree: &Path, target_branch: &str) -> Result<()>;
```

## Tasks

1. `count.rs`: for every language profile, sum declaration and assertion matches over the test
   files present at base (`git ls-tree -r --name-only <base>` filtered by `for_path`, contents
   via `git show <base>:<path>`) and over the current worktree test files of that language.
   Events: `TI-decl-<lang>`, `TI-assert-<lang>` when the current count is lower.
2. `edits.rs`: for every test file that existed at base, `git diff <base> -- <file>` against the
   working tree. Removed lines matching the language's assertion regex, whose exact text
   (trimmed) does not appear among that file's added lines, raise `TI-edit-<path>`; `detail`
   holds those lines. Ratchet: every `ratchet_files` entry whose content differs from base
   raises `TI-ratchet-<path>`. For edit and ratchet events, `current_sha256` is the file's
   current hash.
3. `gate.rs::check` (v2 `standard` and `integration-verify`): every current event must be
   accepted and no worse than accepted, per D13. Otherwise bail with the event list and the way
   out: revert, or dispute (dispute-kinds adds the command name; say "dispute it" for now).
   Languages without a profile are noted, never an error.
4. `loom stage review integrity <stage-id>`: prints the current events with details, and which
   are accepted.

## Named tests (binding), in `verify/integrity/tests.rs`

Every test builds a temp git repo with a base commit on `main` and a branch:

- `deleted_test_declaration_is_an_integrity_event`: a Rust test file loses one `#[test]` fn →
  `TI-decl-rust` with base 2, current 1.
- `removed_assertion_is_an_integrity_event`: one `assert_eq!` line deleted → `TI-assert-rust`.
- `edited_assertion_in_base_test_is_an_integrity_event`: `assert_eq!(x, 5)` changed to
  `assert_eq!(x, 6)` → `TI-edit-<path>` whose detail contains the old line (the counts are equal,
  so only the edit rule catches it).
- `moved_assertion_is_not_an_integrity_event`: the same assertion line moved within the file →
  no event.
- `ratchet_file_change_is_an_integrity_event`: a ratchet file edited → `TI-ratchet-<path>`.
- `accepted_event_passes_until_it_worsens`: `integrity.json` accepts `TI-assert-rust` at current
  4; the gate passes at 4 and fails at 3.

## Proof (one command, once)

`cargo test --manifest-path loom/Cargo.toml --lib verify::integrity::`

## Report

Files changed; the exact public API as written; `dispatch_stage` count; the proof result.
