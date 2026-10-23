# W2: template relocation, orchestration skill and size ceilings

Tier: sonnet (`loom-software-engineer`). Working directory for cargo: `loom/`. Paths below are repo-relative.

## Files you own

- `CLAUDE.md.template` (Rules 3, 3b and the fifth Rule 14 bullet only)
- `skills/loom-orchestration/SKILL.md`
- `loom/src/orchestrator/signals/tests_size.rs`
- `loom/src/orchestrator/signals/tests_doctrine_v2.rs` (`block_f_agrees_across_every_surface` and the `BLOCK_F` doc comment)
- `loom/src/orchestrator/signals/tests_doctrine_gate.rs` (the orchestration-skill operand of its `loom check`/`--criterion` assertion only, Part B)
- `loom/src/orchestrator/signals/helpers.rs` (the `STAGE_EXIT_RULES` doc comment only)
- `loom-hooks/tests/post-tool-use-ceiling-hard-block.sh` (one assertion)
- `loom-hooks/tests/session-start-compact.sh` (two assertions)

Read-only: `loom-hooks/post-tool-use.sh:121-143` (`_loom_ctx_check_main_ceiling`), `loom-hooks/session-start.sh:147-156`, `loom/src/orchestrator/signals/helpers.rs:36` (`CONTEXT_CEILING_HANDOFF`), `:200-231`, `loom/src/orchestrator/signals/cache.rs:90-166`, `loom/src/orchestrator/signals/cache/blocks.rs`, `loom/src/orchestrator/signals/tests_guidance_manifest.rs` (the relocation manifest). W1 edits the Go skill's triggers in parallel; W3 edits `post-tool-use.sh` away from the ceiling message.

The codex unit C1 ran before you and wrote `loom/src/orchestrator/signals/tests_guidance_manifest.rs`; never edit it. It freezes 40 template lines as they stand at this plan's base (the 21 `###` headings, the 6 hard stops, Rule 6's 4 bullets, Rule 12's 4 steps and Rule 14's 5 bullets; identical to 6fee4ecd) and lists each relocated item with a needle in its new home, including the instructions of the skill paragraphs you remove (Part B), pinned in the template or the standard stable prefix.

Plan B (`doc/plans/PLAN-host-verification.md`) ran before this plan:

- It rewrote template Rules 4 and 13 and the skill's paragraphs under the labels `**Complete ONLY a settled stage; ...**`, `**The gate in a standard stage.**` and `**When the stage cannot finish.**`, keeping those bold labels.
- It made `loom check` relay to a daemon host job. A redirected or piped `loom` command loses its relay line.

Gate-dedup (`doc/plans/PLAN-gate-dedup.md`) ran after plan B. It removed the verifier subagent: the orchestrator runs `loom check <stage-id>` itself, once, and a fix round re-checks with `--criterion <n>`. It rewrote the skill's `**The gate in a standard stage.**` paragraph, template Rules 4 and 5 and the stable prefix's gate block (`cache/blocks.rs`) to that flow, deleted the verifier row of Rule 7's table, and added `tests_doctrine_gate.rs`.

Anchor every removal by its bold label, never by line number. Your Part A must leave every one of those lines in place except the fifth Rule 14 bullet, which the manifest records as relocated, and your new Rule 14 bullet must contain the needle `2>&1; echo "exit=$?"`.

## Earlier plans touch these files first

Plan 3 of this programme (`PLAN-01-doctrine-truth-and-plan-guards`) runs before this plan. Re-read each file before editing, and anchor by text, never by line number:

- `CLAUDE.md.template:75`: plan 3 changes `loom memory list` to `loom memory list --stage <stage-id>`. This brief replaces that paragraph anyway.
- `loom-hooks/session-start.sh:153`: plan 3 changes `run 'loom memory list'` to `run 'loom memory list --stage ${LOOM_STAGE_ID}'`. Your needle `loom memory list` survives either spelling.
- `loom-hooks/tests/session-start-compact.sh`: plan 3 adds a check for `loom memory list --stage test-stage` after the signal-path check.
- `loom/src/orchestrator/signals/tests_size.rs:36`: plan 3 rewrites "Actual size is now about 17,000 bytes." (its brief says `Actual size is about 19,900 bytes.`). You rewrite that doc comment.
- `skills/loom-orchestration/SKILL.md`: plan 1 (`PLAN-02-delivery-truth`) relabels the **Read receipts.** paragraph (`:119-125` at 6fee4ecd) **Sibling-read ledger.** and changes "already read whole" to "already requested whole"; nothing else in the file. Keep whatever that paragraph says at run time.

Run `git diff 6fee4ecd -- CLAUDE.md.template skills/loom-orchestration/SKILL.md` first and read what the earlier plans changed.

## Part A: `CLAUDE.md.template`

Both procedures already live in hook output: the ceiling message (`post-tool-use.sh:132`, `_loom_ctx_check_main_ceiling`) and the compact/resume re-anchor (`session-start.sh:147-155`). The stage signal also carries `CONTEXT_CEILING_HANDOFF` (`helpers.rs:36`, appended by `append_completion_rules`). Keep every `###` heading. Hard stop 4 (`:12`) stays.

1. `### 3. CONTEXT CEILING = HANDOFF`: replace the body paragraph with
   `The trigger is the PostToolUse hook's`CONTEXT CEILING REACHED`line in your own tool output, which names the handoff command; never estimate the ceiling yourself.`
2. `### 3b. POST-COMPACTION RECOVERY`: replace the body paragraph with
   `After compaction or a resumed session, follow the SessionStart hook's re-anchor message.`
3. `### 14. TOKEN EFFICIENCY`, the fifth bullet (it starts `- Don't narrate between tool calls`): replace it with this line, exactly:

   ```text
   - Don't narrate between tool calls; explain decisions, not standard operations (Rule 19). Prefer Edit over Write. Log verbose output, then read its failures: `<cmd> > "${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}/<step>.log" 2>&1; echo "exit=$?"`, `rg -n -m 20 -M 300 '<failure pattern>' <log> | head -c 6000`; piping `<cmd>` itself hides its exit code. Never redirect `loom` commands. Skip agents for simple tasks. `.claudeignore` generated files, artifacts, lock files.
   ```

   The `${LOOM_SCRATCH_DIR:-${TMPDIR:-/tmp}}` fallback matters: the installed template also serves sessions outside loom, where `LOOM_SCRATCH_DIR` is unset. "Never redirect `loom` commands" matters because `loom check`, `loom stage commit` and the other relaying commands print a line the relay hook must see. Rule 8's row (`` `cat`, `head`, `tail` as file readers ``) stays.

Measure the template before and after these edits (`wc -c`) and report both. The template must end smaller than at the base: integration-verify compares it with this plan's base (on 2026-10-21 the base text was 20,320 bytes, and the edits above are about 150 bytes net shorter). Nothing else in the template changes: `## KNOWLEDGE-FIRST` is pinned byte for byte (`knowledge_consumption_contract_agrees_with_claude_md_template`, `tests_doctrine_prefixes.rs:128-141`), Rule 4's commit sentinels are pinned (`tests_commit_timing.rs`).

## Part B: `skills/loom-orchestration/SKILL.md`

Every stage main session loads this skill, so its bytes (24,573 at 6fee4ecd, 25,019 on 2026-10-21) are paid on every later request. Remove the paragraphs below. Each one has a surviving copy in `CLAUDE.md.template` or in the standard and integration-verify stable prefixes (the two prefixes whose sessions load the skill, `LOAD_ORCHESTRATION_SKILL` in `cache.rs`). C1's manifest pins each surviving instruction in that surface. Before removing R2, R3 and R11, read their current text (plan B's; gate-dedup's for R3) against the surviving copy named below. A fact the paragraph carries that the surviving copy lacks is a report to the main agent, not a silent deletion. Anchor each removal by its opening text, and delete the blank line that follows it. Line numbers in the table are 6fee4ecd history.

| # | Remove (opening text, line at 6fee4ecd) | Surviving copy |
| --- | --- | --- |
| R1 | The whole `## Rule 1 — Plans and stage execution` section (`:38-46`) | Template `### 1. PLANS` (`:61-63`), "**Loom plans:** only on explicit request ... **Executing a stage:** your signal's tasks and completion checklist are binding" |
| R2 | `**Complete ONLY a settled stage; completion is the session's LAST act.**` (`:61-64`) | Stable prefix `append_settled_completion_rules` (`helpers.rs:208-216`): "`loom stage complete` is the LAST act of your session ... post-completion work is LOST WORK" |
| R3 | `**The gate in a standard stage.**` (`:66-71`) | The stable prefix's orchestrator gate block (`cache/blocks.rs`, gate-dedup's), which carries `` `loom check <stage-id>` `` and `--criterion`; template Rule 5 (a standard stage's gate is the orchestrator's single `loom check`) and Rule 4 (the commit follows a green `loom check` and the final review) |
| R4 | `Group commits logically: 5 files across 3 concerns = 3 commits, not 1 or 5.` (`:73`) | Template Rule 4 (`:92`) "one commit per concern"; stable prefix `append_commit_timing_rules` (`helpers.rs:150`) "One logical commit per concern" |
| R5 | The whole `## Rule 5 — Subagent preamble` section (`:75-87`) | Template `### 5. SUBAGENT RESTRICTIONS` (`:96`), which names the preamble file, its contents, the spawn guard and the codex route |
| R6 | `**GROUP TASKS: small tasks go to ONE subagent, never one per task or file.**` (`:100-104`) | Template Rule 6, first bullet (`:102`). Keep its last sentence: move `The ≤~6 threshold counts subagents after grouping, never raw tasks.` to the end of the `Team lead:` paragraph (`:97-98`) |
| R7 | `**Outside a loom stage,`loom subagents watch`is unavailable:**` (`:127`) | Template Rule 6, fourth bullet (`:105`) |
| R8 | `**Subagents are ONE-SHOT.**` (`:140-143`) | Template Rule 6, second bullet (`:103`) |
| R9 | `**Never pass`name`to a subagent spawn**` (`:145-148`) | Template Rule 6, third bullet (`:104`) |
| R10 | `Fable-tier work has no pinned agent type; pass the model override explicitly at spawn.` (`:197`, first line under `## Rule 7`) | Template Rule 7 (`:122`), the same sentence |
| R11 | `**When the stage cannot finish.**` (`:292`, BLOCK-F) | `STAGE_EXIT_RULES` (`helpers.rs:221`), appended to the standard, integration-verify and knowledge-distill stable prefixes by `append_completion_rules` (`:224-231`) |

Gate-dedup's `tests_doctrine_gate.rs` asserts that the orchestration skill contains `loom check` and `--criterion`, which R3's paragraph carries. After the removals, run `rg -n -F -e 'loom check' -e '--criterion' skills/loom-orchestration/SKILL.md`. If either is gone, remove only the skill from that assertion and keep the gate block's operand and every `loom-verifier` absence assertion: the manifest pins both needles in the standard stable prefix, which the stage main session reads. The main agent disputes the integrity event this raises.

Then replace the intro paragraph (`:29-31`, starts `The rules a session needs`) with:
`The rules a session needs when it spawns subagents or runs a loom stage as its main agent, beyond what`~/.claude/CLAUDE.md`and the stage signal already carry, filed under the CLAUDE.md rule number they belong to. A single-agent session does not need this skill.`

Keep, byte for byte, everything pinned elsewhere: BLOCK-B (Rule 7 points 1-5, `tests_doctrine.rs` `block_b_agrees_across_every_surface`), BLOCK-C (`**Checking on subagents: ...` through the harvest paragraph, `tests_doctrine_waiting.rs:33`), BLOCK-E (`**Review order (plan v2):**`, `BLOCK_E`, `tests_doctrine_v2.rs:32-34`), the commit sentence with `loom stage commit` and `--wait 90` (`tests_commit_timing.rs::commit_command_doctrine_is_on_every_surface`), `loom-codex-forwarder` (`tests_doctrine.rs:280-289`), the `## Rule 6 — Subagents` heading (`format/helpers.rs:19` cites it), and the coordinator preamble text (`commands/usage/sections/agents.rs:268` reasons about it). Measure the skill before and after (`wc -c`) and report both; integration-verify requires it smaller than at this plan's base.

## Part C: BLOCK-F's surfaces

R11 removes BLOCK-F from the skill, so:

1. `tests_doctrine_v2.rs`, `block_f_agrees_across_every_surface` (`:82-105`): delete the `("skills/loom-orchestration/SKILL.md", ORCHESTRATION_SKILL.to_string())` tuple. Keep the three prefix tuples and the assertion. `ORCHESTRATION_SKILL` stays in use by `block_e_agrees_across_every_surface`.
2. Its `BLOCK_F` doc comment (`:36-37`): `Every stable prefix and the` / `orchestration skill carry it byte for byte.` becomes `The standard, integration-verify and knowledge-distill stable prefixes carry it byte for byte; the orchestration skill, which those sessions load, does not repeat it.`
3. `helpers.rs`, the `STAGE_EXIT_RULES` doc comment (`:218-220`): drop `Byte-identical in the orchestration skill`; say that `tests_doctrine_v2.rs` pins every prefix copy.

## Part D: size ceilings (`loom/src/orchestrator/signals/tests_size.rs`)

1. `CLAUDE_MD_TEMPLATE_MAX_BYTES` (`:38`): set it to the template's measured byte count after Part A (`wc -c < CLAUDE.md.template`, whatever it is; earlier plans changed the template). Rewrite its doc comment (`:22-37`) to the current truth: the template is pasted into every session, Rules 3 and 3b live in the hooks that print them (`tests_guidance_manifest.rs` lists every relocation), and the ceiling equals the actual size, so any addition fails here. Drop the raise-and-lower narrative.
2. Add `ORCHESTRATION_SKILL_MAX_BYTES` set to the skill's byte count after Part B, `const ORCHESTRATION_SKILL: &str = include_str!("../../../../skills/loom-orchestration/SKILL.md");`, and `orchestration_skill_stays_under_its_size_ceiling`, mirroring `claude_md_template_stays_under_its_size_ceiling` (`:81-91`). Its doc comment: every stage main session loads the skill, so its bytes are paid on every later request of that session. Update the module doc (`:1-11`) to name the skill among the capped surfaces.

## Part E: runtime pins in two hook tests

No test pins either procedure today: `post-tool-use-ceiling-hard-block.sh` checks only the `CONTEXT CEILING REACHED` prefix, and `session-start-compact.sh` checks only the signal path and `Understand before acting`.

1. `loom-hooks/tests/post-tool-use-ceiling-hard-block.sh`: after the first `*"CONTEXT CEILING REACHED"*` check (`:41-44`), add the same shape asserting `"$STDERR_OUT" == *"--trigger ceiling"*`.
2. `loom-hooks/tests/session-start-compact.sh`: plan 3's brief adds a check that the additionalContext contains `loom memory list --stage test-stage`. Re-read the file. After the `Understand before acting` check (`:43` at 6fee4ecd), add one check in the file's own style that the additionalContext contains `/handoffs/`. Add a `loom memory list` check only if the file has none at run time.

## Your one check

`cargo test --lib signals::`, once, from `loom/`. It includes C1's manifest and gate-dedup's `tests_doctrine_gate.rs`: a failure in either is yours to fix in your own files. The main agent runs the gate and the hook suite.

## Traps

- The manifest's `FROZEN_LINES` holds the base text of the Rule 14 bullet you rewrite; its relocation entry is what lets that line go. Any other frozen line you change or drop fails `every_frozen_template_line_survives_or_is_relocated`.
- A manifest failure naming a skill relocation (an R-item needle missing from the template or the standard stable prefix) means a surviving copy is gone: report it; do not restore the removed paragraph.
- After Part B, `rg -n 'Rule 5|Rule 1' skills/loom-orchestration/SKILL.md` and read each hit: references to "Rule 5's preamble" now point at the template's Rule 5, which still describes the preamble; nothing should point at a removed section.
- Measure both byte counts after `cargo fmt` and after any final wording change; the ceilings are exact.
