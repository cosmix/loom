# W3: embed and install the `loom-ui` plugin

Tier: sonnet. Load the `loom-rust` and `loom-ci-cd` skills. Read `common.md` first (shell commands run from `loom/`).

`install.sh` and `loom update` both end in `loom install-assets` (`install.sh` line 364; `loom/src/commands/self_update/mod.rs` `run_asset_install`, which re-executes the NEW binary, also on the "Assets refreshed" path when no new version exists), and that command ends in `crate::assets::install::install_all` (`loom/src/assets/install.rs`). One new step there covers install and update. Claude Code auto-loads and enables a plugin folder at `<claude-dir>/skills/<name>/` (common.md), so placing the folder is the whole enable step: no settings.json write, no `claude plugin` call. Loom never writes `~/.claude/settings.json` (`loom/src/commands/repair/sandbox_settings.rs:201`); keep it so.

## Files you own

- `loom/build/assets.rs`
- `loom/src/assets/mod.rs`
- `loom/src/assets/install.rs`
- `loom/src/assets/install/claude.rs`
- `loom/src/assets/install/placement.rs`
- `loom/src/assets/install/placement_plugin_tests.rs` (new)
- `loom/src/commands/install_assets.rs`
- `install.sh`
- `.github/workflows/ci.yml`
- `.gitignore`
- `scripts/smoke-loom-ui-install.sh` (new)

Read-only: everything under `claude-plugins/`. You run in the second wave, after W1 and W2 have both returned, so the plugin folder is complete and W1's code compiles when you build; if `claude-plugins/loom-ui/.claude-plugin/plugin.json` is missing, stop and report it rather than creating files there.

## 1. Embed (`loom/build/assets.rs`, 307 lines)

- `generate_embedded_assets` (lines 53-102 at planning time) is exactly 50 lines, `cargo test --test maintainability` scans `build/` with a 50-line function limit, and the baseline ledger may not grow. Net growth there must be zero: FIRST move its two `emit_scalar` calls (`CLAUDE_MD_TEMPLATE`, `AGENTS_MD_TEMPLATE`) into `fn emit_templates(output: &mut String, repo_root: &Path)` and call that (frees about 9 lines); THEN add the `claude_plugins` path and one `emit_group(&mut generated, "CLAUDE_PLUGINS", &claude_plugins, plugin_files(&claude_plugins), KeyShape::Nested)` call. Keep every new function at 50 lines or fewer; the file lands near 370 of 400.
- Add `const CLAUDE_PLUGINS_ROOT: &str = "claude-plugins";` and put it in `ASSET_ROOTS`, bumping its type from `[&str; 6]` to `[&str; 7]` (the `ASSET_ROOTS` loop adds the rerun-if-changed watch and panics if the root is missing; W2's folder exists by then). Update the doc comments that name the root count or list: the file header, "The six asset roots" above `ASSET_ROOTS`, and the `KeyShape` doc if it lists groups.
- The new group `CLAUDE_PLUGINS` uses `KeyShape::Nested` (key `<plugin>/<relpath>`, e.g. `loom-ui/.claude-plugin/plugin.json`) from a new walker `plugin_files(root)`. The existing `walk_files` cannot be reused as is: `is_skipped` (line 228) drops every dot-prefixed name, which would drop `.claude-plugin/` and leave a folder Claude Code never loads. The walker's rules, applied per plugin directory directly under `claude-plugins/`:
  - include a directory named exactly `.claude-plugin` directly under the plugin root, but not its `types/` subdirectory (Claude Code may write `.claude-plugin/types/` into a plugin folder it loads; documented for the mods folder and `--plugin-dir`, unverified for skills-dir, so never embed it);
  - skip every other dot-prefixed file or directory, and `__pycache__`;
  - skip any directory named `fixtures` and any file whose name ends `.test.ts` or `.test.tsx` (test-only; the runtime module never imports them);
  - follow no symlinks (like `walk_files`, use `entry.file_type()`);
  - every included file passes `validate_utf8` (the tables are `include_str!`).
- Sorted rows, exactly like the other groups.

## 2. Table tests (`loom/src/assets/mod.rs`)

- Add `CLAUDE_PLUGINS` to `groups()`, bumping its type from `[(&str, &[Asset]); 4]` to `5`, and to the test module's `use super::{...}` list, so `all_asset_groups_are_non_empty` and `asset_keys_are_clean_sorted_and_unique` cover it.
- New test in `tests_table`: `CLAUDE_PLUGINS` contains `loom-ui/.claude-plugin/plugin.json` and `loom-ui/hooks/hooks.json`; no key contains `.test.ts`, `/fixtures/` or `.claude-plugin/types/`; and every dot-prefixed path component of every key equals `.claude-plugin` and sits at component index 1 (`assert_clean_keys` rejects only `__pycache__`, `\`, a leading `/` and `.`/`..` components, so a leaked `.DS_Store` would pass it).

## 3. Placement (`placement.rs`, `claude.rs`, `install.rs`)

- `placement.rs`: add `pub(in crate::assets) fn place_plugin(name: &str, assets: &[Asset], skills_dir: &Path) -> Result<()>`. It follows `place_skill` (lines 29-49) for the destination `skills_dir.join(name)`: a symlink or file there is unlinked and replaced by a real directory (never written through: the folder is pruned, so following a link would delete files outside it). Do NOT call `write_skill`: it ends in `fs::write(&dest, ..)`, which writes through a symlink sitting at `dest` itself. For each table file, first `clear_non_directory_ancestors(root, parent)` and `create_dir_all(parent)` as `write_skill` does (ancestor directories); then inspect the leaf with `symlink_metadata(dest)`: only a regular file is compared (read it with `crate::fs::safe_read::open_regular_no_follow`, never `fs::read`, which follows a link) and skipped when its bytes equal the embedded content; anything else at `dest` (a symlink, even one whose target holds identical bytes, a directory, a fifo) is removed with `remove_any`. Then write through a temp file in the same directory plus `rename`, which replaces the directory entry and never follows a link that appeared after the check. Only changed regular files are rewritten (as `claude.rs` `write_assets` skips identical content): running sessions hot-reload a watched skills-dir plugin, so rewriting unchanged files on every `install.sh`, `dev-install.sh` or `loom update` would reload every open session, and a truncate-then-write could expose a half-written `hooks.json`. Then it prunes, keeping every table path AND leaving the `.claude-plugin/types/` subtree untouched. `place_skill` never calls `prune_dir` directly: the chain is `place_skill` → `prune_skill` → `prune_dir` (recursive). Thread `skip: Option<&Path>` through `prune_skill` and `prune_dir` (five edits: `place_skill`'s call passes `None`, the `prune_skill` definition, its call to `prune_dir`, the `prune_dir` definition, its recursion); compare the entry's path relative to the plugin root with `skip` BEFORE the `is_dir` branch, so `.claude-plugin/types` and its contents are neither recursed into nor removed. There is no "other root" for a plugin.
- `claude.rs`: add `pub(super) fn install_plugins(claude_dir: &Path, assets: &[Asset]) -> Result<usize>`: for each name in `skill_names(assets)` (the first key component), `place_plugin(name, assets, &claude_dir.join("skills"))`; return the count.
- `install.rs`: `InstallReport` gains `pub plugins: usize` (constructed only in `install_all`'s struct literal; add the field there). In `install_all`, right after `claude::write_layout(&paths.claude_dir, layout)?;` and before the skill index runs, add `let plugins = claude::install_plugins(&paths.claude_dir, crate::assets::CLAUDE_PLUGINS)?;` on one line (it fits rustfmt's width; the stage wiring check greps `claude::install_plugins(`). Placing it after the skill, codex and doctrine steps means a plugin placement error cannot abort them. The file is at 379 lines; stay under 400.
- Plugin placement ignores `--skills core|all` (that flag picks resident versus catalogued skills, `loom/src/cli/types.rs` `SkillLayout`): the plugin always lands in `skills/loom-ui/`.
- Unit tests in `placement_plugin_tests.rs`, declared from `placement.rs` with `#[cfg(test)] #[path = "placement_plugin_tests.rs"] mod plugin_tests;` (`loom/src/assets/tests.rs` is at the 400-line cap; do not add to it): stale file pruned; `.claude-plugin/types/claude-code/index.d.ts` kept; a symlinked plugin directory replaced without touching the link target; a regular file at the plugin path replaced; a symlink at `hooks/register.tsx` whose outside target holds the identical embedded bytes is replaced by a regular file (skip-if-equal must not keep the link), and one whose target holds other bytes leaves the target unchanged (the frozen contract `install-replaces-symlinked-plugin-leaf` in `loom/tests/loom_ui_install_contracts.rs` pins the second case end to end; that file and `loom/tests/loom_ui_contracts.rs` are read-only for you); a second install is byte-identical AND leaves every file's modification time unchanged.

The `skills/` root is shared with skills: `claude::install_skills` touches only the names in the `SKILLS` table, `skill_index` skips a directory with no `SKILL.md` (`loom/src/commands/skill_index.rs:62-65`, also `skills/index.rs` and `skills/index_catalog.rs`), and `loom repair`'s old-skill check skips `loom-*` names (`loom/src/commands/repair/home_assets.rs:95`). All three were confirmed while planning; record a one-line confirmation with `loom memory note` after your change.

## 4. Reporting

- `loom/src/commands/install_assets.rs` `print_summary`: after the Claude skills line, `✓ Claude Code plugins: <n> (loom-ui, auto-loaded from skills/)`. The smoke script greps `Claude Code plugins: 1 (loom-ui`.
- `install.sh`: `confirm_overwrites` adds `[[ -d "$CLAUDE_DIR/skills/loom-ui" ]] && found+=("$CLAUDE_DIR/skills/loom-ui/ (Claude Code plugin)")`; `print_summary` adds a line under `skills/` reading `skills/loom-ui/ Claude Code plugin (stage and subagent views)` in the same style. Keep tab indentation; the file is 370 lines. Use `$CLAUDE_DIR`, never `$HOME/.claude` (`loom/src/fs/permissions/tests/constants_tests/install_roots.rs` checks for that leak), and keep exactly one `install-assets --skills` line (`constants_tests.rs` pins it). The gate runs `bash -n ../install.sh`.

## 5. CI and ignore rules

- `.github/workflows/ci.yml`: add `- "claude-plugins/**"` to the `rust:` filter (the comment above it says every embedded asset root goes there; `doc/loom/knowledge/conventions/git-and-build-workflow.md` says the same). No new job: CI has no `claude` binary, so the plugin's own tests run in the stage gate and integration-verify only.
- `.gitignore`: add the line `claude-plugins/*/.claude-plugin/types/` (defensive: keeps an engine-written subtree out of git if Claude Code ever writes one into the repo copy).

## 6. Smoke script (`scripts/smoke-loom-ui-install.sh`, POSIX sh, `set -eu`)

It runs from any cwd (the gate runs `sh ../scripts/smoke-loom-ui-install.sh` from `loom/`).

1. `root=$(cd "$(dirname "$0")/.." && pwd)`; `smoke="$root/loom/target/loom-ui-smoke"`; `rm -rf "$smoke"`; create `$smoke/claude`, `$smoke/codex` and `$smoke/loom-home` (inside the worktree's ignored `target/`, so the sandbox allows it and git never sees it); write `$smoke/loom-home/config.toml` containing `[update]` then `check = false` (the integration tests' `loom_cmd` does the same: `install-assets` is not update-silent, and a release-versioned build would otherwise write `~/.loom/update-check.lock` and spawn a fetcher).
2. `out=$(LOOM_HOME="$smoke/loom-home" cargo run -q --manifest-path "$root/loom/Cargo.toml" -- install-assets --claude-dir "$smoke/claude" --codex-dir "$smoke/codex" --skills core)`. Both dirs explicit: no completion refresh and no root record (`loom/src/commands/install_assets.rs` lines 18 and 26), so nothing is written outside `$smoke`. Never set `HOME`. Fail unless `$out` contains `Claude Code plugins: 1 (loom-ui`.
3. With `plugin="$smoke/claude/skills/loom-ui"`: `[ -d "$plugin" ]`, `test -f "$plugin/.claude-plugin/plugin.json"` and `test -f "$plugin/hooks/hooks.json"`, each failing with a message (use `test -f`, never `rg`, for existence: `rg --files` skips hidden directories). Then `leaks=$(rg --files --hidden -g '*.test.ts' -g '*.test.tsx' -g '**/fixtures/**' "$plugin" || true)` and fail with the list unless it is empty (`rg` exits 1 when nothing matches, which would abort `set -eu` on the success path; `--hidden` is needed to see inside `.claude-plugin/`; an explicitly named directory under the gitignored `target/` is searched without `--no-ignore`). Fail if `$smoke/claude/settings.json` exists (the plan's non-goal: no settings.json write).
4. `CLAUDE_CONFIG_DIR="$smoke/claude" claude plugin validate --strict "$plugin"` (passes on the stripped copy with no tests or fixtures; never run `claude plugin test` on the installed copy, which exits 1 with no tests). The config dir keeps `claude` from rewriting the operator's `~/.claude.json`.
5. `CLAUDE_CONFIG_DIR="$smoke/claude" claude plugin list --json | jq -e 'any(.[]; .id == "loom-ui@skills-dir" and .enabled == true)'` (proves Claude Code loads and enables the installed folder; offline; a folder with an unparsable manifest lists with `enabled: false`, so this catches a broken install).
6. Print `loom-ui install smoke ok`.

## Your one check

From `loom/`: `cargo test --lib assets::`

## Memory

Record decisions and surprises with `loom memory note` / `loom memory decision` as they happen; never Claude Code auto-memory.
