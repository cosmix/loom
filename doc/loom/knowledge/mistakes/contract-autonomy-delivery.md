# Contract Autonomy Delivery

> Chain bypass, validation

## The Chain Check Read Attributes by Position and Refused by Deny-List

**What happened:** `#![r#cfg(not(test))]`, `#![/**/cfg(..)]` and `#!/**/[..]` passed the contract chain check, so an implementer could compile the frozen contract out under `cfg(test)` while a same-named shadow test passed. The review found it as the critical finding of the integration-verify round.

**Why:** the check read an attribute's path positionally (`named_child(0).named_child(0)`) and refused inner attributes with `starts_with("cfg")`. tree-sitter extras (comments) shift positional children, raw identifiers change the path text, and tree-sitter-rust 0.24 lexes `#!/**/[..]` at a file's start as a shebang node. The contract tests and mutations covered only plain spellings.

**Prevention:** read tree-sitter nodes by kind or field, never by position; normalise `r#` per path segment; treat an unreadable path or a shebang in a chain file as refused; test the evasive spellings beside the plain ones. An allow-list fails closed where a deny-list does not (an open suggestion: inner attributes are still deny-listed while outer ones are allow-listed).

**Fix:** `verify/contracts/collection/rust/items.rs` and `chain.rs::file_problems`; detail in [contract-collection](../architecture/contract-collection.md).

## Recording Validated the Final Plan While Apply Validated Each Patch

**What happened:** an accepted freeze-refusal verdict that deleted a contract and then inserted another on a one-contract stage passed recording and wedged the apply, which refuses the empty intermediate plan.

**Why:** the fix brief said "run the same checks" without naming per-step semantics. The first (opus) fix added the missing checks but validated only the final plan; a failed fix moves up one tier, and the fable fix replayed the patches and ran `schema::validate` after each entry.

**Prevention:** recording-time validation of a multi-patch verdict mirrors the apply's order exactly: replay in array order with `plan::amendment_fields::apply_patch_vec`, validate after each entry, lint only the final plan (the apply never lints, so per-step lints would refuse sequences it accepts). Name the per-step rule in the brief.

**Fix:** `orchestrator/adjudication/verdict_freeze_plan.rs`.

## Names Were Escaped Where Refusing Was the Only Fail-Closed Choice

**What happened:** worktree names with a newline or invalid UTF-8, and host-run log text, reached the adjudication briefing and could split into forged `[loom]`-labelled problem lines.

**Why:** an escaped or lossy name read back as a path can name a different existing (even ignored) file, so escaping cannot fail closed; `problem_lines` splits on newlines, so unquoted run output becomes several lines the judge reads as loom's own.

**Prevention:** refuse such names at the listing source (`changes::listed_name`); record host error, job detail and log text as ONE `run-output:` problem and render it `[run output, unverified]` in a fence. Open siblings of the same class: `fs/safe_fs.rs` error text that interpolates a lossy component, and daemon `Response::Error` text that embeds git stderr (both in [concerns/contract-autonomy-followups.md](../concerns/contract-autonomy-followups.md#freeze-refusal-verdict-and-briefing-gaps)).

**Fix:** `verify/contracts/changes.rs`, `verify/contracts/refusal.rs`.

## A Worker's One Scoped Check Missed Its Consumers and the Maintainability Test

**What happened:** three misses in one plan. (1) A worker's single check (`verify::contracts::`) left 7 lib tests failing in `commands::stage`, `stage_service` and `host_verify`, which share `verify/contracts/test_support.rs`. (2) The fix round added two functions over 50 lines (`v2_lints/contracts.rs::check_collection` at 53, a 56-line test fn); the gate's `maintainability` test failed and the reviewer caught only one. (3) The main agent edited `verdict_freeze_refusal.rs` mid-wave and omitted a `T: Clone` bound, so a worker's compile failed (E0277) and its tests went unrun.

**Why:** briefs asked for `wc -l` on files, not for the test that counts functions (tests included); a scoped check named the module it edits, not the consumers of a shared `test_support`; a main-agent edit to a shared crate during a wave went unchecked.

**Prevention:** a worker that adds or grows functions runs `cargo test --manifest-path loom/Cargo.toml --test maintainability` as part of its one check; a brief names a check that covers every module whose tests the worker adds and every consumer of a shared test-support file it edits; during a wave, run `cargo check` right after any main-agent edit or defer the edit until every worker returns.

## A Package-Free Fixture Base Detected Cargo and Changed Digests

**What happened:** a `Cargo.toml` added to the shared `contract_worktree` test base made runner-less shell contracts (`host_verify/tests_freeze.rs`) detect `cargo-test`, run cargo in the worktree and change its digest.

**Why:** `resolve_adapter` detects the owning package's runner when `contract.runner` is `None`.

**Prevention:** a fixture whose contract test is a shell command needs a package-free base; `test_support` offers `shell_contract_worktree` beside `contract_worktree`. An empty `.rs` file also fails `cargo fmt --check` (rustfmt wants a newline), so a scratch module needs real content.

## A Relay Absence Test Asserted an Empty Sink

**What happened:** a test of the `relay_verdict` absence notice asserted empty stderr and failed.

**Why:** `emit()` writes the PENDING RELAY text to the same sink.

**Prevention:** assert on the specific notice text with `contains()`, never on an empty relay sink.

## A Worker Table Left Shared Re-Export Files Unowned

**What happened:** the contract-autonomy worker table gave the new filing functions to one worker and their callers to another, but left `stage_service/mod.rs` (the `pub(crate) use` that makes `file_freeze_refusal` reachable) and `commands/stage/adjudicate.rs` (the relay pre-check, which used criterion rules for freeze-refusal verdicts) to nobody. Until the orchestrator assigned both to one worker, the new entry points were dead code outside tests.

**Why:** the table listed the files each feature creates, not the files that must change for a new item to be reachable.

**Prevention:** for each new `pub(crate)` item, the ownership table names the module file that re-exports it and each dispatcher that must route the new kind; a private `mod` needs its re-export in the parent (`signals::contract` is private, so `scaffold_line` needed a `signals/mod.rs` re-export).
