# Knowledge distillation: stage `knowledge-distill`

Single agent, no subagents. Use the `loom knowledge` CLI for every knowledge write; never Write or Edit a knowledge file. Every file except `mistakes.md` and `mistakes/*` states current truth only.

## Order

1. `loom memory pending --group`. Apply every `stale-knowledge:` entry first, in place, with `loom knowledge replace-section <file> "<heading>" "<body>"`; read the line it prints, because a heading that matches nothing appends instead.
2. Make the structural check green (section "Structural issues").
3. Rewrite the sections this plan made false (section "Sections this plan changed").
4. Curate the remaining entries, and record every unimplemented reviewer suggestion in `concerns` or the topic it belongs to.
5. Give every entry a receipt with `loom memory resolve <id> --outcome promoted|merged|discarded|deferred` (`--target` or `--reason`), then `loom memory pending --strict`.
6. When this stage removed structural issues, finish with `loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt`.

## Structural issues

At planning time (`loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt`, run on the main checkout) the check failed on three issues the baseline does not record:

- `conventions.md` is 251 lines; a tier-1 file holds at most 250. Move one section of more than a few lines to a tier-2 topic under `conventions/` (`loom knowledge update conventions/<slug> "..."`, then `loom knowledge delete-section conventions "<heading>"`, then a 2-4 line summary plus link with `loom knowledge update conventions "..."`; a new topic also gets `loom knowledge annotate conventions/<slug> --blurb "..."`).
- `mistakes/ci-toolchain-and-cargo.md` is 414 lines at `4cc085b0`; a tier-2 file holds at most 400. Split one or more of its sections into a new `mistakes/<slug>` topic the same way, and link it from the old topic and from `mistakes.md`.
- `conventions/git-and-build-workflow.md` cites `doc/astra-bug-sweep-2026-10-15.md`, which commit `4cc085b0` deleted. Rewrite that section without the path.

Run the check again at the start of this stage: fix every issue it reports, these three or others. `--strict-evidence` stays out of this stage's acceptance (it reports pages whose sources changed for reasons unrelated to this plan).

Freshness metadata: `architecture/owned-waits.md` carries no `sources`/`verified` frontmatter. Read the page, pick the source files it describes, and run `loom knowledge annotate architecture/owned-waits --source <path> ... --verified <commit>` with `<commit>` from `git rev-parse HEAD` in this worktree. `architecture/hook-system.md` carries `verified` but no `sources`: add its sources the same way (`--source` only, keeping its `verified` revision unless you re-verify it).

## Sections this plan changed

Each claim below was true at `4cc085b0` and is false after this plan. Re-read the code the stage changed before writing. The stage's acceptance fails while any page outside `mistakes.md` and `mistakes/` still contains `write_summary`, `generate_summary`, `remind_knowledge_capture`, `check_knowledge_recommendations(&stages)` or `in implementation stages`, and until `architecture/owned-waits.md` carries `sources:` and `verified:` and `architecture/hook-system.md` carries `sources:`.

- `architecture/signal-generation.md`, "Signal Sections: Semi-Stable, Dynamic, Recitation and EmbeddedContext": the "Reminder box", "`## Knowledge Management`" and "`## Stage Memory`" bullets say IntegrationVerify gets the box and the Knowledge Management arm and that Stage Memory is Standard-only, quoting "NEVER use 'loom knowledge' in implementation stages". Integration-verify signals now get "## Stage Memory" with "NEVER use 'loom knowledge' in this stage" and no knowledge box or Knowledge Management arm (`orchestrator/signals/format/sections.rs`, `format_semi_stable_section`). Rewrite those bullets with `replace-section`; the recitation's `loom memory list --stage <id>` line too, if the section quotes it.
- `architecture/memory-spool.md` (the `archive_run_state` paragraph) and `architecture/context-retrieval-state.md` (its archive lines): archiving failure while journals exist is now an error that stops plan completion, `loom clean` and `loom init --clean`.
- `entry-points/cli-and-plan-pipeline.md` and `patterns/cli-process-and-conventions.md`: `check_knowledge_recommendations(&stages)` is now `check_knowledge_recommendations(stages, repo_root)` in `plan/schema/validation/knowledge_recommendations.rs` and warns only when the knowledge tree is missing or scaffold-only.
- `architecture/plan-lifecycle-and-fields.md`: the two version-2 bookend lints (`plan/schema/validation/v2_lints/bookends.rs`) and the stage `model` allowlist (`plan/schema/types_v2.rs`, `deserialize_stage_model`).
- `entry-points/hooks.md`, the `learning-validator.sh` rows ("Memory usage check on stop", "Advisory check for session memory usage"): it warns unless the current stage's own journal holds an entry. Any page that describes the post-commit reminder or commit-guard's memory reminder: the post-commit reminder is a memory reminder (`remind_memory_capture`, in `post-tool-use.sh`); `commit-guard.sh` reads only the current stage's journal; `remind_knowledge_capture` is gone. At `4cc085b0` no page names `remind_*`; `architecture/hook-system.md`, `conventions/plan-yaml-and-hooks.md` and `patterns/hook-content-stripping.md` list `learning-validator.sh` in terms that stay true.
- Any page that quotes the scaffold line: the tier-1 scaffold line names `update`, `replace-section` and `delete-section`; `KnowledgeDir::has_content` counts any `##` heading.
- No page names the memory summary path at `4cc085b0` (`architecture.md`'s "KnowledgeDir API" section was corrected while planning), and `patterns/orchestrator-daemon-loop.md` already says "there is no auto-summarize step", which is true once the plan merges. Re-check with `rg -n -e write_summary -e generate_summary doc/loom/knowledge` and fix any hit outside `mistakes.md` and `mistakes/`.

`architecture/memory-spool.md` "The Design" and `stack.md` line 4 were already current at `4cc085b0`; leave both.

The daemon still reports a run complete after a failed plan completion: `finish_orchestration` (`loom/src/daemon/server/orchestrator.rs`) logs the archive error and writes the completion marker, which the status broadcaster turns into `OrchestrationComplete`. Record that in `concerns` (the topic that covers merge and recovery edge cases) as an open gap: an archive failure under the daemon is visible only in the daemon log.

## What this plan learned

Promote the plan's own lessons to `mistakes/` only when they generalise beyond this plan, and record the codex-lane outcome of this stage (units C1 and C2: whether each finished inside the wrapper deadline, and what the orchestrator had to fix after it) in the codex knowledge topics the memory names.
