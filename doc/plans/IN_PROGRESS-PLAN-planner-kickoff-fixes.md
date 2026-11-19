# PLAN: Planner Kickoff Fixes

Implements: none

## Overview

Three planner kickoff problems, fixed in one implementation stage:

1. **The size drifts.** A kickoff started from the Feature offer can become a standalone plan.
   The feature opening says only "Kickoff" (`loom/src/planning/guidance/opening.rs:81`), so the
   agent guesses the size, and the server, the draft and the browser then follow the agent's
   `size` instead of the offer.
2. **Set aside looks like it does nothing.** `set_aside` (`loom/src/plan_server/chat/draft_cmd.rs:102`)
   ends with `EndMode::FinalWrite`, which waits for the next idle, a closing reply and up to
   600 s (`loom/src/plan_server/chat/mod.rs:182`) while the "… is still open" notice stays the
   same.
3. **A wrongly started plan cannot be thrown away.** Discard removes only the draft file, and a
   draft whose document exists is deleted as "landed" even when its session was cancelled
   halfway.

After this plan, a kickoff keeps the size it was started with, Set aside ends an unconfirmed
kickoff at once and a confirmed one within 60 s (with a Stop now that ends it at once), a draft
whose session did not finish stays under Unfinished, and Delete removes a draft together with
exactly the files its kickoff wrote.

The plan file and its briefs must be committed before `loom run`. `loom run` renames this file
to `doc/plans/IN_PROGRESS-PLAN-planner-kickoff-fixes.md`; the briefs stay at
`doc/plans/briefs/planner-kickoff-fixes/`.

## Goals

- Both kickoff openings name the size; the kickoff skill reads it from there.
- The size is fixed at start from the offer, stored in the kickoff scope and the draft; every
  published shape, the Write button and the write grant use it; `confirm_kickoff` with another
  size is refused.
- Set aside works in every state; plan or vision writing can be interrupted (Interrupt) or
  ended (Set aside, Stop now).
- A draft lands only when its session finished (the agent's done fence) with its document
  present, or when its plan went into execution.
- Delete removes a draft and only the files its confirmed target names, refuses anything else,
  and never follows a symlink.
- Non-goals: user-supplied organisational guidance (dropped; it belongs in CLAUDE.md/AGENTS.md);
  deleting a finished plan from its own page; changing the End command's 600 s closing write for
  ordinary chats.

## Decisions

1. **One implementation stage.** The server, bridge and web changes share the wire contract and
   two fixtures, and no part needs another merged first (Stage Necessity Test Q1-Q4 all no). The
   stage runs in `loom/` because loom runs Rust contracts as `cargo test <name> -- --exact` from
   the stage working directory; web work is proven by acceptance commands run in `../web`.
   Knowledge bootstrap is skipped: `doc/loom/knowledge/architecture/planner-uploads-and-drafts.md`
   and `planner-web-shell.md` already describe the draft store and the shell.
2. **The scope holds the size.** `KickoffScope.size` is written at start by `prepare_scope`
   (`loom/src/plan_server/chat/team.rs:41`), the only place that knows the offer for every later
   step; the draft copies it. The agent's `size` is overwritten on every observed block.
3. **Set aside bounds its own end.** `EndPlan` gains a per-end `bound` and
   `Bridge::end_within` sets it, so the 600 s default stays for ordinary chat ends and server
   drain. A second set-aside with `now: true` uses the bridge's existing rule that a later
   Teardown hardens a FinalWrite plan (`loom/src/plan_session/bridge/ending.rs` `plan_end`).
4. **"Finished" comes from the end cause.** `BridgeEvent::Ended` gains `cause`; a draft is
   finished when the done fence ended it (`EndCause::Agent`, `bridge/done_fence.rs:17`).
5. **Delete trusts only the stored target.** The server derives the files from the draft's
   stored confirmed document, checked byte for byte against `kickoff_target(size, name)`, never
   from the request, and removes them through `safe_fs` without following links.
6. **Continue resumes a written draft.** A confirmed draft whose document exists continues into
   a kickoff whose scope already holds the grant for that same target
   (`SessionScope::kickoff_resumed`), so the session can finish the document.
7. **Implementation runs on gpt-6.2-sol wherever possible.** The user chose this. Twelve of the
   thirteen units are codex units at effort xhigh, sized to about 4 to 6 file-sized changes each,
   including the security-sensitive delete path (`safe-remove`, `draft-delete`), which
   integration-verify reviews on its own. Three pieces stay on Claude, because each one has to
   run fixture-backed tests, and a codex unit cannot:
   - reproducing and confirming the Set aside cause (`repro`);
   - converging on failing fixture-backed tests after the waves (one sonnet fix worker);
   - review and the full-suite verification in integration-verify.

   Codex is not allowed on the bookend stages (knowledge-distill and integration-verify), so
   review findings there are fixed by Claude engineers. If codex is unavailable at run time, the
   stage signal reroutes the units to sonnet.

## Execution Diagram

```mermaid
graph LR
    kickoff-fixes --> integration-verify
    integration-verify --> knowledge-distill
```

## Stages

### 1. kickoff-fixes

One stage, `working_dir: "loom"`. Thirteen units, one brief each (every unit reads `common.md`
first, then the territory brief its own brief names). Twelve are codex units on gpt-6.2-sol at
effort xhigh; one, `repro`, is a Claude sonnet worker:

| Worker | Role | Tier | Files owned | Shared context | Brief path |
| --- | --- | --- | --- | --- | --- |
| repro | Reproduce Set it aside (wave 0) | sonnet | src/plan_server/chat/tests_dispatch/set_aside.rs; src/plan_server/chat/tests_dispatch.rs | common.md, server.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-repro.md |
| bridge | Bridge end bound and cause (wave 1) | codex sol | src/plan_session/bridge/session.rs; src/plan_session/bridge/ending.rs; src/plan_session/bridge/end_plan.rs; src/plan_session/bridge/commands.rs; src/plan_session/bridge/mod.rs; src/plan_session/bridge/recovery.rs; src/plan_session/bridge/harness.rs; src/plan_session/bridge/tests_stopping.rs; src/plan_session/bridge/closing_contract_tests.rs; src/plan_server/summaries/rebuilds/session.rs | common.md, bridge.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-bridge.md |
| scope-size | Kickoff scope size and arming (wave 1) | codex sol | src/plan_session/scope.rs; src/plan_session/scope/arm.rs; src/plan_session/scope_tests.rs | common.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-scope-size.md |
| safe-remove | No-follow tree removal (wave 1) | codex sol | src/fs/safe_fs.rs; src/fs/safe_fs/write.rs; src/fs/safe_fs_write_tests.rs | common.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-safe-remove.md |
| openings | Openings, title, continuation, skill (wave 1) | codex sol | src/planning/guidance/opening.rs; src/planning/guidance/continuation.rs; src/planning/guidance/start.rs; src/planning/guidance/tests/drafting.rs; src/planning/guidance/opening/tests/drafting.rs; src/planning/guidance/start/tests/lookup.rs | common.md, server.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-openings.md |
| wire | Wire fields and fixtures (wave 1) | codex sol | src/plan_server/wire/chat.rs; src/plan_server/wire/kickoff.rs; src/plan_server/wire/tests_chat.rs; src/plan_server/wire/tests_schema.rs | common.md, server.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-wire.md |
| web-schemas | Web schemas and commands (wave 1) | codex sol |  | common.md, web.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-web-schemas.md |
| draft-store | Draft size, finished, landing, detail (wave 2) | codex sol | src/plan_server/chat/drafts.rs; src/plan_server/chat/drafts/store.rs; src/plan_server/chat/drafts/handlers.rs; src/plan_server/chat/drafts/encode.rs; src/plan_server/chat/tests_drafts.rs | common.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-draft-store.md |
| draft-delete | Confined delete of the grant's files (wave 2) | codex sol | src/plan_server/chat/drafts/delete.rs; src/plan_server/chat/drafts/delete_tests.rs | common.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-draft-delete.md |
| size-pinning | Size from the offer (wave 2) | codex sol | src/plan_server/chat/team.rs; src/plan_server/chat/kickoff.rs; src/plan_server/chat/kickoff_cmd.rs; src/plan_server/chat/kickoff_tests.rs; src/plan_server/chat/tests_dispatch/kickoff.rs | common.md, server.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-size-pinning.md |
| draft-commands | Set aside, Stop now, Continue, Delete commands (wave 2) | codex sol | src/plan_server/chat/draft_cmd.rs; src/plan_server/chat/draft_continue.rs; src/plan_server/chat/dispatch.rs; src/plan_server/chat/mod.rs; src/plan_server/chat/tests_dispatch/drafts.rs; src/plan_server/chat/tests_dispatch/draft_commands.rs | common.md, server.md, bridge.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-draft-commands.md |
| web-size | Size from the offer in the browser (wave 2) | codex sol |  | common.md, web.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-web-size.md |
| web-drafts | Notice, Stop now, Delete confirm (wave 2) | codex sol |  | common.md, web.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-web-drafts.md |

Outside the crate, the worker table cannot name paths (`loom plan verify` treats a `../` claim
as escaping the stage). `openings` also owns `plan-plugin/skills/kickoff/SKILL.md`; `wire` also
owns `web/src/plan/api/fixtures/chat-commands.json` and
`web/src/plan/api/fixtures/kickoff-drafts.json`; `web-schemas`, `web-size` and `web-drafts` own
the `web/src/plan/` files their briefs list, which are disjoint. The stage's `files:` covers them.

Waves, by what each unit needs on disk:

- **Wave 0, `repro` (Claude).** It reproduces "Set it aside does nothing" with a failing dispatch
  test and records the confirmed cause before any code changes. It stays on Claude because
  confirming a cause means running a fixture-backed test, which a codex unit cannot. If the cause
  is not the 600 s closing write, the orchestrator amends `unit-draft-commands.md` before wave 2.
- **Wave 1, six codex units:** `bridge`, `scope-size`, `safe-remove`, `openings`, `wire`,
  `web-schemas`. They write the interfaces the later units call.
- **Wave 2, six codex units:** `draft-store`, `draft-delete`, `size-pinning`, `draft-commands`,
  `web-size`, `web-drafts`. Each reads the wave-1 files it names on disk, and codes to the pins
  in the territory briefs for the wave-2 files it calls.

The crate does not compile between waves, because wave 1 changes signatures whose callers are
wave-2 files. Each codex unit compiles and fixes errors only in its own files; the orchestrator
runs `cargo build --all-targets` and `tsc -b` after wave 2. It sends each compile error to a
fresh gpt-6.2-sol unit for the territory that owns the file, re-split against the partial tree
when a unit timed out. Then it runs `cargo fmt --all`, `oxfmt`, each unit's proof command and
`cargo test --test maintainability`. Fixture-backed test failures go to ONE Claude
`loom-software-engineer` fix worker for all of them, because converging on those tests means
running them. A codex fix that fails twice moves to that Claude worker too. After every codex
run the orchestrator checks `git status --short` for writes outside the unit's files and for
any `git` or `.loom/` activity. The longest chain is three steps (repro, wave 1, wave 2), each
short.

Risk checklist walk:

- Untrusted input: the stored draft target and the browser's commands. Contracts
  `delete-refuses-non-grant-target`, `confirm-refuses-other-size`.
- Filesystem paths and symlinks: Delete removes files. Contracts
  `delete-removes-only-grant-files`, `delete-does-not-follow-symlinked-bundle`.
- Lifecycle and concurrency: ending sessions, a second end, restart. Contracts
  `set-aside-unconfirmed-ends-at-once`, `stop-now-tears-down-a-closing-write`,
  `end-within-bounds-the-closing-write`, `set-aside-draft-with-document-stays-listed`,
  `continue-resumes-grant-for-existing-target`.
- Reachability: every contract above drives `dispatch` or the bridge, the real entry points;
  the wiring entries below pin the consumer call sites.
- Process I/O volume, configuration propagation and external data: not touched (the
  `git cat-file` check runs once per existing path, at most three).
- The size fix itself: `feature-kickoff-keeps-feature-size`, `kickoff-opening-names-the-size`.

### 2. integration-verify

Full Rust suite, clippy, fmt and the web `check` script on the merged tree; read-only reviewers
for security (the delete path), the bridge end logic and the UI; a smoke run of the planner with
a feature kickoff, set aside, continue and delete.

### 3. knowledge-distill

Curates the stage memories into `doc/loom/knowledge/`, in particular the corrected "Kickoff
drafts" section of `architecture/planner-uploads-and-drafts.md` (landing rule, size, delete).

## Open questions

None. The planner spec was refined to this plan's behaviour before the run:
`doc/product/feat-planner/section_08.md` (the kickoff draft rules), `section_07.md` ("Kickoff"),
`section_09.md` (`loom-plan:kickoff`), `section_12.md` (Kickoff scope) and the kickoff entry of
`doc/product/feat-planner/index.md`. If the run changes a behaviour, the stage records it in
loom memory and a refine session corrects those passages.

---

<!-- loom METADATA -->

```yaml
loom:
  version: 2
  ratchet_files:
    - loom/maintainability-baseline.txt
    - doc/loom/knowledge/check-baseline.txt
  provision:
    - working_dir: "web"
      command: "test ! -e .npmrc && test ! -L .npmrc && bun install --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null"
  sandbox:
    enabled: true
    auto_allow: true
    filesystem:
      deny_read: ["~/.ssh/**", "~/.aws/**", "~/.config/gcloud/**", "~/.gnupg/**"]
    network:
      allowed_domains: ["crates.io", "index.crates.io", "static.crates.io", "registry.npmjs.org"]
      allow_local_binding: false
      allow_unix_sockets: []
  stages:
    - id: kickoff-fixes
      name: "Kickoff keeps its size; set aside and delete work"
      summary: "Pins a kickoff's size to its start offer, makes Set aside end a kickoff promptly with a Stop now, keeps unfinished drafts listed, and adds a Delete that removes a draft with exactly the files its kickoff wrote."
      stage_type: standard
      skills: ["loom-rust", "loom-react", "loom-typescript"]
      implementers: ["codex", "claude"]
      subagent_timeout_secs: 900
      working_dir: "loom"
      dependencies: []
      description: |
        Fix the three planner kickoff problems in doc/plans/IN_PROGRESS-PLAN-planner-kickoff-fixes.md
        (doc/plans/PLAN-planner-kickoff-fixes.md before the run renames it): read its Overview,
        Decisions and stage 1 section by line range. Every worker reads
        doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/common.md, then its own brief.
        Use parallel subagents and skills to maximize performance.
        PATHS: working_dir is loom/ because loom runs each Rust contract as cargo test <name> -- --exact
        from it. YAML paths are crate-relative (../ for plan-plugin/ and web/); brief paths are
        repository-relative. Web checks run from ../web.
        UNITS: start 12 codex units, each with gpt-6.2-sol at effort xhigh and a DISJOINT file set,
        plus one Claude worker. Hand each unit the absolute worktree path of its brief and the prompt
        "Your brief: <path>. Read it in full before anything else."; run loom check for the gate,
        then commit yourself. Codex units never run git and never touch a .loom/ path; check
        git status --short after every codex run and revert anything outside the unit's files.
        WAVE 0: repro (Claude loom-software-engineer, sonnet): it needs a fixture-backed test run to
        confirm the Set aside cause. If the cause it records is not the 600 s closing write, amend
        unit-draft-commands.md before wave 2.
        WAVE 1 (codex, started in one go): bridge, scope-size, safe-remove, openings, wire, web-schemas.
        WAVE 2 (codex, started in one go): draft-store, draft-delete, size-pinning, draft-commands,
        web-size, web-drafts.
        | Worker | Role | Tier | Files owned | Shared context | Brief path |
        | repro | Reproduce Set it aside (wave 0) | sonnet | src/plan_server/chat/tests_dispatch/set_aside.rs; src/plan_server/chat/tests_dispatch.rs | common.md, server.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-repro.md |
        | bridge | Bridge end bound and cause (wave 1) | codex sol | src/plan_session/bridge/session.rs; src/plan_session/bridge/ending.rs; src/plan_session/bridge/end_plan.rs; src/plan_session/bridge/commands.rs; src/plan_session/bridge/mod.rs; src/plan_session/bridge/recovery.rs; src/plan_session/bridge/harness.rs; src/plan_session/bridge/tests_stopping.rs; src/plan_session/bridge/closing_contract_tests.rs; src/plan_server/summaries/rebuilds/session.rs | common.md, bridge.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-bridge.md |
        | scope-size | Kickoff scope size and arming (wave 1) | codex sol | src/plan_session/scope.rs; src/plan_session/scope/arm.rs; src/plan_session/scope_tests.rs | common.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-scope-size.md |
        | safe-remove | No-follow tree removal (wave 1) | codex sol | src/fs/safe_fs.rs; src/fs/safe_fs/write.rs; src/fs/safe_fs_write_tests.rs | common.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-safe-remove.md |
        | openings | Openings, title, continuation, skill (wave 1) | codex sol | src/planning/guidance/opening.rs; src/planning/guidance/continuation.rs; src/planning/guidance/start.rs; src/planning/guidance/tests/drafting.rs; src/planning/guidance/opening/tests/drafting.rs; src/planning/guidance/start/tests/lookup.rs | common.md, server.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-openings.md |
        | wire | Wire fields and fixtures (wave 1) | codex sol | src/plan_server/wire/chat.rs; src/plan_server/wire/kickoff.rs; src/plan_server/wire/tests_chat.rs; src/plan_server/wire/tests_schema.rs | common.md, server.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-wire.md |
        | web-schemas | Web schemas and commands (wave 1) | codex sol |  | common.md, web.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-web-schemas.md |
        | draft-store | Draft size, finished, landing, detail (wave 2) | codex sol | src/plan_server/chat/drafts.rs; src/plan_server/chat/drafts/store.rs; src/plan_server/chat/drafts/handlers.rs; src/plan_server/chat/drafts/encode.rs; src/plan_server/chat/tests_drafts.rs | common.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-draft-store.md |
        | draft-delete | Confined delete of the grant's files (wave 2) | codex sol | src/plan_server/chat/drafts/delete.rs; src/plan_server/chat/drafts/delete_tests.rs | common.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-draft-delete.md |
        | size-pinning | Size from the offer (wave 2) | codex sol | src/plan_server/chat/team.rs; src/plan_server/chat/kickoff.rs; src/plan_server/chat/kickoff_cmd.rs; src/plan_server/chat/kickoff_tests.rs; src/plan_server/chat/tests_dispatch/kickoff.rs | common.md, server.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-size-pinning.md |
        | draft-commands | Set aside, Stop now, Continue, Delete commands (wave 2) | codex sol | src/plan_server/chat/draft_cmd.rs; src/plan_server/chat/draft_continue.rs; src/plan_server/chat/dispatch.rs; src/plan_server/chat/mod.rs; src/plan_server/chat/tests_dispatch/drafts.rs; src/plan_server/chat/tests_dispatch/draft_commands.rs | common.md, server.md, bridge.md, delete.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-draft-commands.md |
        | web-size | Size from the offer in the browser (wave 2) | codex sol |  | common.md, web.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-web-size.md |
        | web-drafts | Notice, Stop now, Delete confirm (wave 2) | codex sol |  | common.md, web.md | doc/plans/briefs/planner-kickoff-fixes/kickoff-fixes/unit-web-drafts.md |
        OUTSIDE THE CRATE (not in the table, which cannot name ../ paths): openings also owns
        ../plan-plugin/skills/kickoff/SKILL.md; wire also owns ../web/src/plan/api/fixtures/chat-commands.json
        and ../web/src/plan/api/fixtures/kickoff-drafts.json; web-schemas, web-size and web-drafts own
        the disjoint ../web/src/plan/ files their briefs list.
        Units are disjoint; workers never spawn workers; two units never write one file. The crate
        does not compile between waves (wave 1 changes signatures whose callers are wave-2 files);
        codex units fix compile errors in their own files only.
        Codex units relay surprises in their final report: record each with loom memory note.
        AFTER WAVE 2: cargo build --all-targets and (cd ../web) tsc -b; send each compile error to a
        fresh gpt-6.2-sol unit for the owning territory (re-split against the partial tree if a unit
        timed out). Then cargo fmt --all; cd ../web && ./node_modules/.bin/oxfmt; run each unit's
        proof command and cargo test --test maintainability. Send ALL fixture-backed test failures to
        ONE Claude loom-software-engineer fix worker (converging on them needs test runs); a codex
        fix that failed twice goes there too. Never add or raise a row of
        maintainability-baseline.txt; split the file instead (the briefs name the split for
        scope.rs, ending.rs and draft_cmd.rs).
        CONTRACT SURFACE (the contract session writes these from this text before any code):
        - Rust contracts in src/plan_server/chat/kickoff_fix_contracts.rs (a new #[cfg(test)] module
          of plan_server::chat, using the dispatch test harness in src/plan_server/chat/tests_dispatch.rs),
          src/plan_session/bridge/end_within_contracts.rs (a new #[cfg(test)] module of
          plan_session::bridge, using bridge/harness.rs), and
          src/planning/guidance/opening_size_contracts.rs (a new #[cfg(test)] module of
          planning::guidance, calling brief::common_brief as guidance/tests/drafting.rs does).
        - Public surface: ChatCommand::SetAsideKickoff { client_id, session, now: bool };
          ChatCommand::DiscardKickoff { client_id, session, delete_files: bool };
          ChatCommand::ConfirmKickoff { client_id, size, name } (unchanged);
          KickoffDraftView.size: KickoffSize; KickoffDraftDetail { draft, shape, messages,
          written: Vec<String>, committed: bool }; KickoffScope { size, existing, grant };
          SessionScope::kickoff(root, size); SessionScope::kickoff_resumed(root, size, name);
          ArmRefusal::WrongSize(KickoffSize); Bridge::end_within(id, mode, cause, bound: Duration);
          BridgeEvent::Ended { id, kind, outcome, cause: Option<EndCause> }; the openings
          "loom plan: Kickoff, feature." and "loom plan: Kickoff, standalone plan.".
          GET /api/kickoffs/<session> answers the detail with written and committed.
        - Keep every contract fn at most 50 lines and each contract file at most 400 lines and
          rustfmt-clean: cargo test --test maintainability scans src/ and a frozen file cannot be edited.
        MEMORY: record mistakes, decisions and surprises via loom memory immediately, subagents
        too; NEVER loom knowledge (implementation stage); NEVER Claude Code auto-memory.
      files:
        - "src/plan_session/**"
        - "src/plan_server/chat/**"
        - "src/plan_server/wire/**"
        - "src/plan_server/summaries/rebuilds/session.rs"
        - "src/planning/guidance/**"
        - "src/fs/**"
        - "../plan-plugin/skills/kickoff/SKILL.md"
        - "../web/src/plan/**"
        - "maintainability-baseline.txt"
      wiring:
        - source: "src/plan_server/chat/draft_cmd.rs"
          pattern: "remove_written("
          literal: true
          description: "Discard with delete_files removes the kickoff's files through the confined delete"
        - source: "src/plan_server/chat/draft_cmd.rs"
          pattern: "end_within("
          literal: true
          description: "Set aside of a confirmed kickoff ends within its own bound"
        - source: "src/plan_server/chat/team.rs"
          pattern: "SessionScope::kickoff("
          literal: true
          description: "The kickoff scope is written with the offer's size at start"
      acceptance:
        - "cargo build --all-targets"
        - "cargo clippy --all-targets -- -D warnings"
        - "cargo fmt --all -- --check"
        - "cargo test --lib plan_server::chat::"
        - "cargo test --lib plan_server::wire::"
        - "cargo test --lib plan_session::"
        - "cargo test --lib planning::guidance::"
        - "cargo test --lib fs::"
        - "cargo test --test maintainability"
        - 'rg -qF "Kickoff, standalone plan" ../plan-plugin/skills/kickoff/SKILL.md'
        - 'rg -qF "setAsideKickoff(store, draft.session, true)" ../web/src/plan/new/draft-view.tsx'
        - "cd ../web && ./node_modules/.bin/tsc -b --noEmit"
        - "cd ../web && ./node_modules/.bin/oxlint --deny-warnings"
        - "cd ../web && ./node_modules/.bin/oxfmt --check"
        - "cd ../web && ./node_modules/.bin/vitest run src/plan/api src/plan/chat src/plan/new src/plan/home src/plan/header"
      contracts:
        - id: feature-kickoff-keeps-feature-size
          file: src/plan_server/chat/kickoff_fix_contracts.rs
          scenario: "starts a kickoff through dispatch with offer kickoff, then feeds an assistant text item ending in a loom-kickoff block with size plan and name demo-work; reads the published KickoffView and GET /api/kickoffs"
          rejects: "a tracker or draft that takes the agent's size, so the view or the draft listing reads plan for a kickoff started as a feature"
        - id: confirm-refuses-other-size
          file: src/plan_server/chat/kickoff_fix_contracts.rs
          scenario: "starts a kickoff with offer kickoff, then sends confirm_kickoff with size plan and name demo-work as the holder"
          rejects: "an arm_kickoff that grants doc/plans/PLAN-demo-work.md because the browser asked for plan; the command must be refused and the session scope's grant stay None"
        - id: set-aside-unconfirmed-ends-at-once
          file: src/plan_server/chat/kickoff_fix_contracts.rs
          scenario: "starts a kickoff with offer kickoff, sends one user message, leaves a turn running, then sends set_aside_kickoff with now false before any confirmation"
          rejects: "a set aside that plans a FinalWrite end, so the session waits for the closing reply and the draft stays live past two seconds of harness time"
        - id: stop-now-tears-down-a-closing-write
          file: src/plan_server/chat/kickoff_fix_contracts.rs
          scenario: "confirms a feature kickoff, sends set_aside_kickoff with now false while a turn runs, then set_aside_kickoff with now true before the closing reply arrives"
          rejects: "a second set aside that is refused as the chat is closing or ignored, so the session is not torn down at the next end step"
        - id: set-aside-draft-with-document-stays-listed
          file: src/plan_server/chat/kickoff_fix_contracts.rs
          scenario: "confirms a plan kickoff named demo-work, creates doc/plans/PLAN-demo-work.md in the project, sets the kickoff aside and lets it end with cause Holder, then lists GET /api/kickoffs"
          rejects: "a landing rule that deletes the draft because its document exists although the session never finished"
        - id: continue-resumes-grant-for-existing-target
          file: src/plan_server/chat/kickoff_fix_contracts.rs
          scenario: "an ended, confirmed plan draft named demo-work whose doc/plans/PLAN-demo-work.md exists; sends continue_kickoff and loads the new session's scope"
          rejects: "a continuation that starts an unconfirmed kickoff (grant None) or that arm_kickoff refuses because the target exists, so the new session cannot write the half-finished document"
        - id: delete-removes-only-grant-files
          file: src/plan_server/chat/kickoff_fix_contracts.rs
          scenario: "an ended plan draft whose target is doc/plans/PLAN-demo-work.md, with that file, a.md inside the demo-work brief bundle directory, doc/plans/PLAN-other.md and b.md inside the other brief bundle directory present; sends discard_kickoff with delete_files true"
          rejects: "a delete that also removes a sibling plan or bundle, or that removes the draft but leaves PLAN-demo-work.md or its bundle behind"
        - id: delete-refuses-non-grant-target
          file: src/plan_server/chat/kickoff_fix_contracts.rs
          scenario: "an ended draft file under the runtime kickoffs directory whose stored target was edited to doc/loom/knowledge/INDEX.md, which exists; sends discard_kickoff with delete_files true"
          rejects: "a delete that trusts any relative path under doc/ in the stored target and removes INDEX.md"
        - id: delete-does-not-follow-symlinked-bundle
          file: src/plan_server/chat/kickoff_fix_contracts.rs
          scenario: "an ended plan draft for demo-work whose brief bundle directory (briefs/demo-work under doc/plans) is a symlink to a directory outside the project TempDir holding keep.md; sends discard_kickoff with delete_files true"
          rejects: "a recursive delete that follows the symlink and removes keep.md outside the project"
        - id: end-within-bounds-the-closing-write
          file: src/plan_session/bridge/end_within_contracts.rs
          scenario: "an authoring session in the bridge harness (final_write_bound 30 s) that the user wrote in and that never answers its closing message; calls Bridge::end_within with FinalWrite, Holder and a 1 s bound, then advances past 1 s and runs end_step"
          rejects: "an end_within that ignores its bound and waits for the configured 30 s final_write_bound"
        - id: kickoff-opening-names-the-size
          file: src/planning/guidance/opening_size_contracts.rs
          scenario: "builds the common brief for AuthoringStart::Kickoff with small false and with small true"
          rejects: "an opening that says only Kickoff for the feature offer, so the agent must guess the size"

    - id: integration-verify
      name: "Integration Verification"
      summary: "Runs the full Rust and web suites on the merged tree, reviews the delete path and the end logic, and smoke-tests a feature kickoff through set aside, continue and delete."
      stage_type: integration-verify
      skills: ["loom-rust", "loom-react", "loom-typescript"]
      implementers: ["claude"]
      working_dir: "."
      dependencies: ["kickoff-fixes"]
      description: |
        Final verification after kickoff-fixes. Verify FUNCTIONAL INTEGRATION, not just tests passing.
        Use parallel subagents and skills to maximize performance. NEVER Claude Code auto-memory.
        CONTEXT: read the plan's Overview, Decisions and stage 1 section by line range
        (doc/plans/IN_PROGRESS-PLAN-planner-kickoff-fixes.md), loom memory show --all,
        doc/loom/knowledge/INDEX.md and only the planner sections it points to
        (architecture/planner-uploads-and-drafts.md, architecture/planner-web-shell.md).
        BUILD & TEST (zero tolerance, fix ALL warnings and errors): run loom check integration-verify
        once (full Rust suite, clippy as errors, fmt, the web check script).
        CODE REVIEW: spawn parallel loom-code-reviewer subagents: (1) security of
        loom/src/plan_server/chat/drafts/delete.rs and safe_remove_tree_in_workdir (target derivation,
        symlinks, locked spellings, nothing beyond the grant's files); (2) the bridge end logic
        (end_within, the hardening of a second end, Ended.cause on every path including recovery);
        (3) the web notice and delete confirm (state while ending, no browser dialogs, accessibility).
        Fix ALL findings with ONE Claude loom-software-engineer for the round (loom-senior-software-engineer
        for a finding in the delete path or the bridge end logic); codex is not licensed on this
        stage. Fix or dispute every finding, never defer one.
        SUGGESTIONS: weigh every pending reviewer suggestion the signal lists; resolve each one
        implemented with loom memory resolve <id> --outcome implemented --reason <what changed>.
        FUNCTIONAL: prove the behaviour end to end with the dispatch-level tests the stage added
        (feature kickoff keeps feature; set aside unconfirmed ends at once; Stop now; unfinished draft
        listed; Continue resumes; Delete removes only the grant's files), and confirm in the built web
        bundle that draft-view.tsx renders Stop now and Delete (tsc -b and vite build in web/).
        Record discoveries to loom memory for knowledge-distill, including any knowledge file the tree
        now contradicts: loom memory note "stale-knowledge: <file>#<heading> claims X; the tree does Y".
        The planner spec (doc/product/feat-planner/ sections 07, 08, 09, 12 and index.md) already
        describes this behaviour and is NOT edited here; record any divergence in loom memory.
      acceptance:
        - command: "cargo test --manifest-path loom/Cargo.toml --all-targets --no-fail-fast"
          timeout_secs: 3600
        - "cargo clippy --manifest-path loom/Cargo.toml --all-targets -- -D warnings"
        - "cargo fmt --manifest-path loom/Cargo.toml --all -- --check"
        - command: "cd web && ./node_modules/.bin/tsc -b --noEmit && ./node_modules/.bin/oxlint --deny-warnings && ./node_modules/.bin/oxfmt --check && ./node_modules/.bin/vitest run"
          timeout_secs: 1200
        - command: "cd web && ./node_modules/.bin/tsc -b && ./node_modules/.bin/vite build"
          timeout_secs: 600
      files:
        - "loom/src/plan_session/**"
        - "loom/src/plan_server/**"
        - "loom/src/planning/guidance/**"
        - "loom/src/fs/**"
        - "loom/maintainability-baseline.txt"
        - "plan-plugin/skills/kickoff/SKILL.md"
        - "web/src/plan/**"

    - id: knowledge-distill
      name: "Knowledge Distillation"
      summary: "Records what this plan changed about kickoff drafts, set aside and delete in the knowledge base."
      stage_type: knowledge-distill
      working_dir: "."
      dependencies: ["integration-verify"]
      description: |
        Curate all stage memories into permanent knowledge. NEVER Claude Code auto-memory.
        SINGLE-AGENT: do NOT spawn subagents; memories are compact summaries, so keep code spot-reads narrow.
        START with loom memory pending --group (corrections, mistakes, decisions, other); read the
        plan's Overview and Decisions by line range (doc/plans/IN_PROGRESS-PLAN-planner-kickoff-fixes.md)
        and the knowledge sections it touches.
        CORRECTIONS FIRST: apply every stale-knowledge: memory in place with
        loom knowledge replace-section <file> "<heading>" "<body>", never loom knowledge update.
        Expected: the "Kickoff drafts" section of architecture/planner-uploads-and-drafts.md (the landing
        rule now needs a finished session; drafts store their size; set_aside_kickoff has now; discard
        deletes the grant's files) and any planner-web-shell.md text about the open-draft notice.
        Record the confirmed cause of "Set it aside does nothing" as a mistake entry
        (What happened / Why / Prevention / Fix) under the planner mistakes topic it fits.
        Then curate mistakes, patterns, decisions and conventions via loom knowledge update.
        TIER ROUTING: findings of about 40 lines or fewer go inline in the tier-1 file; larger ones go
        via loom knowledge update <category>/<slug> with a 2-4 line tier-1 summary and link. INDEX.md
        regenerates on every knowledge write.
        Update README/CONTRIBUTING only if they describe kickoff drafts; otherwise record WHY not in memory.
        SUGGESTIONS: record every unimplemented reviewer suggestion in concerns or its topic, then
        resolve it promoted, merged or discarded.
        RECEIPTS: every Note/Decision/Question taken into knowledge gets loom memory resolve <id>
        --outcome promoted|merged|discarded|deferred right after the write that used it; finish with
        loom memory pending --strict and resolve whatever it lists.
        LAST, if this stage removed structural issues, ratchet the baseline:
        loom knowledge check --write-baseline doc/loom/knowledge/check-baseline.txt
      acceptance:
        - "loom knowledge check --strict --baseline doc/loom/knowledge/check-baseline.txt"
        - "loom memory pending --strict"
      files: ["doc/loom/knowledge/**", "README.md", "CONTRIBUTING.md"]
```

<!-- END loom METADATA -->
