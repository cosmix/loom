# W3 — `loom install-assets` CLI wiring and the codex skills loader

Tier: codex `gpt-5.6-luna`, effort `xhigh`.

## Files you own (write)

- `loom/src/commands/install_assets.rs` — new file.
- `loom/src/commands/mod.rs` — register the module.
- `loom/src/cli/types.rs` — the subcommand variant.
- `loom/src/cli/dispatch.rs` — the dispatch arm.
- `loom/src/lib.rs` — `pub mod assets;`.
- `codex/skills/loom-skills/SKILL.md` — new file.

You do NOT own `loom/maintainability-baseline.txt`. `cli/types.rs` is at 375 of 400 lines with no
ledger entry: the new variant is three flags with one-line doc comments and no blank-line padding.

Read-only: `skills/loom-skills/SKILL.md` (the Claude loader you are adapting),
`codex/skills/pressure/SKILL.md` (the codex skill format already in this repo).

## Entry points

- `Commands` enum in `loom/src/cli/types.rs`; the `SkillIndex` variant near line 277 is the
  smallest neighbour to copy the doc-comment style from.
- `dispatch` in `loom/src/cli/dispatch.rs`; the arm `Commands::SelfUpdate => self_update::execute()`
  is where the new arm belongs, alphabetically irrelevant — put it beside its neighbours.
- `pub fn install_all(paths: &InstallPaths, layout: Option<SkillLayout>, refresh_completions: bool) -> Result<InstallReport>`
  and `pub fn default_paths() -> Result<InstallPaths>` in `crate::assets::install`, written by
  another worker in this stage. `InstallReport` carries `agents`, `commands`, `hooks`,
  `skills_resident`, `skills_catalogued`, `codex_skills_resident`, `codex_skills_catalogued`,
  `backups: Vec<PathBuf>` and `layout`.

## The command

```rust
/// Install loom's agents, skills, commands, hooks and doctrine files
InstallAssets {
    /// Claude configuration directory (default: ~/.claude)
    #[arg(long)]
    claude_dir: Option<PathBuf>,

    /// Codex configuration directory (default: ~/.codex)
    #[arg(long)]
    codex_dir: Option<PathBuf>,

    /// Skill layout to apply (default: the layout recorded in loom-install.toml)
    #[arg(long, value_parser = ["core", "all"])]
    skills: Option<String>,
},
```

`install_assets::execute(claude_dir, codex_dir, skills)`:

1. Resolve paths — a supplied directory wins, otherwise the matching field of `default_paths()`.
   `refresh_completions = claude_dir.is_none() && codex_dir.is_none()` — completions are refreshed
   only when the operator named neither directory, because they live outside both trees.
2. Resolve the layout: an explicit `--skills` maps to `Some(SkillLayout::Core)` /
   `Some(SkillLayout::All)`; otherwise, if `<claude>/loom-install.toml` does not exist AND
   `<claude>/skills` does not exist (a fresh tree), pass `Some(SkillLayout::Core)` — the default
   `install.sh` has always applied; otherwise `None`, and the placer reads the recorded or
   inferred layout. Without the fresh-tree rule, `SkillLayout::read` infers `All` from an empty
   directory and a bare `loom install-assets` would install all 62 skills resident.
3. Call `install_all(&paths, layout, refresh_completions)`.
4. Print a summary in the house style — `crate::utils::print_logo_header` plus `"✓".green()` lines,
   the way `self_update::execute` does. Name each backup path the report returned.

Leave the existing `SelfUpdate` variant completely alone. It is renamed to `Update` by the next
stage, together with the `update_check` notice text and the tests that pin it; splitting that
rename across two stages would leave a half-renamed command in the tree.

## `codex/skills/loom-skills/SKILL.md`

Adapt `skills/loom-skills/SKILL.md` for codex. Same frontmatter keys codex reads (`name`,
`description`); drop `allowed-tools` and `triggers`, which are Claude Code concepts. Keep the
catalog table verbatim — it is the whole point of the skill.

The differences that matter:

- Paths are `~/.codex/loom-skill-catalog/<name>/SKILL.md` and `~/.codex/skills/<name>/SKILL.md`,
  not the `~/.claude/…` ones. The literal `~/.codex/loom-skill-catalog` must appear in the file:
  an acceptance criterion greps the installed codex copy for it, and `diff`s it against the
  Claude loader to prove the codex flavour was placed.
- Codex has no Read tool. Say to read the file with `cat`, or with
  `sed -n '<first>,<last>p'` for a long one.
- Keep the multi-name behaviour (`loom-ci-cd loom-rust` loads both, never one path with a space in
  it) and the "a single unresolved name means a bad name, not a broken install" rule.
- `name:` must be `loom-skills`; the description should say the argument is the full skill name.

Every fenced block needs a language tag, and the file must pass
`bunx markdownlint-cli2@0.23.2 codex/skills/loom-skills/SKILL.md`.

## The ledger is not yours

`loom/maintainability-baseline.txt` records `function src/cli/dispatch.rs dispatch 119`. Your new
match arm grows that function; the orchestrator sets the new exact value after every worker has
landed. Do not edit the file. Report the number if you happen to see the gate print it.

## Done means

- `cargo build --manifest-path loom/Cargo.toml` succeeds and
  `loom/target/debug/loom install-assets --help` exits 0 — once the other workers' files exist.
  The crate cannot compile until W1's asset table, W2's `install_all` and W4's
  `AGENTS.md.template` are all present; if your slice is finished before theirs, say so in your
  report rather than stubbing their files.

## Constraints the graph will not show you

- **Never run `loom install-assets` without both `--claude-dir` and `--codex-dir` pointing under
  `$TMPDIR`.** The bare form writes the operator's real `~/.claude` and `~/.codex`.
- Do not run `git`. Do not run the full test suite, the linter or the formatter.
