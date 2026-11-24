# Review Remediation Stage Practice

> Stage-practice mistakes: scratch rm, cwd, fmt, gate timing, briefs

## A Scratch-Setup Command Deleted the Main Checkout's Knowledge Directory

**What happened:** a stage ran `rm -rf doc/loom/knowledge` after a `cd <tmp clone>` that had failed, chained with `;` instead of `&&`, so the delete ran in the main checkout. The `git clone --local` into a `TMPDIR` on another device had failed on a cross-device hardlink, leaving the cwd in the main repository.

**Why:** a relative `rm -rf`, a `;` after a `cd`, and a clone mode that depends on both paths sharing a device.

**Prevention:** build scratch trees only with absolute paths, never `rm -rf` a relative path, never chain with `;` after a `cd`, and clone with `git clone --no-hardlinks`.

**Fix:** the main agent restored the tree with `git checkout -- doc/loom/knowledge`; uncommitted edits to four knowledge files were lost.

## A Contract Session's Shell Left in a Subdirectory Made the Sandbox Create `.mcp.json`

**What happened:** running Bash with the cwd in `loom/src` made the sandbox create an empty mount-point file `loom/src/.mcp.json` (it masks `.mcp.json` in the cwd). The contract freeze refused the file as an unlisted change, and `rm` inside the sandbox failed with `EBUSY`, so the operator had to remove it outside.

**Why:** the shell cwd carried into `loom/src` after a `cd`.

**Prevention:** in a contract session keep every Bash cwd at the worktree root; use absolute paths or `cargo --manifest-path` instead of `cd` into subdirectories.

**Fix:** the operator removed the file; the freeze dispute named it as the only refusal reason, not contract work.

## A Briefed Fixture or Owned-File List Missed Something Already in the Tree

**What happened:** a contract-neighbour fixture used a wiring check with `literal: true` in a version 1 plan (needs version 2) and duplicated `ScratchHome` as `HomeGuard`; another stage's owned-file list omitted `confine_provision_tests.rs`, whose test called `sccache_socket_rule`, which the stage deleted, so a one-line edit outside the list was needed.

**Why:** codex and sonnet briefs did not name the existing helpers, and nobody searched the crate for the symbols the change removed.

**Prevention:** name existing fixtures and helpers in the brief, keep version 1 fixtures `literal: false`, and `rg` a symbol across the crate before listing owned files (a deleted or renamed symbol's callers are owned too).

**Fix:** the fixtures reused the helper and the file was edited and reported.

## A Work-List Brief's Replacement Bodies Carried Seven Wrong Claims

**What happened:** a knowledge-repair brief gave verbatim replacement bodies for stale sections; seven of their claims were wrong against the tree (a grammar-pack gap message, a method listed as live that was dead, a function named `run_git_stream` that is `git.run_stream`, a test said to re-run `current_exe` that does not, drifted line references, a log bullet already fixed, a wrong line count).

**Why:** the brief's author verified the stale claim but not every sentence of the new body.

**Prevention:** before applying a correction list, verify the new body's claims against the tree, not only the "Tree today" evidence; read the cited lines of every sentence you write.

**Fix:** the stage deviated from the brief where its bodies were wrong and recorded each deviation.

## A Maintainability Refactor Added a Clippy Error Only the Lib-Test Run Showed

**What happened:** extracting `run_test_hook` to meet the 50-line function limit introduced `clippy::type_complexity` in `cfg(test)` code, visible only in the lib-test clippy run.

**Why:** the fix worker ran the maintainability test but not clippy.

**Prevention:** follow a maintainability refactor of `cfg(test)` code with `cargo clippy --all-targets` before the gate.

**Fix:** the helper's signature was reworked until `clippy --all-targets` passed.

## The Full Integration-Verify Gate Was Started Before the Last Review Round Returned

**What happened:** `loom check integration-verify` (about an hour of host time) was started right after a fix round, in parallel with the follow-up review; the review found a new defect, so the run could not be the final gate. Separately, the first gate on the merged tree failed two tests that no stage's own acceptance ran: `stand_in_signal_guard` flagged an unbounded `child.wait()` in a planner test, and `checkpoint_resumes_from_offset` failed because a prefix anchor now detects the in-place rewrite its probe relied on. The host job also runs in place in the stage worktree, so an edit made while it runs changes what later criteria see.

**Why:** "run `loom check` beside the final review" was read as "beside every review round"; a stage that changes `tokens/` or adds test processes was not told which test targets reach it.

**Prevention:** run the full gate only once the latest review round has returned with no open findings, and let fix rounds rely on the workers' targeted module tests. Wait for the job before editing. A stage that changes `tokens/` runs `--test token_flow_contracts`, and one that adds a test process runs `--test integration stand_in_signal_guard`.

**Fix:** the findings were fixed and the gate was rerun on the final tree.

## `rustfmt` Reached Files the Stage Did Not Own

**What happened:** `rustfmt` on `host_verify/mod.rs` also formatted every sibling module (other workers' and frozen files), because it recurses into out-of-line child modules; `rustfmt --edition 2024` on three files of a 2021-edition crate reordered their imports; and `cargo fmt --all` was refused by `subagent-verify-guard` as project-wide verification.

**Why:** the edition flag was guessed, not read from `loom/Cargo.toml`, and a file that declares `mod` children was passed as if it were a leaf.

**Prevention:** read the edition from `loom/Cargo.toml` (2021), pass leaf files only (never a `mod.rs`), and run `rustfmt --check` first when unsure; see [Formatting and Test Invocation in a Shared Worktree](../conventions/git-and-build-workflow.md#formatting-and-test-invocation-in-a-shared-worktree).

**Fix:** frozen contract files kept their line counts, the touch was reported, and `rustfmt --edition 2021` was rerun on the same files.

## A CI Panic Line Was Read Against the Working Tree

**What happened:** a panic at `tests_freeze_memo.rs:227` was read against the working tree, where it is a different assertion, and the "format gate fails to run" hypothesis built on it was wrong.

**Why:** the line belongs to the commit CI ran, not to `HEAD`.

**Prevention:** map a CI panic location with `git show <ci-sha>:<file>` before diagnosing.

**Fix:** the diagnosis restarted from the right assertion.

## Concurrent Subagents Overwrote a Shared Log Name

**What happened:** a fixed log name such as `t1.log` under `$TMPDIR` was overwritten by another worker's run in the same stage and showed foreign test output.

**Why:** `$TMPDIR` is shared by every concurrent subagent of a stage.

**Prevention:** write logs under the scratchpad directory with a worker-specific name.

**Fix:** the run was repeated with its own log.

## A Codex Turn Sandbox Option Was Blamed on bubblewrap Mounts

**What happened:** native Codex units could not link because `native_worker/protocol/wire.rs` `turn_start` sent `excludeSlashTmp` and `excludeTmpdirEnvVar` as true in the `workspace_write` sandbox policy, while `/tmp` was already a private writable tmpfs from the criteria bubblewrap boundary (`verify/criteria/confine_host.rs`).

**Why:** the Codex turn policy and the bwrap mounts both shape `/tmp`, and only the mounts were examined first.

**Prevention:** check the Codex turn `sandboxPolicy` before changing bwrap mounts.

**Fix:** `turn_start` sends `excludeTmpdirEnvVar: false` (`wire.rs:137`).

## A Doctrine Addition Broke a Size Pin

**What happened:** `skills/loom-orchestration/SKILL.md` is pinned at 21,794 bytes by `orchestrator/signals/tests_size.rs`, and `tests_doctrine_waiting.rs` pins the Exit 6 / RE-DELEGATE sentence in the checking-on-subagents paragraph; `CLAUDE.md.template` sits exactly at `CLAUDE_MD_TEMPLATE_MAX_BYTES` = 20,388, so one added paragraph (21,127 bytes) failed `claude_md_template_stays_under_its_size_ceiling`.

**Why:** the ceilings equal the current file sizes, so any addition fails until text is trimmed.

**Prevention:** budget a `SKILL.md` or `CLAUDE.md.template` addition against its ceiling first and pair it with an offsetting trim; raising a ceiling is a decision, never a subagent's edit.

**Fix:** the text was trimmed or moved to its home; `tests_size.rs` says the template ceiling equals its actual size, so any addition fails there.

## A New Root `.gitattributes` Was Ignored by `.gitignore`

**What happened:** the root `.gitignore` ignores `/.gitattributes`, so the new root `.gitattributes` the graph enumeration reads was invisible to `git add`.

**Why:** an ignore rule written for another purpose covered the file.

**Prevention:** `git check-ignore -v <path>` before adding a root dotfile, and `git add -f` when the ignore is intended.

**Fix:** the file was added with `git add -f`.
