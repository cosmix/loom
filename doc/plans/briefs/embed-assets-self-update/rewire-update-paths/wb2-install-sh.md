# WB2 — `install.sh` delegates asset placement to the binary

Tier: codex `gpt-5.6-luna`, effort `xhigh`.

## File you own (write)

- `install.sh` — nothing else.

Read-only: `loom/src/commands/install_assets.rs` (merged in the previous stage — read it for the
flags), `dev-install.sh` (calls `install.sh`; it needs no change, confirm that).

## Entry points in the current script

`main()` at the bottom drives everything. The functions to remove are `install_agents`,
`install_skills`, `install_commands`, `install_codex_skill`, `install_claude_md`, `install_hooks`,
and their curl-pipe twins `install_agents_remote`, `install_skills_remote`,
`install_claude_md_remote`, `install_hooks_remote`, plus `install_skills_from_source`,
`load_core_skills`, `is_core_skill`, `build_skill_index`, `write_install_config`,
`download_and_extract_zip`, `check_dependencies`, `cleanup_backups` and `update_completions`.
Everything they did is now done by one command (the binary refreshes existing completion files
itself when run without directory flags, and caps its own backups; an acceptance criterion
asserts none of these names survive in the file).

**`loom_bin` does not exist at `main()` scope today.** It is a `local` in `install_loom_local`
(`:627`), `install_loom_remote` (`:654`) and `update_completions` (`:829`), and the script runs
under `set -euo pipefail`, so the delegation line would abort with `loom_bin: unbound variable`.
`main()` declares `loom_bin="$HOME/.local/bin/loom"` immediately after the binary-install step.

## The contract

Asset placement happens through **exactly one** invocation, written literally as:

```bash
"$loom_bin" install-assets --skills "$SKILLS_MODE"
```

Another worker in this stage writes a test that pins that line, so the wording is not yours to
vary. After it runs, `install.sh` must contain no per-asset copy loop and no `all_hooks` array.

## What `install.sh` keeps

- `print_banner`, `print_components`, `print_usage`, and the `step`/`ok`/`warn`/`err`/`info`
  output helpers.
- `parse_args` with `--skills core|all`, `read_recorded_skills_mode`, and the validation that
  rejects anything else. The mode is still resolved in bash, because it is a user-facing flag; it
  is then handed to the binary, which records it in `loom-install.toml`.
- `is_curl_pipe`, `download_file`, `install_loom_local` and `install_loom_remote` — getting the
  binary in place is still the script's job, and it is the step that must happen first.
- `confirm_overwrites`, extended: the list of things that may be updated now also names
  `~/.claude/commands`, `~/.claude/hooks/loom`, `~/.codex/AGENTS.md` and `~/.codex/skills`.
- `check_requirements` for the local (non-curl-pipe) path, trimmed to what still matters: the
  script no longer copies `agents/`, `skills/`, `commands/` or `codex/`, so it should assert only
  what it still uses. `unzip` is no longer required at all.
- `print_summary`.

`print_summary`'s counts came from the removed functions. Replace them with what the binary
prints, or reduce the summary to the destination tree without per-directory counts — do not invent
numbers in bash that the binary already reports.

## Shape of `main()` afterwards

```text
parse_args → banner → (curl-pipe? download binary : check requirements, confirm, build/copy binary)
→ loom_bin="$HOME/.local/bin/loom"; [[ -x "$loom_bin" ]] || err …
→ "$loom_bin" install-assets --help >/dev/null 2>&1 || err "… does not support install-assets …"
→ "$loom_bin" install-assets --skills "$SKILLS_MODE" → print_summary
```

The curl-pipe and local paths converge the moment the binary exists. `$loom_bin` is
`$HOME/.local/bin/loom`, the path both `install_loom_local` and `install_loom_remote` already
write to — declare it in `main()`; it is not visible there today. If that file is missing or not
executable after the install step, fail loudly with `err` and exit non-zero — a silent skip here
is how an install "succeeds" with nothing placed. The `--help` probe exists because the curl-pipe
path downloads `releases/latest`, which has no `install-assets` subcommand until a release
carrying this plan is published; without the probe `set -e` aborts on clap's usage error after
the binary is already in place. The `err` message names the version skew.

One more behaviour to state rather than hide: `install_loom_local` falls back to a remote download
when `loom/target/release/loom` is absent, so `./install.sh` run from a checkout that has not
built a release binary installs the RELEASED binary's embedded assets, not the checkout's. That is
self-consistent (binary and assets from one release) and `dev-install.sh` builds `--release` first
so it never hits it; keep the fallback, and print one `info` line in that branch saying the assets
will come from the downloaded release.

## Bash rules this script already follows

- `set -euo pipefail` at the top stays.
- macOS ships bash 3.2: under `set -u` a bare `"${a[@]}"` on an empty array aborts, so the file
  uses the `${a[@]+"${a[@]}"}` guard. Keep that idiom wherever an array survives.
- The script must parse under `bash -n` and under `./scripts/check-hook-syntax.sh`, which parses
  every shell file in the repository.

## Done means

- `bash -n install.sh` exits 0.
- `rg -qF "install-assets --skills" install.sh` succeeds, and the string appears exactly once.
- `rg -q "all_hooks|update_completions|cleanup_backups|download_and_extract_zip" install.sh` finds
  nothing, and `rg -q "LOOM_INSTALL_LIB_ONLY" install.sh` still succeeds.

## Constraints the graph will not show you

- **Do not execute `install.sh`, and do not run `loom install-assets` yourself.** Both write the
  operator's real `~/.claude` and `~/.codex`. Your verification is `bash -n` only.
- Do not run `git` at all.
