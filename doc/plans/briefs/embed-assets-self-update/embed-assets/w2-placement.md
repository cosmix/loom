# W2 — Asset placement into `~/.claude` and `~/.codex`

Tier: codex `gpt-5.6-terra`, effort `xhigh`.

## Files you own (write)

- `loom/src/assets/install.rs` — new file, the module ROOT. It stays under 400 lines and keeps
  the call sites the plan's wiring checks pin (see "Module layout" below); implementation detail
  goes into `loom/src/assets/install/claude.rs` and `loom/src/assets/install/codex.rs` (sibling
  layout — never `install/mod.rs`, which deletes the pinned path).
- `loom/src/assets/tests.rs` — new file.
- `loom/src/skills/mod.rs` — re-export three existing names (below).
- `loom/src/commands/skill_index.rs` (396 lines, 4 of headroom, no ledger entry) and its
  existing `#[path]` child `loom/src/commands/skill_index/tests.rs` — rename the entry point in
  place, add no net lines to the parent.
- `loom/src/completions/install.rs` (372 lines with its test module INLINE at `:356`) and a new
  `loom/src/completions/install/tests.rs` — move the inline module out FIRST via
  `#[cfg(test)] #[path = "install/tests.rs"] mod tests;`, then add the new entry point. The
  ledgered `fn install` (exactly 52 lines) must not be reflowed.

**Module layout — pinned.** The plan's artifact entry and five wiring checks read the exact path
`loom/src/assets/install.rs` and grep it for `install_loom_hooks_to(`,
`crate::assets::AGENTS_MD_TEMPLATE`, `crate::assets::CODEX_SKILLS`,
`skill_index::execute_in_claude_dir(` and `completions::install::refresh_existing_in(`. Those five
call sites live in `install.rs` itself; the submodules hold the per-tree placement helpers they
call. Integration-verify replays every stage's wiring, so a layout that moves them fails twice.

Read-only: `loom/src/skills/install_layout.rs`, `loom/src/skills/index_catalog.rs`,
`loom/src/fs/permissions/hooks.rs`.

## Entry points

- `install_loom_hooks_to(hooks_dir: &Path) -> Result<usize>` in
  `loom/src/fs/permissions/hooks.rs` (re-exported from `crate::fs::permissions`). It is
  content-aware: it skips a hook already current and returns the number written.
- `SkillLayout::read(claude_dir) -> SkillLayout` in `loom/src/skills/install_layout.rs`.
  `SkillLayout` is `Core` or `All` and derives `PartialEq, Eq, Debug, Clone, Copy`; `read` falls
  back to inferring the layout from the filesystem when `loom-install.toml` is missing or
  malformed — and on a FRESH tree (no toml, no catalog directory) that inference is `All`. The
  CLI layer (another worker) turns a fresh tree into `Some(Core)` before calling you; you take
  `layout` as given and call `read` only when it is `None`. Do NOT call `apply_install_layout`
  — it moves any `loom-*` directory regardless of ownership and is deleted next stage.
- `is_core_skill(name) -> bool` and `CATALOG_DIR_NAME` in `loom/src/skills/index_catalog.rs`.
- `execute_in_home(home: &Path, verbose: bool)` in `loom/src/commands/skill_index.rs`.
- `install_path(shell) -> Result<PathBuf>` and `install(shell)` in
  `loom/src/completions/install.rs`; the generator is `crate::completions::generator`.
- The embedded tables — `CLAUDE_AGENTS`, `CLAUDE_COMMANDS`, `SKILLS`, `CODEX_SKILLS`,
  `CLAUDE_MD_TEMPLATE`, `AGENTS_MD_TEMPLATE`, and `pub type Asset = (&'static str, &'static str)`
  — come from `crate::assets`, written by another worker in this stage. Their exact shape is in
  the plan's Design section; code against it.

## Public surface to implement in `assets/install.rs`

```rust
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InstallPaths {
    pub claude_dir: PathBuf,
    pub codex_dir: PathBuf,
}

pub struct InstallReport {
    pub agents: usize,
    pub commands: usize,
    pub hooks: usize,
    pub skills_resident: usize,
    pub skills_catalogued: usize,
    pub codex_skills_resident: usize,
    pub codex_skills_catalogued: usize,
    pub backups: Vec<PathBuf>,
    pub layout: SkillLayout,
}

/// Place every embedded asset. `layout` overrides the recorded one when `Some`.
/// `refresh_completions` is the caller's decision (true only when the operator
/// passed no directory flag), because completion files live outside both trees.
pub fn install_all(
    paths: &InstallPaths,
    layout: Option<SkillLayout>,
    refresh_completions: bool,
) -> Result<InstallReport>;

/// `~/.claude` and `~/.codex`, from `dirs::home_dir()`.
pub fn default_paths() -> Result<InstallPaths>;
```

## Placement rules

1. `CLAUDE_AGENTS` → `<claude>/agents/<key>`.
2. `CLAUDE_COMMANDS` → `<claude>/commands/<key>`.
3. Hooks → `install_loom_hooks_to(&claude.join("hooks/loom"))`. Do not re-implement hook writing
   and do not call the no-argument `install_loom_hooks()`.
4. Skills. Resolve `layout` (argument, else `SkillLayout::read(&claude)`). For each distinct
   top-level skill directory name in `SKILLS`: the destination root is `<claude>/skills` when the
   layout is `All` or `is_core_skill(name)`, otherwise `<claude>/loom-skill-catalog`. Before
   writing, remove any copy of that skill from the *other* root, so a skill that changed sides
   does not exist twice. Write every file of that skill, creating parent directories.
5. `<claude>/CLAUDE.md` ← the timestamp header plus `CLAUDE_MD_TEMPLATE`.
6. Codex skills. The same core/catalog split under `<codex>/skills` and
   `<codex>/loom-skill-catalog`, over the same `SKILLS` set, with two substitutions:
   `loom-skills` is taken from `CODEX_SKILLS` (the codex-flavoured loader), not from `SKILLS`; and
   `pressure`, which exists only in `CODEX_SKILLS`, is always placed resident. Everything else in
   `CODEX_SKILLS` is placed resident too.
7. `<codex>/AGENTS.md` ← the timestamp header plus `AGENTS_MD_TEMPLATE`.
8. `<claude>/loom-install.toml` ← the layout actually applied, in the file's existing format:
   a `# Managed by loom` comment line then `skills = "core"` or `skills = "all"`.
9. Under `All`, after every embedded skill has been placed resident, remove
   `<claude>/loom-skill-catalog` (and `<codex>/loom-skill-catalog`) **only if it is now empty**.
   A user's own directory left inside it keeps the catalog directory alive. There is no other
   reconciler: do not call `apply_install_layout`.
10. Every run: `skill_index::execute_in_claude_dir(&paths.claude_dir, false)` — it writes
    `<claude>/hooks/loom/skill-keywords.json`, inside the tree the caller named, so it is safe
    under a `TempDir`. Only when `refresh_completions` is true:
    `completions::install::refresh_existing_in(&home)` with `home` from `dirs::home_dir()`.

**Preservation invariant — this is a guarantee `install.sh` makes today and you inherit.** The
placer touches only what loom ships. Never remove a directory whose name is not one of the
embedded skill names — a user's own `loom-`-prefixed directory included; never remove an agent,
command or hook file that is not one of the embedded file names; never clear `<claude>/skills`,
`<claude>/agents`, `<claude>/commands`, `<codex>/skills` or either catalog directory wholesale.
The removals you do perform are rule 4's "remove the copy from the other root", scoped to the
single skill being placed by exact name, and rule 9's removal of an EMPTY catalog directory. A
user's own `~/.claude/skills/rust/`, `~/.claude/skills/my-custom/`, `~/.claude/skills/loom-mine/`,
`~/.claude/agents/my-agent.md` and `~/.claude/commands/my-cmd.md` must survive every run under
both layouts. A file loom no longer ships inside a shipped skill directory also stays — that is
the plan's recorded decision, not an omission.
(`loom/src/fs/permissions/tests/constants_tests.rs` currently proves this by sourcing `install.sh`
and calling its bash functions; those functions disappear next stage, so your tests become the
only proof.)

Header format for rules 5 and 7 — reuse the existing one from `save_with_header` in
`loom/src/commands/self_update/mod.rs`: three box-drawing rule lines with
`# claude-loom | updated <UTC %Y-%m-%d %H:%M:%S>` in the middle, then a blank line, then the
content.

**Backups, and not rewriting what is unchanged.** Compare the destination's BODY — everything
after the header block — with the template. If they are equal, do not touch the file at all (the
old header, with its old timestamp, stays; this is what makes a second run leave the tree
byte-identical, which a `diff -r` criterion checks). If they differ and the destination exists:
first delete every older `<dest>.bak.*` loom wrote, then move the current file to
`<dest>.bak.<UTC %Y%m%d-%H%M%S>`, push that path onto `InstallReport::backups`, and write the new
file. At most one backup per file ever survives; `install.sh`'s interactive `cleanup_backups` is
removed next stage, so this cap is the only thing preventing one file per update.

## Two helpers that must stop resolving `~` themselves

- `skill_index.rs`: rename `execute_in_home(home, verbose)` IN PLACE to
  `pub fn execute_in_claude_dir(claude_dir: &Path, verbose: bool) -> Result<()>` — its body
  already only needs the `.claude` directory (`home.join(".claude/skills")` becomes
  `claude_dir.join("skills")`, etc.) — and change its two callers, `execute()` and
  `execute_quiet()`, to pass `home.join(".claude")`. No wrapper, no net new lines: the file is at
  396 of 400 and has no ledger entry. `execute()` and `execute_quiet()` behave exactly as before.
- `completions/install.rs`: add
  `pub fn refresh_existing_in(home: &Path) -> Result<usize>` that, for each of bash, zsh and
  fish, resolves the same per-shell path `install_path(shell)` would resolve but rooted at
  `home` (factor the `$HOME`-relative part of `install_path` into a `home`-taking helper the two
  share), rewrites the file **only if it already exists**, and returns how many were rewritten.
  Keep a no-argument `pub fn refresh_existing() -> Result<usize>` that calls it with
  `home_dir()?`. It must not create files and must not edit `.bashrc` or `.zshrc` — that is
  `install()`'s job and stays there. Do the test-module move (see "Files you own") before adding
  any of this.

`install_all` calls `execute_in_claude_dir` on every run and `refresh_existing_in` only when
`refresh_completions` is true (the CLI passes `true` only when the operator supplied neither
directory flag). Tests always pass `false` to `install_all` and drive `refresh_existing_in`
directly against a `TempDir` home.

## `skills/mod.rs`

Re-export `index_catalog::is_core_skill`, `index_catalog::CATALOG_DIR_NAME` and
`install_layout::SkillLayout`. The module's existing comment says those names have no consumer
outside `src/skills/`; that is no longer true — update the comment to name `crate::assets::install`
rather than deleting it.

## Tests, in `assets/tests.rs`

Every test builds a `TempDir` and passes both directories explicitly. **Never** call
`default_paths()`-backed installation, `install_loom_hooks()` or `skill_index::execute()` in a
test — those resolve the operator's real home.

Four of these are pinned BY NAME by the stage's acceptance criteria (`cargo test -- --list` is
grepped for them), so the test function names below are not yours to vary.

- `core` layout: `<claude>/skills/loom-plan-writer/SKILL.md` exists,
  `<claude>/loom-skill-catalog/loom-rust/SKILL.md` exists, `<claude>/skills/loom-rust` does not,
  and a nested non-markdown file travels: `<claude>/loom-skill-catalog/loom-md-tables/fix-md-tables.py`.
- `all` layout: `<claude>/skills/loom-rust/SKILL.md` and `<codex>/skills/loom-rust/SKILL.md`
  exist, and NEITHER `<claude>/loom-skill-catalog` nor `<codex>/loom-skill-catalog` exists.
- `codex_loader_differs_from_claude_loader`: `<codex>/skills/loom-skills/SKILL.md` is
  byte-identical to the `CODEX_SKILLS` entry and **differs** from the `SKILLS` entry of the same
  name — this is the assertion that proves the substitution happened rather than the Claude
  loader being copied to codex.
- `<codex>/skills/pressure/SKILL.md` exists under both layouts.
- `<claude>/hooks/loom/post-tool-use.sh` exists and is executable (mode `0o755`).
- `<claude>/CLAUDE.md` and `<codex>/AGENTS.md` both start with the header's first line and contain
  a distinctive line from their template.
- `reinstall_is_idempotent`: re-running `install_all` over an existing tree writes no backup and
  leaves every file byte-identical, `CLAUDE.md`/`AGENTS.md` headers included.
- A changed template body (simulate by editing the installed `CLAUDE.md` body) produces exactly
  one `.bak.*`, and a third run after another change leaves still exactly one.
- `core_after_all_moves_catalogued_skill`: running with `Core` after a tree was installed with
  `All` moves `loom-rust` into the catalog and leaves no copy behind — on the codex side too.
- `all_after_core_moves_catalogued_skill_back`: the mirror image, and the empty catalog
  directory is gone afterwards on both sides.
- Other-root cleanup with a stale copy: seed `<claude>/loom-skill-catalog/loom-plan-writer/` and
  `<codex>/loom-skill-catalog/loom-skills/` before a `Core` run; both are gone afterwards.
- `<claude>/loom-install.toml` records the layout that was applied (assert the VALUE, `core` and
  `all`).
- `user_owned_assets_survive_core_and_all` (**preservation**): seed the temp tree with
  `<claude>/skills/rust/SKILL.md`, `<claude>/skills/my-custom/SKILL.md`,
  `<claude>/skills/loom-mine/SKILL.md`, `<claude>/agents/my-agent.md`,
  `<claude>/commands/my-cmd.md` and `<codex>/skills/my-codex-skill/SKILL.md` before installing;
  assert all six still exist afterwards, under `Core` and under `All`, and that a user's
  `<claude>/loom-skill-catalog/loom-mine/` keeps the catalog directory alive under `All`.

Two more, in the modules you extend rather than in `assets/tests.rs`:

- `skill_index::execute_in_claude_dir` against a `TempDir` claude directory containing one skill:
  the index lands at `<claude>/hooks/loom/skill-keywords.json` and names that skill. It goes in
  `loom/src/commands/skill_index/tests.rs`.
- `completions::install::refresh_existing_in` (the acceptance grep pins that name in the test
  list): against a `TempDir` home with no completion file present it writes nothing and returns
  0; with `<home>/.zfunc/_loom` pre-created it rewrites the content and returns 1. Both go in the
  new `loom/src/completions/install/tests.rs`. Never mutate `$HOME` in a test and never call the
  no-argument `refresh_existing()`.

## Done means

`cargo test --manifest-path loom/Cargo.toml --lib assets::`,
`--lib commands::skill_index::` and `--lib completions::` all pass.

## Constraints the graph will not show you

- Each file under 400 lines, each function under 50 — the maintainability gate is an exact ledger
  and a new entry is debt you must not add. `install.rs` is the module root and keeps the five
  pinned call sites (see "Module layout"); `install/claude.rs` and `install/codex.rs` hold the
  rest. Never edit `loom/maintainability-baseline.txt`; report any number the gate prints.
- Follow the crate's error style: `anyhow::Result` with `.with_context(|| …)` naming the path, as
  `fs/permissions/hooks.rs` does.
- Do not run `git`. Do not run the full test suite, the linter or the formatter.
