# Codex Job J2: Completion and Verification Doctrine Text

Lane: `loom-codex-forwarder`, `--model gpt-6.1-sol --effort high`, foreground, Bash timeout 1560000 ms. Wave 1, in parallel with worker F and J1.

Do not run git. Do not read or write anything under `.loom/` or `.work/`. Run no `cargo` command: J1 edits the same crate at the same time; the orchestrator compiles and runs the signal tests after wave 1. `wc -c` and `rg` are fine.

## Files you own

- `loom/src/orchestrator/signals/helpers.rs`
- `loom/src/orchestrator/signals/cache.rs`
- `loom/src/orchestrator/signals/cache/blocks.rs`
- `loom/src/orchestrator/signals/tests_doctrine_blocks.rs`
- `loom/src/orchestrator/signals/tests_doctrine.rs` and `loom/src/orchestrator/signals/tests_doctrine_prefixes.rs` (only where they pin a sentence you change)
- `loom-hooks/_subagent-preamble.txt`
- `CLAUDE.md.template`
- `skills/loom-orchestration/SKILL.md`
- `agents/loom-senior-software-engineer.md`

Locate every edit by text, never by line number: `IN_PROGRESS-PLAN-fewer-disputes.md` and `PLAN-web-host-graft-followthrough.md` edit the same files, and the numbers below are as observed at commit 104ec387.

## Size budget (read before writing anything)

`loom/src/orchestrator/signals/tests_size.rs` pins byte ceilings, and its messages say to trim, never to raise:

| Surface | Ceiling | At 104ec387 |
| --- | --- | --- |
| `CLAUDE.md.template` (`CLAUDE_MD_TEMPLATE_MAX_BYTES`, `:28`) | 20,131 | 20,112 |
| `skills/loom-orchestration/SKILL.md` (`ORCHESTRATION_SKILL_MAX_BYTES`, `:71`) | 20,530 | 20,515 |
| standard stable prefix (`STABLE_PREFIX_MAX_BYTES`, `:42`) | 8,192 | unmeasured |
| standard signal boilerplate floor (`:61`) | 10,240 | unmeasured |

- Run `wc -c CLAUDE.md.template skills/loom-orchestration/SKILL.md` before and after. Each file ends at or below its ceiling: pay for every added byte by trimming wording elsewhere in the same file, without dropping a rule, and name each trim in your report. Trim candidates are wording only: repeated qualifiers, a sentence that restates the line above, a long example where a short one serves. When a file cannot be brought within its ceiling that way, leave that file's edit out entirely and report the byte overage; the orchestrator gives the overage to one loom-software-engineer, who must also stay within the ceiling. Never edit `tests_size.rs`.
- The two prefixes: BLOCK-A and the `helpers.rs` sentence below land in the standard stable prefix. Keep the BLOCK-A change to the pinned text and add the completion sentence at the two places named, nowhere else. The orchestrator measures both ceilings with `cargo test --lib orchestrator::signals::tests_size` after wave 1.
- `cache.rs` is recorded at exactly 511 lines in `loom/maintainability-baseline.txt:16` (and `generate_knowledge_distill_stable_prefix` at exactly 74), and `loom/tests/maintainability.rs` fails on growth AND on shrinkage. Edit inside existing string literals on their existing lines; never add or remove a line in `cache.rs`. Keep `helpers.rs` (385 lines) under 400.

## Change 1: BLOCK-A, the subagent verification rule

The old text starts `VERIFICATION IS THE MAIN AGENT'S JOB - NOT YOURS:` followed by `- Do NOT verify your work. No full build, no full test suite, no linter, no`. Replace it, byte for byte, with the new text below in `loom-hooks/_subagent-preamble.txt` (lines 11-17), `loom/src/orchestrator/signals/cache/blocks.rs` (45-52) and `BLOCK_A` in `loom/src/orchestrator/signals/tests_doctrine_blocks.rs` (11-17). Worker D replaces the fourth copy in `loom-hooks/subagent-verify-guard.sh`. Then run `rg -n "Do NOT verify your work" loom loom-hooks skills agents CLAUDE.md.template --glob '!target'` and confirm the only remaining hit is that guard file.

```text
VERIFICATION IS THE MAIN AGENT'S JOB - NOT YOURS:
- Compile the files YOU wrote as often as you need (`cargo check`,
  `cargo build`, `tsc --noEmit`, `bun run build`) and format them with
  `rustfmt --edition 2021 <files>`. No full test suite, no linter, no
  `cargo fmt`, and never a repeated or looping check.
- AT MOST ONE narrowly-scoped check over the files YOU wrote (e.g.
  `cargo test <your_module>::`), run ONCE. Skip it if you are unsure.
- Report instead: files changed, assumptions made, anything unresolved.
  The stage's gate compiles, tests, and lints; the MAIN AGENT fixes.
```

In a Rust string literal, keep the backticks and the apostrophe as they are. The line breaks must match exactly. `loom/src/orchestrator/signals/cache.rs:381` asserts `AT MOST ONE narrowly-scoped check`, and that phrase is kept.

In `CLAUDE.md.template`, Rule 5: replace `A subagent runs at most one narrowly scoped check, once.` with `A subagent compiles its own files freely and runs at most one narrowly scoped check, once.`

In `skills/loom-orchestration/SKILL.md`, the coordinator rule (the bullet that starts `- AT MOST ONE narrowly-scoped check over the files your workers wrote`, line 123): insert `Compile your workers' files freely; ` at the start of the bullet text, so it reads `- Compile your workers' files freely; AT MOST ONE narrowly-scoped check ...`.

The skill has no section about waves (`rg -i wave` finds nothing). Add this sentence as the last paragraph of `## Rule 6 — Subagents`, directly before `## Rule 7`: `After each wave of workers returns, the main agent compiles the tree once before the next wave or the gate.`

In `agents/loom-senior-software-engineer.md` (`## Self-Review Before Returning`): replace `not by running the build, test suite, or any linter` with `not by running the test suite or any linter (compiling your own files is fine)`. No test pins this text; `loom/tests/token_optimization_contracts.rs:160` reads only the frontmatter.

## Change 2: the completion command

The hook now rewrites a plain `loom stage complete <stage-id>`, alone or followed by `2>&1` and one `| tail -N` / `| head -N`, to the pinned binary path. Every other form is still rejected. Add the sentence `Run it alone; at most \`2>&1 | tail -N\` may follow it.` (in Rust source, the backticks stay literal) at exactly these places:

- `loom/src/orchestrator/signals/helpers.rs`: the `push_str` literal containing `in the worktree ROOT directory` (line 284): after `fix, commit, rerun` and before the trailing `\n\n`, as `. Run it alone; ...`, on the same source line.
- `loom/src/orchestrator/signals/cache.rs`: the `push_str` literal containing `**Run \`loom stage complete <stage-id>\`** when done` (line 270): before its trailing `\n`, on the same source line.
- `CLAUDE.md.template` Rule 4: after `Completion is the session's last act: \`loom stage complete\` prints \`Host job: <state>\` first.` in the **When to commit** paragraph (line 90).

Leave `helpers.rs:210`, `format/helpers.rs:236` and `knowledge.rs:121` unchanged: one completion sentence per signal is enough, and both prefixes are near their byte ceilings.

Do not change `loom stage complete <stage-id>` itself: `loom/src/orchestrator/signals/tests_doctrine_prefixes.rs:51` asserts that completion is framed as the session's last act, and that framing stays.

## Out of scope

`loom-hooks/codex-forward.sh:99-101` and `AGENTS.md.template:56` keep the codex lane's `DO NOT VERIFY` wording: a codex job runs inside the forwarder's sandbox and compiles at most once (`skills/loom-plan-writer/references/codex-implementers.md:23`). `README.md`, `CONTRIBUTING.md` and the knowledge files are knowledge-distill's.

## Done when

The text is written and `wc -c` shows both files within budget. Report each file, each trim, and every pinned test line you changed (`rg` the old sentence under `loom/src/orchestrator/signals/`; a doctrine test that pins the exact old text of a line you changed is updated in this job). The orchestrator then runs `cargo test --manifest-path loom/Cargo.toml --lib orchestrator::signals::` and `git status --short`.
