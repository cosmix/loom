# C4: file-tool credential guard, the signal's cache note, plan-writer guidance

Stage `capsule-policy`, wave 1, tier sonnet. Read `../common.md` first (D3, D4 and D5 are the
decisions your text and hook follow).

## Why

The capsule's OS lists (`sandbox.filesystem.denyRead`, and after this stage `allowRead`) bind
Bash only; the native file tools (Read, Glob, Grep, Edit, MultiEdit, Write, NotebookEdit) never
touch the sandbox. `loom-hooks/credential-guard.sh` applies the read list to them, but its rule
(b) reads `$PROJECT_DIR/.claude/settings.local.json` (`credential-guard.sh:379-384`), a file
loom no longer writes: every session launches from its capsule
`$LOOM_WORK_DIR/capsules/$LOOM_SESSION_ID.settings.json`, so rule (b) is inert. You point it at
the capsule, teach it `allowRead` (D5 denies `R/.worktrees` and re-allows the session's own
worktree, so without it the guard would block every file-tool read in the worktree), and fix its
path spelling. You also rewrite the two texts that still say the package caches are writable.

## Files you own

`loom-hooks/credential-guard.sh`, `loom-hooks/tests/credential-guard-deny-read.sh`,
`loom-hooks/tests/credential-guard-tokens.sh`, `loom-hooks/tests/credential-guard-dotdot.sh`,
`loom/src/orchestrator/signals/format/helpers.rs`,
`skills/loom-plan-writer/references/sandbox.md`.

Never edit `loom-hooks/tests/run-all.sh` (the host-git-integrity stage owns it) and never
create a new hook test script: every case you add goes into the three credential-guard scripts
`run-all.sh` already registers. Never edit `format/sandbox_section.rs` or
`signals/tests_commit_timing.rs` (another stage's files).

## 1. `credential-guard.sh` rule (b)

Anchors at `3fc28031`; locate by function name.

1. **Where the lists come from.** Add

       # capsule_path - Echo this session's settings capsule, when the hook runs in a loom
       # session: $LOOM_WORK_DIR/capsules/$LOOM_SESSION_ID.settings.json.
       capsule_path()

   It returns 1 unless both variables are non-empty and `LOOM_SESSION_ID` is 1-128 characters
   matching `^[A-Za-z0-9][A-Za-z0-9._-]*$` (the rule `codex-forward-guard.sh:99`
   `valid_identity_id` uses), so no variable content can steer the path. Deny entries come from
   the capsule's `.sandbox.filesystem.denyRead` AND from
   `$PROJECT_DIR/.claude/settings.local.json`'s (kept: an operator may still write one, and more
   denies never widen anything). Allow entries come from the capsule's
   `.sandbox.filesystem.allowRead` only. A missing or unparsable file contributes nothing; rule
   (a) always stands.
2. **Spelling.** These are sandbox-list entries, not permission rules: `~` and `~/p` are under
   `$HOME`, `/p` and `//p` are both absolute, a bare `p` is project-relative
   (`doc/loom/knowledge/architecture/execution-containment.md`, "Sandbox-List vs Permission-Rule
   Path Syntax"). Rename `expand_deny_entry` to `expand_entry` and fix its `/*` branch, which
   today roots a single-slash path at the project; the capsule spells every absolute path with a
   single slash. Correct its comment block.
3. **Innermost wins.** Replace `deny_read_blocks` with a check that expands, splits and
   canonicalizes every entry exactly as today (`split_pattern`, `canonical_existing`,
   `pattern_matches`, unchanged), and records the length of the canonical prefix of the longest
   matching deny entry and of the longest matching allow entry. The target is blocked when some
   deny entry matches and its prefix is at least as long as every matching allow entry's
   (a tie denies). That is the mount order Claude Code builds: a denied directory becomes a
   tmpfs and the allowed paths inside it are bound back, while a deny inside an allowed path
   still wins. `DENY_ENTRY` names the matched deny entry, and the block message is unchanged.
   A plain directory entry already covers everything below it (`pattern_matches` with an empty
   remainder); keep that and cover it with a test.
4. Update the header comment (`:21-28`): rule (b) is the session capsule's `denyRead` and
   `allowRead` lists plus the project's local `denyRead` list, with the innermost match
   deciding. Keep functions small; the file is 417 lines today, so move nothing else in.

## 2. Hook tests (added to the registered scripts only)

All three scripts must stop reading a real capsule when they themselves run inside a loom
session: in each, the line that runs the hook gains `env -u LOOM_WORK_DIR -u LOOM_SESSION_ID`
in front (`credential-guard-deny-read.sh:25`, `credential-guard-dotdot.sh:26`,
`credential-guard-tokens.sh:34`). That is a change to the helper, not to any `check` line.

Add to `credential-guard-deny-read.sh`, with a fixture of its own under `$TMPROOT`: a work dir
holding `capsules/session-1.settings.json`, a repository with `.worktrees/s1/src/a.rs` and
`.worktrees/s2/src/b.rs`, a `.netrc` in the fake home, and a capsule whose `denyRead` is
`["<repo>/.worktrees", "//<fake home>/.netrc"]` (the first single-slash absolute) and whose
`allowRead` is `["<repo>/.worktrees/s1"]`. A second helper runs the hook with
`LOOM_WORK_DIR`, `LOOM_SESSION_ID=session-1` and `CLAUDE_PROJECT_DIR=<repo>/.worktrees/s1`.
Every boundary case has its control beside it:

- "Read in a sibling worktree is blocked by the session capsule" (exit 2) and "Read in the
  session's own worktree is re-allowed by allowRead" (exit 0);
- "Glob rooted at the worktrees directory is blocked" (2) and "Glob rooted in the session's own
  worktree runs" (0);
- "Read of a credential the capsule denies by // path is blocked" (2);
- "A capsule directory entry covers a file below it" (2, a file two levels under
  `.worktrees/s2`);
- "Without a loom session the capsule contributes nothing" (0: the sibling read with
  `LOOM_SESSION_ID` unset);
- "A malformed session id reads no capsule" (0: `LOOM_SESSION_ID=../session-1`, with the
  capsule also reachable under that spelling).

The five existing checks stay exactly as they are and keep passing.

## 3. `signals/format/helpers.rs`: `append_package_cache_note`

Replace the text (the body of `append_package_cache_note`; locate it by name, the stage-exits
plan moved its lines) and its doc comment. Keep the label
`**Package-manager caches:**` byte for byte: `signals/tests_commit_timing.rs:181` asserts it.
The new text says, in plain sentences:

- this session has a cache directory of its own: `CARGO_HOME`, `BUN_INSTALL_CACHE_DIR`,
  `npm_config_cache`, `npm_config_store_dir` (pnpm), `YARN_CACHE_FOLDER`,
  `GOPATH`/`GOMODCACHE`/`GOCACHE`, `UV_CACHE_DIR`, `PIP_CACHE_DIR`, `DENO_DIR` and
  `XDG_CACHE_HOME` point into it; it is seeded at spawn from the operator's caches for the
  ecosystems this worktree uses, and nothing written there reaches the next session (the next
  spawn of this stage replaces it at the same path);
- the operator's own caches are read-only: never point a tool back at them;
- `cargo add`, `bun install`, `uv sync` and `go get` work and download anything not seeded into
  the session cache (the registry must be on the stage's network allow-list);
- two things cannot happen in the sandbox: installing a Rust toolchain (`rustup` is read-only
  and auto-install is off) and fetching a cargo git dependency from a host the allow-list lacks;
- `EROFS` / `Read-only file system` from a package manager means a tool reached a real cache or a
  toolchain directory: block the stage with `loom stage block <stage-id> "<path> is read-only in
  the sandbox"`; never work around it.

PLAN-stage-exits-and-environment (merged first) already created an inline `#[cfg(test)] mod
tests` in `helpers.rs` with `the_package_cache_note_names_loom_stage_block`: the note must hold
`loom stage block` and must not hold `report it as a blocker`, a phrase that plan added to
`signals/tests_doctrine_blocks.rs::RETIRED_PHRASES`. Add both tests below to that existing `mod
tests`, and keep `the_package_cache_note_names_loom_stage_block` green (the last bullet above is
worded for it). The tests are
`the_package_cache_note_states_the_per_session_caches` (the note holds the label, `read-only`
and `CARGO_HOME`) paired with `the_package_cache_note_no_longer_grants_the_real_caches` (it holds
neither "are writable" nor "allow_write entry").

## 4. `skills/loom-plan-writer/references/sandbox.md`

Replace the paragraph that opens with the label `**Package-manager caches are pre-granted.**`
(find it by that label, not by a line number) with one headed exactly
`**Package-manager caches are per session.**` that states: every session runs on
its own cache directory, at a path stable per stage and session kind, emptied and seeded afresh
at every spawn for the ecosystems the worktree uses (by `Cargo.toml`, `go.mod`,
`bun.lock`/`bun.lockb`, `package-lock.json`, `pnpm-lock.yaml`, `uv.lock`) and removed when its
session retires (`loom/src/sandbox/session_cache/`); the operator's real caches are
read-only in every session, and a plan never grants one in `allow_write` (that grant reopens
the path by which a session poisons what the host later builds); a dependency install needs
only its registry domain in `network.allowed_domains`; out of reach: installing a Rust toolchain
(preinstall it), a cargo git dependency whose host the allow-list lacks, and a private registry
configured only in `~/.npmrc`, `~/.yarnrc.yml` or `~/.pypirc`, which sessions cannot read
(commit a project-level config instead); the stage session's first build after its contract
session recompiles Rust registry dependencies once (the two kinds have different cache paths).
End the paragraph with one more sentence: "The repository's pre-commit hook runs only in
checkout sessions; a worktree stage's commits are applied by the daemon with hooks disabled."

In the "ungrantable classes" list (find it by that name; its lines move once the paragraph above
grows) add two bullets and change the count word:
**Git writes in a worktree stage** (stage, contract and adjudication sessions cannot write the
git directory: `git add`, `git commit`, `git stash`, anything writing refs, objects or the
index; a criterion that writes git is not an acceptance criterion) and **Other stage
worktrees** (a worktree session cannot read `.worktrees/<other>`; the main checkout stays
readable). Every fenced block keeps a language tag; no other section changes.

## Traps

- `jq` failing on a capsule must not block the tool call by itself: an unparsable capsule
  contributes nothing, like an unparsable settings file today.
- Do not follow the capsule path through `canonical_existing` before validating the session id.
- Keep `set -euo pipefail` semantics: every `jq` and `canonical_existing` call that may fail
  already carries `|| true`; mirror that.
- The hook runs outside the sandbox with the session's environment, so `LOOM_WORK_DIR` and
  `LOOM_SESSION_ID` are the wrapper's; nothing in the session can change them.

## Check (once)

`bash loom-hooks/tests/credential-guard-deny-read.sh` from the repository root (it does not
need the crate). Report its PASS/FAIL lines.
