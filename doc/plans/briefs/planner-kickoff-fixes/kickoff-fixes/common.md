# Kickoff fixes: shared context for every worker

Read this first, then your own brief. Paths are repository-relative; the stage runs from `loom/`.

## The three problems

1. **The size drifts.** A kickoff started from the Feature offer can turn into a standalone plan.
   `kickoff_opening` (`loom/src/planning/guidance/opening.rs:81`) writes "Kickoff, small work" for
   the standalone offer but only "Kickoff" for the feature offer, so the agent guesses the size.
   The server then takes the agent's `size` from its `loom-kickoff` block: the shape the tracker
   publishes (`loom/src/plan_server/chat/kickoff.rs` `KickoffTracker::observe`), the draft's
   `KickoffDraftView.size` (from `shape.size`), the offer `continue_draft` picks
   (`loom/src/plan_server/chat/draft_cmd.rs`, `small = shape.size == Some(Plan)`), and the web's
   `kindOf` (`web/src/plan/new/kickoff-state.ts`, which prefers the agent's size over the offer).
   `confirm_kickoff {size, name}` arms whatever size the browser sends.
2. **Set aside looks like it does nothing.** `set_aside` (`draft_cmd.rs:102`) ends with
   `EndMode::FinalWrite`: the closing message waits for the next idle, then the reply, all within
   `final_write_bound` = 600 s (`loom/src/plan_server/chat/mod.rs:182`). The draft stays `live`
   until teardown, so the "… is still open" notice does not change. An unconfirmed kickoff has
   nothing to write. The stage's first task reproduces this and confirms the cause (see the
   server brief).
3. **A wrongly started plan cannot be thrown away.** Discard deletes only the draft file. A
   draft whose confirmed document exists "lands" and is deleted on the next read
   (`drafts/store.rs` `Files::landed`), even when the session was cancelled halfway, which also
   leaves no way to continue or delete the half-written document.

## Settled decisions (the behaviour every worker builds to)

- Both kickoff openings name the size: `loom plan: Kickoff, feature.` and
  `loom plan: Kickoff, standalone plan.`. The kickoff skill tells the agent to read the size from
  that line.
- A kickoff's size is fixed at start from its offer (`kickoff` = feature, `kickoff-small` = plan)
  and stored in the session's kickoff scope and in its draft. Every published shape carries that
  size, whatever the agent's block says. `confirm_kickoff` with any other size is refused.
- Set aside works in every state:
  - an unconfirmed kickoff (no `target`) is torn down at once (`EndMode::Teardown`);
  - a confirmed kickoff first has its running turn interrupted, then gets a closing write bounded
    by 60 s (`SET_ASIDE_BOUND`) instead of 600 s;
  - `set_aside_kickoff` with `now: true` tears down at once, also while an earlier set-aside is
    still in its closing write ("Stop now").
- A draft lands (is deleted, never listed) only when its session finished: it ended because the
  agent's done fence asked (`EndCause::Agent`) and its target exists, or a locked spelling of the
  target exists (`IN_PROGRESS-`, `DONE-`, `RETIRED-`, or under `archive/`). A draft whose
  session was set aside, interrupted, crashed or restarted stays under Unfinished even when its
  document exists.
- Continue on a confirmed draft whose document exists starts a kickoff whose scope already holds
  the grant for that same target, and whose opening says the document exists and is to be
  finished. Continue on any other draft works as today, with the stored size.
- Delete: `discard_kickoff {session, delete_files}`. The server derives the files from the
  stored confirmed target only, never from the request: for a plan `doc/plans/PLAN-<slug>.md`
  and `doc/plans/briefs/<slug>/`; for a feature `doc/product/<name>/index.md`, then the
  `doc/product/<name>/` directory if it is then empty. It refuses `delete_files: false` when any
  of those exist ("This draft wrote files; delete them too or keep the draft"). It refuses a live
  draft ("Set it aside first"). The draft detail lists the files that exist (`written`) and
  whether HEAD holds any of them (`committed`), so the confirm can list them.

## Wire contract (Rust and TypeScript agree on exactly this)

```json
{"command": "set_aside_kickoff", "client_id": "…", "session": "<uuid>", "now": false}
{"command": "discard_kickoff", "client_id": "…", "session": "<uuid>", "delete_files": true}
```

- `KickoffDraftView.size` becomes required (`"feature" | "plan"`), from the draft's stored size.
- `KickoffDraftDetail` gains `written: string[]` (repository-relative paths that exist, the
  document first, then the bundle directory) and `committed: boolean`.
- `confirm_kickoff` keeps `{size, name}`; the server refuses a size other than the scope's.
- The fixtures `web/src/plan/api/fixtures/chat-commands.json` and
  `web/src/plan/api/fixtures/kickoff-drafts.json` are read by both the Rust wire tests
  (`loom/src/plan_server/wire/tests_chat.rs`) and the TS schema tests. The server worker owns
  both fixtures; the web worker owns the zod schemas.

This project is unreleased: no backwards compatibility, no migration. An old draft file without
`size` fails to parse and is skipped with the existing warning.

## Rules for every worker

- Code size limits: file 400 lines, function 50 lines (`cargo test --test maintainability`
  enforces them against `loom/maintainability-baseline.txt`). Split a file you push past 400;
  never add or raise a ledger row.
- No placeholders, no stubs. Comments explain constraints only.
- Record surprises and mistakes with `loom memory note`, never Claude Code auto-memory. A codex
  unit cannot: it writes the surprise into its final report, and the orchestrator records it.

## Units and territory briefs

The work is cut into units, each with its own brief `unit-<id>.md` in this directory. The
territory briefs (`bridge.md`, `delete.md`, `server.md`, `web.md`) hold the specification and
the pinned interfaces; a unit brief names which of their sections it builds and the exact files
it owns. "Worker B / D / S / W" in a territory brief means the units of that territory: B is
`bridge`; D is `scope-size`, `safe-remove`, `draft-store`, `draft-delete`; S is `openings`,
`wire`, `size-pinning`, `draft-commands` (plus the Claude reproduction `repro`); W is
`web-schemas`, `web-size`, `web-drafts`. Where a territory brief says "Run `cargo test …`" or
`bunx vitest run …`, that is the orchestrator's proof for the unit, run after the wave.

## Rules for codex units

Codex units run gpt-6.2-sol at effort xhigh. They open no skill files, so the rules that apply
are quoted here:

- Never run `git`, in any form. Never read or write any path under `.loom/`.
- You may compile (`cargo check --all-targets` from `loom/`, `./node_modules/.bin/tsc -b --noEmit`
  from `web/`). Do not run tests: fixture-backed tests cannot run in your sandbox, and the
  orchestrator runs them after the wave. Fix compile errors only in files you own; an error in a
  file another unit owns is expected while the wave runs and is not yours to fix.
- Your file list is exclusive. Never edit a file outside it, not even a one-line import; if you
  need one, say so in your report.
- Interfaces shared with other units are pinned in the territory briefs and `common.md`. Code to
  them exactly, even before they exist on disk. The navigation kit (`loom map`,
  `loom knowledge context`) answers from the base tree and does not see other units' edits.
- Keep each file at least 10% under 400 lines (`cargo fmt` and `oxfmt` run after you and can push
  a file over); functions at most 50 lines.
- Rust: literal braces inside `format!` strings are doubled; a CLI placeholder such as `<id>` in
  a rustdoc comment goes in backticks; no `unwrap()` outside tests; errors carry context.
- TypeScript: no parameter properties in classes (`erasableSyntaxOnly`); no `any`; zod schemas are
  strict objects as the neighbouring ones are; components follow the file's existing patterns.
  Do not run `oxfmt`; the orchestrator formats `web/` after the wave.
- End with a short report: files changed, anything you could not do, and every surprise.
