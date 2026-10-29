# W7: a test that every doctrine `loom` command exists

Wave 2: you start after W2, W3 and C1 have finished. Read `doc/plans/token-optim/briefs/doctrine-truth-and-plan-guards/doctrine-and-guards/common.md` first.

## Files you own

- `src/plan/schema/validation/v2_lints/loom_subcommands.rs`
- `src/orchestrator/signals/tests_doctrine_commands.rs` (new)
- `src/orchestrator/signals/mod.rs` (one test-module declaration)

C1 (a codex unit) has already made the module public to the crate: `pub(crate) mod loom_subcommands;` in `v2_lints/mod.rs`, which now also registers `bookends`. Do not edit `v2_lints/mod.rs`.

## Why

The plan lint `loom_subcommands::check` resolves `loom` commands in plan criteria against the real CLI tree (`crate::cli::Cli::command()`). Nothing checks the commands that doctrine surfaces teach agents, so a renamed or removed subcommand stays in a skill or the template until an agent runs it and pays a correction turn.

## 1. A text entry point in `loom_subcommands.rs`

`unknown_subcommand(root, argv)` (about line 60, private) walks the leading non-flag words of a `loom` argv down the CLI tree and returns a message for an unknown word or for a parent that requires a subcommand and gets none. Add, without changing the lint's behaviour or messages (the tests in `src/plan/schema/tests/v2_lint_tests.rs`, `unknown_loom_subcommand_is_error_in_v2`, pin them):

```rust
/// What `loom_command_paths` found in a script.
#[cfg(test)]
pub(crate) struct LoomCommandPaths {
    /// How many `loom` invocations the script holds.
    pub(crate) checked: usize,
    /// Each invocation's path up to and including a word no subcommand
    /// answers to, as `loom <word> ...`.
    pub(crate) unresolved: Vec<String>,
}

#[cfg(test)]
pub(crate) fn loom_command_paths(script: &str) -> LoomCommandPaths
```

Both items carry `#[cfg(test)]`, as `src/verify/criteria/confine.rs:20-21` does for `host_launcher`: their only caller is the test module below, so without the gate the library build warns that they are never used, and `cargo clippy --all-targets -- -D warnings` fails (your own check builds only the test profile and never sees that warning). The same holds for the shared walk: every variant and field of its outcome enum must be used on the lint path that the non-test build compiles; a variant or field only the test-gated function reads makes the library build warn.

- Lex `script` with the module tree's `visit_argvs(script, 0, &mut |argv| ...)` (`v2_lints/mod.rs`, `pub(super)`), which yields the argv of every simple command, past assignments, nested `sh -c` scripts and substitutions included.
- Normalise the command word before the walk: drop a leading `loom_run_bounded <n>` pair; treat a first word whose value is `${LOOM_BIN:-loom}`, `$LOOM_BIN` or `${LOOM_BIN}` as `loom`. An argv whose (normalised) command word is not `loom` is skipped and not counted.
- Walk as `unknown_subcommand` does, and additionally stop, clean, at a word containing `*`, `[`, `{`, `<` or `…`, or equal to `...` (doc placeholders).
- Report only unknown words. A family named alone (`loom knowledge`, a parent that requires a subcommand and gets none) is accepted: prose names a family that way.
- Share the walk between the lint and this function (a private helper returning an enum for the two outcomes is enough). The lint's two message texts stay byte-identical.

## 2. The doctrine test

New `src/orchestrator/signals/tests_doctrine_commands.rs` (`tests_doctrine.rs` is at 399 lines), declared in `src/orchestrator/signals/mod.rs` directly after `tests_doctrine_waiting`, in the same three-line form (`#[cfg(test)]`, `#[path = "tests_doctrine_commands.rs"]`, `mod tests_doctrine_commands;`). It reads the surfaces at run time from the repository root (`Path::new(env!("CARGO_MANIFEST_DIR")).join("..")`), groups them, and for every group:

| Group | Files | How candidates are taken | Minimum `checked` |
| --- | --- | --- | --- |
| templates | `CLAUDE.md.template`, `AGENTS.md.template` | markdown rule | 20 |
| commands | `commands/*.md` | markdown rule | 5 |
| skills | every `*.md` under each `skills/loom-*` directory, recursively | markdown rule | 150 |
| hooks | every `loom-hooks/*.sh` | hook rule | 10 |

Hook rule: the whole file is one script (the lexer drops comments and heredoc bodies: `shell_lex.rs`, `skip_heredoc_bodies`), plus, as separate candidates, the guidance lines a hook prints: every line whose text, after leading whitespace and box-drawing characters (`│`), starts with `loom` (a word boundary after `loom`), with a trailing `│` and trailing whitespace removed. Those lines sit in heredoc bodies the lexer never sees (`post-tool-use.sh`'s post-commit box, `learning-validator.sh`'s box, `stage-terminal-guard.sh`, `codex-forward.sh`). Write the extractor as a small function in the test file (`hook_guidance_lines(text: &str) -> Vec<String>`). At `4cc085b0` it yields about ten lines, all resolvable, and W2 rewrites the post-commit box in wave 1.

Markdown rule: every line inside a fenced block whose info string is `bash`, `sh`, `shell`, `zsh` or `console`, with a leading `$` stripped, plus the text of each inline code span (between single backticks) on a line outside any fence whose trimmed text starts with `loom` (a word boundary after `loom`). Spans that start otherwise are skipped: `skills/loom-plan-writer/SKILL.md:121` carries the span `<!-- loom METADATA -->`, whose `<` the lexer reads as a redirect, leaving the argv `loom METADATA`. A fence opens on a line whose trimmed text starts with three or more backticks and closes on the next line that starts with at least as many (CommonMark: a block opened by four backticks keeps its inner three-backtick lines as content). Lines inside fences of any other language are skipped: `text` fences hold prose, and `skills/loom-orchestration/SKILL.md` (the coordinator preamble, a `text` fence) carries `(loom caps the tree at 2 levels)`, which the lexer reads as a subshell running `loom caps`. Feed each candidate to `loom_command_paths` and sum `checked` per group.

Assertions:

- every group's `checked` is at least its minimum (a port of the lexer under these rules counted 35 invocations in the templates, 9 in `commands/*.md`, 340 in the skills and 23 `${LOOM_BIN:-loom}` invocations on code lines of the hooks at `4cc085b0`, before the hook guidance lines; a count under the minimum means the extractor misses a form, so fix the extractor and keep the minimum);
- no group has an unresolved path; the failure message lists each as `<file>: <path>`;
- a self-check in the same file: `loom_command_paths("loom knowledg update mistakes x").unresolved` is non-empty, `loom_command_paths("\"${LOOM_BIN:-loom}\" hook nope").unresolved` is non-empty, `loom_command_paths("loom knowledge update mistakes x")` has `checked == 1` and no unresolved path, and `loom_command_paths("loom knowledge")` has no unresolved path;
- a guidance self-check on the fixture `cat <<'EOF'\n│  loom memroy note "x"   │\nEOF\n`: `loom_command_paths` on the whole fixture has `checked == 0` (the lexer skips the heredoc body), and the one line `hook_guidance_lines` returns has a non-empty `unresolved`.

Keep every function under 50 lines and the file under 400.

## When the test finds a real unresolved command

At `4cc085b0`, under the rules above, one path failed: `skills/loom-usage/SKILL.md:713`, `loom stage hold/release <id>` in a `bash` fence, which W3 splits into `loom stage hold` and `loom stage release` in wave 1. `loom stage dispute-*` in `AGENTS.md.template` stops clean at `*`. If your test reports one, do not edit the surface: report the file, the line and the path to the main agent, who routes the fix to the surface's owner.

## Check (run once, at the end)

```bash
cargo test --lib -- orchestrator::signals::tests_doctrine_commands plan::schema::tests::v2_lint
```

Report each group's `checked` count and any unresolved path.
