# WB4 — Retarget the `install.sh` drift tests at the delegation

Tier: codex `gpt-5.6-terra`, effort `xhigh`.

## File you own (write)

- `loom/src/fs/permissions/tests/constants_tests.rs` — nothing else.

Read-only: `install.sh` (being rewritten by another worker in this same stage — code against the
contract below, not against the file's current text), `loom/src/assets/tests.rs` (merged in the
previous stage; it now holds the preservation coverage).

## Entry points

- `const INSTALL_SH: &str = include_str!("../../../../../install.sh")` at line 13.
- The doc comment above `install_sh_hook_arrays()` (around line 74) listing the **five**
  registration sites a hook script needs.
- `fn install_sh_hook_arrays()` and `#[test] fn install_sh_hook_arrays_match_loom_hooks_exactly()`.
- `#[test] fn installer_does_not_delete_legacy_unprefixed_names()`.
- `#[test] fn local_skill_install_preserves_bare_rust_and_custom_skill()` and
  `#[test] fn local_agent_install_preserves_bare_and_custom_agents()` — both source `install.sh`
  with `LOOM_INSTALL_LIB_ONLY=1` and call bash functions that no longer exist after this stage.

## The contract you are testing against

After this stage `install.sh` performs asset placement with **exactly one** invocation, written
literally as:

```bash
"$loom_bin" install-assets --skills "$SKILLS_MODE"
```

and carries no per-asset copy loop and no `all_hooks` array. It keeps `parse_args` with
`--skills core|all`, its validation of that flag, and the
`if [[ "${LOOM_INSTALL_LIB_ONLY:-0}" != "1" ]]; then main "$@"; fi` guard at the bottom.

## What to change

### 1. Delete what no longer has a subject

Remove `install_sh_hook_arrays`, `install_sh_hook_arrays_match_loom_hooks_exactly`,
`installer_does_not_delete_legacy_unprefixed_names`,
`local_skill_install_preserves_bare_rust_and_custom_skill` and
`local_agent_install_preserves_bare_and_custom_agents`. The bash functions all five reach into are
gone, and a test whose subject no longer exists passes for the wrong reason.

The preservation guarantees those last three encoded are not being dropped — they moved to
`assets/install.rs` and are covered by `loom/src/assets/tests.rs`. Say so in a comment where the
tests used to be, naming that file, so the next reader can find the coverage.

### 2. Add the delegation tests

- `install_sh_delegates_asset_placement_to_the_binary`: `INSTALL_SH` contains the exact string
  `install-assets --skills`, exactly once. Count the matches — "contains" would pass with the line
  duplicated in a comment.
- `install_sh_carries_no_per_asset_copy_loops`: `INSTALL_SH` contains none of `all_hooks`,
  `agents.zip`, `skills.zip`, `download_and_extract_zip`, `update_completions`, `cleanup_backups`.
  Assert each separately, with a message naming which one reappeared, so a failure says what
  happened.
- `install_sh_still_validates_the_skills_flag`: source the script the way the deleted tests did —
  `LOOM_INSTALL_LIB_ONLY=1`, writing `INSTALL_SH` to a `TempDir` and running bash against it — and
  assert `parse_args --skills bogus` fails while `parse_args --skills all` succeeds and leaves
  `SKILLS_MODE=all`. This keeps the `LOOM_INSTALL_LIB_ONLY` seam live and keeps the one piece of
  bash logic that survives under test. Follow the exact bash-invocation pattern of the tests you
  are deleting, including how they materialise the script and pass its path.
- `install_sh_invokes_the_binary_exactly_once_with_the_resolved_mode` — the behavioural test, and
  the one the stage's acceptance greps the test list for by name. The three static tests above
  cannot tell "the string is in the file" from "the installer calls the binary": a single
  invocation in an unreachable branch, an unset `$loom_bin`, or placement before the binary
  exists all pass a grep. So drive `main` through the sourcing seam with a stub binary:
  - write `INSTALL_SH` to a `TempDir`; create `<tmp>/home/.local/bin/loom` as an executable stub
    script that appends `"$@"` as one line to `$LOOM_STUB_ARGV_LOG` and exits 0 for every
    invocation (including `install-assets --help`);
  - run `bash` with `HOME=<tmp>/home`, `LOOM_INSTALL_LIB_ONLY=1`, `LOOM_STUB_ARGV_LOG=<tmp>/argv`
    and a script body that sources the installer, then overrides `install_loom_local()` and
    `install_loom_remote()` with functions that do nothing (the stub already sits at
    `$HOME/.local/bin/loom`), overrides `confirm_overwrites()` to return 0, and finally calls
    `main --skills core`;
  - assert the argv log's LAST line is exactly `install-assets --skills core`, that exactly one
    line matches `install-assets --skills`, and that `<tmp>/home/.claude` and `<tmp>/home/.codex`
    are absent or empty (the stub places nothing, so anything there came from bash). Never let the
    real `~/.local/bin/loom` or the real `$HOME` into this test.

The checklist above `install_sh_hook_arrays` says a hook has up to **five** registration sites, two
of them `install.sh` arrays. Those two are gone: hooks now reach a machine only through
`LOOM_HOOKS` and the installer that reads it. Rewrite the comment for the remaining three sites —
the `HOOK_*` `include_str!` constant, the `LOOM_HOOKS` row naming it, and the event registration
(`fs/permissions/hooks/config.rs`'s `pre_tool_hooks()` for a global PreToolUse hook, or
`hooks/config.rs`'s `HookEvent` enum plus `all()` for a per-session one; a sourced library like
`_common.sh` needs no third site). Keep the pointers to which test pins which site accurate:
`loom_hooks_config_only_names_embedded_hooks` and
`hooks_tests.rs::test_hook_event_scripts_are_all_embedded` are unchanged and still pin site 3.

## Done means

`cargo test --manifest-path loom/Cargo.toml --lib fs::permissions::` passes, and the file stays
under 400 lines.

## Constraints the graph will not show you

- **Never execute `install.sh`'s `main`, and never run `loom install-assets` without both
  `--claude-dir` and `--codex-dir` under `$TMPDIR`.** Both write the operator's real `~/.claude`
  and `~/.codex`. The sourcing test must set `LOOM_INSTALL_LIB_ONLY=1` so `main` never runs.
- Your worker writes `install.sh` in parallel with you. Do not edit it, and do not read its
  in-progress state to decide what to assert — assert the contract above.
- Do not run `git` at all. Do not run the full test suite, the linter or the formatter.
