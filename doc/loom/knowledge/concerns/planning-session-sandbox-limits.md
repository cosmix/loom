# Planning Session Sandbox Limits

>

## Planning Session Sandbox Limits

Open items of [Planning Session Sandbox](../architecture/planning-session-sandbox.md). The accepted
limits are the ones `doc/product/section_12.md` lists ("Bash writes inside `doc/loom/`", "New
top-level paths", "Locked plans locked mid-session"); the implementation matches them.

### Accepted limits the implementation confirmed

- **Bash writes inside `doc/loom/`.** `loom knowledge` and `loom plan summary` need OS write access
  there, so a raw approved Bash command can write the same bytes.
- **New top-level paths.** Layer 1 denies only the top-level entries that exist at spawn
  (`filesystem.rs` `authoring`), and the root directory stays writable. An approved command can create
  a new root file; the sharpest instance is `<root>/.ignore` holding `!.loom`, which un-ignores the
  state root's `terminal-history` for a repo-root Grep (the state-root `.ignore` lists only
  `STATE_ROOT_SECRET_FILES`).
- **Locked mid-session, for Bash.** Layers 1 and 2 are built at spawn; only the guard rescans the
  locked set, and it covers file tools. An approved Bash command can write a plan locked after spawn.
- **Check-to-write symlink swap.** A symlink swapped in between the guard's decision and Claude Code's
  write passes (`guard/mod.rs` `confined_file`); creating it needs a holder-approved command.
- **Search above the state root.** A Glob rooted above the state root with an ordinary pattern
  (`**/*`) is allowed and may list state-root file names with `rg --files --glob`; reads stay denied.
  The parent rule covers only the immediate parent of a state root outside the project, so a plain
  search at a higher ancestor outside any git repository walks into it, as the shell guard does.

### Unverified or untested

- **Seatbelt.** `planning/runtime.rs` `check_parent` relies on `access(2)` reporting Seatbelt's write
  deny on the planning root; no macOS run exists.
- **Provenance race.** `check_state` and `grants_mounted` are path-based: `access(W_OK)` and
  `symlink_metadata` resolve ancestors at check time and `safe_open_dirfd` applies `O_NOFOLLOW` to the
  last component only, so a parent swapped between check and use, or a self-made parent chmodded to
  0555, passes. Harden by opening the state directory as a dirfd once and `fstatat`-ing the grants
  relative to it.
- **No gate test for `grants_mounted` accepting a state directory** (it needs real mounts); only the
  srt confinement test covers it, and that test skips in a stage's criteria runner. Keep
  `LOOM_TEST_REQUIRE_SANDBOX_FREE=1` host runs as acceptance evidence.
- **No test for the name filter on top-level `doc/` files** (`permissions.rs` `RULE_SYNTAX`, symlinks):
  a file named `*` would re-emit `Edit(//<root>/doc/*)` without failing a test. Add `doc/*` and
  `doc/link -> loom` to `no_kind_allows_an_edit_under_doc_loom`'s fixture.

### Code debt

- `sandbox/mod.rs` `write_session_sandbox` builds `guard.json` from a `json!` literal that duplicates
  `GuardPolicy` and its `SCHEMA`; serialize `GuardPolicy` directly.
- `sandbox/assert.rs` infers the kind from a `Bash(touch` allow; an authoring object carrying one
  skips the peer-tool deny check. It also accepts any hook command containing
  ` plan hook PreToolUse ` and extra `PreToolUse` entries, and checks neither `sandbox.filesystem` nor
  `network.strictAllowlist`. Pass the kind and tighten the checks.
- `sandbox/tests.rs` `expected_read_denies` duplicates `tests_kinds_fixture.rs` over a separate
  `Fixture`; consolidate. The module doc of `sandbox/tests_kinds.rs` still says the expected-value
  helpers are reused by `tests_assert`; rewording it raises a test-integrity event, because the line
  contains `tests_assert`.
- `doc/product/section_12.md` (graph-cache paragraph) and `context/graph_store/fallback.rs` say
  `loom knowledge context` refreshes the source graph cache; retrieval refreshes structurally and a
  denied write falls back to an in-memory knowledge catalog, not a graph layer. Reword: `loom map`
  refreshes the source graph and `loom knowledge context` the knowledge catalog, both under
  `.loom/cache/context-v1`.
- `doc/plans/briefs/feat-planner-06-session-sandbox/planning-sandbox/w1-settings.md` states
  `read_denies(...) -> ReadDenies`; the function returns `anyhow::Result<ReadDenies>`.
