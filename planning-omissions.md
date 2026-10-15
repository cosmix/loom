
---

## Pressure test: `PLAN-release-versioning-config-and-loom-dir.md` (2026-09-02)

Six investigators, five dimensions, every finding grounded in `file:line` before it was folded in.
`loom plan verify` was 0 errors before and after. Ranked by what they would have cost.

### Blocks execution

1. **The maintainability ledger was in no stage's `files:`.** `loom/maintainability-baseline.txt` is
   exact-match and *bidirectional* — it errors on shrinkage as loudly as on growth
   (`mistakes/pinned-literals-ledgers-and-wiring.md:22`) — and `loom/tests/maintainability.rs:13` is
   autodiscovered, so it runs under every stage's `cargo test --all-targets`. Four stages move
   ledgered entries: `config-foundation` adds a `dispatch` arm (pinned at 123),
   `init-auto-repair` *shrinks* `repair.rs` from 1131 plus three of its functions,
   `version-and-release` touches `self_update/mod.rs`, `loom-dir-migration` moves eight entries.
   Every one of those stages would have gone red on a correct implementation.
2. **`loom/tests/**` was unowned while the gate compiled and ran it.** 25 files, 86 `.work`
   references, including `e2e/helpers/git.rs:60`, the fixture builder every e2e test uses. The stage
   could not pass its own acceptance without writing outside its declared territory. A fourth worker
   now owns it, sequenced after the hook sweep it depends on.
3. **The foundation named three resolvers; there are five.** The one missed —
   `commands/common/mod.rs:20-35 find_work_dir`, its own unbounded upward walk — feeds
   `loom handoff`, `attach`, `sessions`, `graph`, `subagents` and `usage`. All six would have
   silently stopped finding state, and the stage's `rg` gate would have passed anyway.
4. **`loom init` applying "every fix" reaches outside the repo.** `fix_old_skill` and `fix_old_agent`
   *string-parse a filesystem path out of an English `issue.description`* before deleting under
   `$HOME/.claude` (`repair.rs:775-843`), and `execute(fix)` never prompts. In a stage sandbox both
   are denied and `apply_fixes` counts rather than aborts, so init would print `Issues failed: N` on
   every invocation — contradicting the stage's own "silence on a clean workspace" requirement. Now
   an explicit allow-list. (Overstated in one report as `remove_dir_all` on a live `.work/`; that
   fires only on the "neither directory nor symlink" branch. Corrected before folding.)
5. **`~/.loom/` was not writable.** `allow_write` is additive, but listed only `loom/target/**`.
   Three stages write `~/.loom/config.toml` or `update-state.json`, and integration-verify's own
   proof is a config round-trip. Every one would have hit EPERM.
6. **clap cannot simply be given a `-v` alias.** A manual `ArgAction::Version` arg collides with the
   auto-generated `--version`; the shape is `disable_version_flag = true` plus the manual arg. And
   the commit must be in `version`, not `long_version`, or the functional proof fails.
7. **`config-tui` could not add `--print` from its declared scope**, and the file it needed
   (`cli/types.rs`) was owned concurrently by `init-auto-repair`. Moved the flag to
   `config-foundation`, which already owns both CLI files.
8. **`config-foundation` could not declare its own module** — `loom/src/commands/mod.rs` was absent
   from `files:`, so `commands/config/` would have been unreachable and the build would fail.

### Silently ships broken

9. **`build.rs`'s `cargo:rerun-if-changed` paths cannot resolve.** Build scripts run with CWD =
   `loom/`, so `.git/HEAD` means `loom/.git/HEAD`; and seven of nine stages build in a worktree where
   `.git` is a *file*. Cargo treats a missing rerun path as permanently dirty — verified with a probe
   crate — so every `cargo build`, `clippy` and `test` would have been a full rebuild, under a 300s
   ceiling. The plan's own text said "without those the version silently goes stale."
10. **The release workflow's `build` job checks out shallow and tagless** (`release.yml:43`, bare
    `actions/checkout@v7`; the `fetch-depth: 0` at `:98` is on the job that never compiles). Every
    published binary would have shipped `0.0.0-dev+unknown`.
11. **The repo has zero tags**, so the no-tag branch is the path every build takes — and it produced
    no commit hash, which integration-verify explicitly requires. Also means the two interesting
    branches are unreachable in any environment this plan runs in, so the derivation had to move into
    `loom/src` to be testable at all.
12. **`--skip daemon::rpc::tests` is a substring filter, not a prefix filter.** It removed six tests,
    not two — four of them socket-free, and two of those read the `user.token` path this plan moves.
    True coverage loss was 12 tests, not 8. Now skipped by exact path.
13. **A filtered `cargo test` matching nothing exits 0.** Every exact-test gate in the plan was
    vacuous, including the one pinning a test in a file the stage was instructed to edit. All now
    assert a passing count.
14. **Acceptance has a hard 300s per-criterion ceiling** that the plan schema cannot raise, and a
    cold worktree build of this crate exceeds it. The failure reads as a hang, not a slow build.
15. **`deny_read` replaces rather than merges**, so listing four credential globs silently dropped
    the two daemon IPC token denies from `default_deny_read()`.
16. **Three unnamed sweep surfaces, each failing silently:** `target_is_worktree` probing `.work`
    (strips a real worktree's escape denies); a *second* resolved-symlink settings generator at
    `git/worktree/settings.rs:415-447`; and `fs/permissions/sync.rs`, where `../../` becomes
    `../../../` — the same arithmetic as the symlink, pinned by tests.
17. **`gpt-5.6-sol` is the pressure-test model, not an implementer tier.** `codex-forward.sh` accepts
    the string, so it fails quietly rather than loudly, while the signal loom generates names terra
    and luna.

### Wrong assumptions, corrected against the tree

18. **The spool-drain trap described a hazard that does not exist.** The drain reads stage ids and
    joins a fixed `SPOOL_RELPATH`; it never enumerates candidates, so there is nothing to
    `symlink_metadata()`-skip. The real hazard is `git/cleanup/worktree.rs:176`, whose
    `remove_required_symlink` is a no-op on a missing path and strands the worktree.
19. **`project_root()` has the same off-by-one as `main_project_root()`, at 18 call sites** — and the
    hop count must follow the layout that resolved. An unconditional two-hop breaks the legacy `.work`
    fallback that this plan's own orchestration depends on.
20. **The `.work`-named-base branch was unmentioned.** Every hook passes `LOOM_WORK_DIR` naming the
    state directory itself; after the move `file_name()` is `work`, not `.work`, so the resolver
    appends a second copy and yields `<root>/.loom/work/.loom/work`.
21. **The `sun_path` check was placed in two functions that cannot return an error.** Retargeted to
    the bind site in `lifecycle.rs`.
22. **The YAML contradicted its own prose** on whether the doctrine equality tests scan `agents/*.md`.
    The prose was right; the YAML — which is what the executing agent reads — was wrong.
23. **`hooks/_common.sh` has no `.work` state paths.** All five matches are `.worktrees` membership
    logic. Sending a sweeping worker at the 1500-line file every hook sources is how that gets
    mangled.
24. **The hook and Rust territories are not disjoint.** Every hook is `include_str!`'d into the
    binary, and a doctrine test asserts `HOOK_CODEX_FORWARD.contains(".work/")`.
25. **Three claims about self-update were wrong:** it does not "update nothing" (the binary is
    swapped first, leaving a *half-updated* install); `self_update/tests.rs` pins nothing; and the
    literal occurs at eight sites, not two.
26. **The scale figure did not reproduce.** "1006 references across 265 files" matches no query;
    the real number is 1523 across 309, excluding `loom/tests/`.

### Under-specified

27. Detached-fetcher lifecycle (stdio, session, cwd, concurrency, write atomicity) was deferred
    entirely — and a detached child inheriting the acceptance criterion's piped stdout hangs the
    output collector.
28. "The daemon's own re-entry" names something that does not exist; the daemon forks in-process.
29. Config write atomicity and parse-failure behaviour were unstated, for a file written concurrently
    from hooks.
30. The TUI's panic hook sits *inside* the line range the plan cited and was omitted; without it a
    panic leaves the terminal in raw mode.
31. Two questions the plan deferred to the executor were already decidable: `theme`/`widgets` are
    public along the whole path, and `commands/init/execute.rs` exists.
32. No stage ran the `docs` CI job (`RUSTDOCFLAGS: "-D warnings"`) while the plan adds three public
    modules, and `RUSTFLAGS: "-Dwarnings"` never covered the new `build.rs`.

### The recurring shape

Two classes account for most of the above. **A gate that proves existence rather than behaviour** —
`rg -q` on a file the sweep rewrites, a filtered test that matches nothing, `--help` as the only proof
of a subsystem, `exit_code: 0` as the only success criterion. And **a ledger, a test, or a generator
that is a second copy of something the plan edited once** — the maintainability baseline, the second
settings generator, the fourth resolver, the `include_str!` coupling. Both are found the same way:
ask what would still pass if the work were never done.

**Addendum (same pass).** Three doc comments assert the old symlink shape and are caught by no test —
`git/worktree/settings.rs:52`, `fs/work_dir.rs:364`, and the `is_worktree_scaffold_path`
`.loom/config.toml` carve-out at `settings.rs:31-33`. The stale-comment residue of a large rename is
already a recorded pattern (`mistakes/refactor-stragglers.md`); the plan now names all three.

---

## Second pressure pass: `PLAN-release-versioning-config-and-loom-dir.md` (2026-09-02)

Thirteen findings reviewed. Nine had real substance; four were written against the draft the first
pass had already fixed. `loom plan verify` was 0 errors before and after. Ranked by cost.

### The plan asserted two things that cannot both be true

1. **"Old `.work/` workspaces stay readable" AND "nothing ever writes `.work/`".** `WorkDir` has one
   field — `root: PathBuf` (`fs/work_dir.rs:89-91`) — and every accessor derives from it, mutating
   ones included (`write_config` `:451-462`, `merge_section` `:471-495`). There is no read-root /
   write-root split, so a resolved legacy root is written to or it is not the workspace at all. The
   plan promised both and left the executor to pick, which is the worst of the three outcomes.
   **The tell:** two guarantees stated in different sections, never checked against the one struct
   that has to satisfy both. The plan's *own* back-compat rationale ("reinstalling loom partway
   through the run would strand the orchestration") already implied writes, and that contradiction
   sat two paragraphs from the claim it refuted.

2. **Three behaviours branch on which layout resolved, and only one was named.** The hop count was
   handled. `ensure_work_symlink` (`git/worktree/settings.rs:49-68`) hard-codes *both* halves —
   `repo_root.join(".work")` and `Path::new("../../.work")` — and takes `repo_root`, not a `WorkDir`,
   so it cannot see the layout at all. Sweeping it to the nested spelling breaks every worktree a
   legacy workspace creates, including this plan's own run.

### A goal the plan stated and did not deliver

3. **`loom self-update` is broken three ways; the plan fixed one.** It renamed the checksum asset and
   declared the goal met. Also wrong at HEAD: `GITHUB_REPO = "cosmix/claude-loom"`
   (`self_update/mod.rs:39`) against an origin of `cosmix/loom`, and an asset name built from the
   **target triple** (`format!("loom-{target}")`, `:113-150`) against a workflow that publishes
   **os-arch** names (`loom-darwin-arm64`, `release.yml:26-37`). No supported platform's asset is
   ever found. **The tell:** the plan grounded the one defect already recorded in `concerns.md` and
   never read the rest of the function it was editing. A recorded defect is where to start looking,
   not the boundary of the search.

4. **Two of four advertised config keys had no consumer.** `terminal.backend` and
   `context.ceiling_tokens` are read only from the *workspace* config
   (`commands/run/mod.rs:166`, `orchestrator/terminal/backend.rs:96`, `monitor/core.rs:51`,
   `hook/context_ceilings.rs:43`), and the plan put workspace precedence out of scope. Setting them
   would have persisted and displayed a value nothing reads — the "writer and reader disagreeing on
   one address" failure the knowledge base already records, shipped as a feature. **The tell:** the
   key list was written from what a config surface *ought* to expose, never traced forward to a
   consumer. Both funnel through two one-line functions, so the fix was small — but only once
   somebody asked who reads them.

### Ordering and output contracts, assumed rather than read

5. **`init` auto-repair was ordered after the failure it heals.** `init::execute` calls
   `validate_work_dir_state` at `commands/init/execute.rs:75`, before anything else, and that
   validator bails on the corrupted symlink with "Or run: `loom repair --fix`"
   (`fs/work_integrity.rs:101-121`). Any repair call placed later never runs on the one case that
   matters. And `repair::execute` prints a logo, a mode banner and a "workspace is healthy" line
   (`repair.rs:132-158`), contradicting the same stage's "silence on a clean workspace" requirement.
   **The tell:** the plan specified *what* to call and *what* to allow-list, never *where in the
   sequence* — and the wiring pattern it chose (`repair::`) cannot distinguish any of it.

6. **A failed update fetch never stamped `last_checked`.** The plan specified an `O_EXCL` lock,
   atomic state writes, `setsid` and `/dev/null` stdio — thorough on concurrency, silent on failure.
   The lock stops two fetchers running at once; it does nothing about a stale record that makes
   *every* invocation spawn one. With loom called on every Claude Code hook, a network outage
   becomes a fork storm. **The tell:** the success path was designed in detail and the failure path
   was assumed to be its absence.

### Gates that matched the repository's own gate only in shape

7. **`--no-fail-fast`, `cargo audit`, and rustdoc on three stages** were missing against
   `loom/.githooks/pre-push:90-117`, whose error text says outright why the flag exists. The plan
   quoted that file for its skip-list reasoning and did not diff the command itself. `cargo-audit`
   also is not installed on this host, which is why it now runs once, in the stage whose description
   installs it first.

8. **Count floors that any single surviving test satisfies.** `rg -q "test result: ok\. [1-9]"` on a
   nine-case module passes on one. The plan had already learned that a filter matching nothing exits
   0 and wrote count assertions — then set the floor at 1.

### Four findings that were already wrong

Recorded because "the reviewer said so" is not a reason to change a plan. The spool-drain trap
(retracted in the first pass, with the real hazard at `git/cleanup/worktree.rs:176` named in its
place); `--print` registration in `config-tui` (already moved to `config-foundation`); the
maintainability baseline being unowned (four stages own it); and the doctrine acceptance quoted as
`one subagent per` when the plan uses the phrase-anchored `never one subagent per task`. A review
written against an earlier draft drifts exactly like a line reference does.

### The recurring shape, this pass

Last pass it was *gates that prove existence rather than behaviour*. This pass it is **a promise
checked against the reader's intuition instead of against the one thing that has to satisfy it** — two
guarantees against one struct field, four config keys against zero consumers, a goal against one
line of a function, a repair call against an ordering nobody opened the file to check. Every one was
a single `rg` away. The question that finds them is not "is this claim plausible?" but "which
function has to be true for it, and what does that function do today?".
