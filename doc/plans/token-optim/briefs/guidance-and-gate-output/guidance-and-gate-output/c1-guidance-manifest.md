# C1: the relocation manifest test (codex unit `guidance-manifest`)

Lane: codex, `--model gpt-5.6-terra --effort xhigh`, forwarded by `loom-codex-forwarder` in wave 1, before any Claude worker starts. Paths below are repo-relative; cargo runs from `loom/`.

## Files

Write:

- `loom/src/orchestrator/signals/tests_guidance_manifest.rs` (new)
- `loom/src/orchestrator/signals/mod.rs` (one three-line module declaration)

Read only: `CLAUDE.md.template`, `agents/loom-verifier.md`, `loom-hooks/post-tool-use.sh`, `loom-hooks/session-start.sh`, `loom/src/orchestrator/signals/helpers.rs` (`CONTEXT_CEILING_HANDOFF`), `loom/src/orchestrator/signals/cache.rs` (`generate_stable_prefix`), `loom/src/fs/permissions/constants.rs` (`HOOK_POST_TOOL_USE`, `HOOK_SESSION_START`), `loom/src/orchestrator/signals/tests_size.rs` (import style to mirror).

Never run git and never write under `.loom/`. The `loom map` and `loom knowledge context` commands answer from the stage's base commit; read files directly with `sed -n` or `rg`.

## Why

`CLAUDE.md.template` is installed as every session's `~/.claude/CLAUDE.md`, inside and outside loom stages. Claude workers running after you shrink its Rules 3 and 3b to one line each (the hooks print both procedures) and rewrite the fifth Rule 14 bullet; `agents/loom-verifier.md` gains a gate-log recipe. This test records where each relocated item now lives and fails when a template heading or rule bullet disappears without a recorded home.

## Step 1: declare the module in `loom/src/orchestrator/signals/mod.rs`

Find the block of `#[cfg(test)] #[path = "tests_doctrine*.rs"] mod ...;` declarations (start from `mod tests_doctrine_prefixes;`). After the last declaration of that block, add:

```rust
#[cfg(test)]
#[path = "tests_guidance_manifest.rs"]
mod tests_guidance_manifest;
```

Declared at this level, the filter `cargo test --lib signals::tests_guidance_manifest` selects the module. Declare it once; never `#[path]`-include the file anywhere else.

## Step 2: data in `tests_guidance_manifest.rs`

1. Module doc comment (`//!`), three or four lines: the installed template also serves sessions outside loom plans, so guidance is relocated and never deleted; this file lists each relocated item with its new home and a literal needle, and freezes the template's `###` headings and rule bullets as they stood at 6fee4ecd. Write any `<placeholder>` in a doc comment inside backticks (rustdoc runs with warnings denied).
2. Homes:

   ```rust
   use super::cache::generate_stable_prefix;
   use crate::fs::permissions::constants::{HOOK_POST_TOOL_USE, HOOK_SESSION_START};

   const TEMPLATE: &str = include_str!("../../../../CLAUDE.md.template");
   const VERIFIER: &str = include_str!("../../../../agents/loom-verifier.md");
   ```

3. `const RULE_14_VERBOSE_BULLET: &str = r#"..."#;` holding line 40 of the frozen list below, and `const FROZEN_LINES: &[&str]` holding all 40 lines in order, each as a raw string `r#"..."#` copied byte for byte (em dashes, backticks and double quotes included), with `RULE_14_VERBOSE_BULLET` standing in for line 40. No line contains the two-character sequence `"#`, so `r#` delimiters are safe.
4. The relocation table:

   ```rust
   struct Relocation {
       item: &'static str,
       frozen: Option<&'static str>,
       home: &'static str,
       needle: &'static str,
   }
   ```

   `const RELOCATIONS: &[Relocation]` with these eight entries, in this order (`-` in the frozen column means `None`; the needle column names a line of the needle block below):

   | # | item | frozen | home | needle |
   | --- | --- | --- | --- | --- |
   | 1 | `Rule 3 ceiling trigger` | - | `loom-hooks/post-tool-use.sh` | N1 |
   | 2 | `Rule 3 handoff command` | - | `loom-hooks/post-tool-use.sh` | N2 |
   | 3 | `Rule 3 finish the unit, then hand off` | - | `standard stable prefix` | N3 |
   | 4 | `Rule 3b re-read the signal` | - | `loom-hooks/session-start.sh` | N4 |
   | 5 | `Rule 3b memory list` | - | `loom-hooks/session-start.sh` | N5 |
   | 6 | `Rule 3b latest handoff` | - | `loom-hooks/session-start.sh` | N6 |
   | 7 | `Rule 14 verbose output` | `Some(RULE_14_VERBOSE_BULLET)` | `CLAUDE.md.template` | N7 |
   | 8 | `Rule 14 verbose output` | `Some(RULE_14_VERBOSE_BULLET)` | `agents/loom-verifier.md` | N8 |

   The needles, one per line after the `Nk:` label, copied byte for byte; no needle has leading or trailing spaces. Write each as a raw string `r#"..."#`.

   ```text
   N1: CONTEXT CEILING REACHED:
   N2: --trigger ceiling
   N3: finish the unit of work in progress and run `loom handoff --stage <id> --session <id> --trigger ceiling`
   N4: Re-anchor: re-read ${SIGNAL_PATH}
   N5: loom memory list
   N6: /handoffs/ file
   N7: 2>&1; echo "exit=$?"
   N8: 2>&1; echo "exit=$? log=
   ```

   Before writing the first six needles, confirm each one is a substring of today's file with `rg -n -F` (`session-start.sh` was edited by an earlier plan; the stable-prefix needle is in `CONTEXT_CEILING_HANDOFF` in `helpers.rs`). If one is missing, report it and the text you found; do not invent another needle. The last two are absent today by design: Claude workers add them after you.
5. `fn home_text(home: &str) -> String`: `"CLAUDE.md.template"` gives `TEMPLATE`, `"agents/loom-verifier.md"` gives `VERIFIER`, `"loom-hooks/post-tool-use.sh"` gives `HOOK_POST_TOOL_USE`, `"loom-hooks/session-start.sh"` gives `HOOK_SESSION_START`, `"standard stable prefix"` gives `generate_stable_prefix()`; any other label panics naming it.
6. `fn missing_frozen_lines(template: &str) -> Vec<&'static str>`: every `FROZEN_LINES` entry that is neither a whole line of `template` (`template.lines().any(|l| l == frozen)`) nor the `frozen` value of some relocation entry, in `FROZEN_LINES` order.

## Step 3: four tests in the same file

Each `#[test]` fn under 50 lines:

- `every_relocated_item_is_carried_by_its_home`: collect every entry whose needle is not a substring of `home_text(home)` as a string naming item, home and needle; assert the collection is empty and print all of it on failure (not only the first).
- `every_frozen_template_line_survives_or_is_relocated`: `missing_frozen_lines(TEMPLATE)` is empty; the message lists the missing lines.
- `a_deleted_frozen_heading_fails_the_manifest_check`: `missing_frozen_lines(&TEMPLATE.replace("### 3b. POST-COMPACTION RECOVERY", ""))` equals `vec!["### 3b. POST-COMPACTION RECOVERY"]`. This keeps the check from passing vacuously.
- `frozen_list_counts_the_base_template_shape`: `FROZEN_LINES.len()` is 40 and exactly 21 entries start with `###`.

Keep the file under 360 lines: the limit is 400, and the orchestrator's `cargo fmt` pass only grows hand-wrapped code.

## The 40 frozen lines (6fee4ecd, `rg '^(### |- |[0-9]+\. )' CLAUDE.md.template`)

One line per entry, in order; the text block fence is not part of any line.

```text
1. **PLANS** — plan files only under `doc/plans/`. (Rule 1)
2. **MEMORY** — `loom memory`/`loom knowledge`, never Claude Code auto-memory. (Rule 12)
3. **GIT** — only the main agent stages, commits, or completes a stage, and only as its final step. (Rule 4)
4. **CONTEXT** — handoff only on the hook's ceiling report, never pre-emptively. (Rule 3)
5. **BRANCHES** — never create one unasked. (Rule 9b)
6. **DELEGATION** — a cost decision, tokens times model tier: beyond a small change, the main agent delegates. (Rule 7)
### 1. PLANS
### 2. NO PLACEHOLDERS
### 3. CONTEXT CEILING = HANDOFF
### 3b. POST-COMPACTION RECOVERY
### 4. COMMIT AND COMPLETE (HOOK-ENFORCED)
### 5. SUBAGENT RESTRICTIONS
### 6. SUBAGENTS
- Group small tasks into ONE subagent, never one per task or file; two subagents never write one file.
- Subagents are ONE-SHOT: brief completely, wait, harvest once, let it end. A follow-up is a fresh spawn of the same type.
- Never pass `name` to a subagent spawn (agent teams are the exception): a named spawn idles as a teammate instead of stopping, so `loom subagents watch` cannot bind it and a named `loom-code-reviewer` records no review round.
- Inside a loom stage, wait with one background `loom subagents watch`, never a poll loop. Outside a stage it exits 5 (it needs `LOOM_STAGE_ID`, `LOOM_SESSION_ID`, `LOOM_WORK_DIR`): a background Agent-tool subagent reports its own completion, so wait for that notification and never poll.
### 7. SKILLS, AGENTS, AND MODEL CHOICE
### 8. NATIVE TOOLS AND CODE FORMATTING
### 9. NO ATTRIBUTION
### 9b. NEVER CREATE BRANCHES UNASKED
### 10. WORKTREE ISOLATION
### 11. BINARY USAGE
### 12. PROJECT KNOWLEDGE
1. Find the home in `INDEX.md`: the tier-2 topic whose blurb covers it, else a new `<category>/<slug>`. Tier-1 files take summaries only.
2. Write through the CLI; every write regenerates `INDEX.md`. `loom knowledge update <category>/<slug> "<entry>"` APPENDS, scaffolding a new topic. Correct a section with `loom knowledge replace-section <file> "<Heading>" "<body WITHOUT the ## line>"`, never `update`; on a non-matching heading it appends and SAYS so — read that line, or the stale text stands. Name the wrong claim in the replacement.
3. A new topic also gets `loom knowledge annotate <category>/<slug> --blurb "<80 chars max>"` and a 2-4 line summary plus link in its tier-1 file.
4. An Edit-tool fix leaves `INDEX.md` stale: run `loom knowledge sync` after it. End a batch with `loom knowledge check`.
### 13. SILENT FAILURE DETECTION
### 14. TOKEN EFFICIENCY (HARD REQUIREMENT)
- Query before reading (KNOWLEDGE-FIRST); open a file only once a lookup names its lines.
- Read ranges, not files (Read's offset/limit); whole files only when using all of it.
- Independent reads, greps, commands go in ONE message.
- Never re-read what's already in context, or from adjacent curiosity.
- Don't narrate between tool calls; explain decisions, not standard operations (Rule 19). Prefer Edit over Write. Pipe verbose output: tests through `tail -50` or `rg -B 2 -A 5 'FAIL|ERROR'`, builds to a file, linters through `head -30`. Skip agents for simple tasks. `.claudeignore` generated files, artifacts, lock files.
### 15. QUALITY GATES
### 16. DEPENDENCIES
### 17. CODE SIZE LIMITS
### 18. KNOWLEDGE PLACEMENT
### 19. WRITING STYLE
```

Cross-check a few long lines against the worktree's `CLAUDE.md.template` with `rg -n -F`. If a line there differs from the block, keep the block's text and report the difference; the orchestrator decides.

## Done means

Both files written and no other file touched. Run no build, no test and no formatter: a crate build can outlast the 540-second forward deadline, and the orchestrator formats, compiles and tests right after you. The orchestrator then runs `cargo test --lib signals::tests_guidance_manifest` and expects, at this point, exactly one failing test, `every_relocated_item_is_carried_by_its_home`, naming exactly the two `Rule 14 verbose output` entries (their needles land after you). Any other failure is yours.

## Report

Files changed, any needle you could not confirm with the text you found instead, and anything you could not resolve.
