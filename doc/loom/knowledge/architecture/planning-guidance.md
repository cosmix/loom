# Planning Session Guidance
>

What a planning session is told: the `loom-plan` plugin, the brief, the opening message and the model pair. Rendered per session by `planning::guidance` (`loom/src/planning/guidance/`), installed into the chat bridge by `plan_server::guidance` (`loom/src/plan_server/guidance/`).

## Layout and one-source rules

`plan-plugin/` at the repository root holds `.claude-plugin/plugin.json` (name `loom-plan`), `skills/` (`brainstorming`, `vision`, `section`, `metaplan`, `plan`, `refine`, `kickoff`, `knowledge`, `summaries`, `review`) and `agents/` (`summary.md`, `overview.md`). The `review` skill (`loom-plan:review`, the Review the feature offer) records findings in `doc/plans/<f>/findings.md` and writes nothing else ([planning trace](planning-trace.md#findings-actions)). It is embedded as `PLAN_PLUGIN` by `loom/build/assets.rs` beside `CLAUDE_COMMANDS` and the skills; the walker admits the `.claude-plugin` dot-directory only under that root. `install_all` never installs it: a session gets it only through the per-session copy below (see "Loom Assets Outside ~/.claude Need a Plugin" in [guidance-channels-and-plugin-scope](../conventions/guidance-channels-and-plugin-scope.md)).

- The two pressure commands have no second copy: `plan-plugin/` has no `commands/`, and `plugin.rs` renders `commands/pressure.md` and `commands/address.md` from `CLAUDE_COMMANDS` (`pressure.md`, `address.md`). Both carry the sentence forbidding edits to the plan's `## Pressure tests` section.
- The `knowledge` skill quotes `commands/distill.md`'s routing list; `knowledge_skill_preserves_every_distill_routing_line` (`planning/guidance/tests.rs`) takes distill.md lines 14-19 by position, so a change to that list is a change to the skill. The skill states ``Do not run `loom memory pending` or `loom memory resolve`.`` because the user's message names what to record, and writes only through `loom knowledge update`, `replace-section` or `annotate`.

## Per-kind render table

`render_session_guidance(kind, project, runtime, session_dir, models)` checks `StartBlocks::current` first (a present `config-block.json` refuses the start with `start blocked: <reason>`, an unreadable one fails closed), then writes `<session dir>/plugin/` and `<session dir>/brief.md`. `preflight` refuses a destination that already exists; `write_new` uses `create_new`, mode 0600 and `O_NOFOLLOW`, directories 0700. Each launch gets a fresh session directory, so a partial write is safe only for that reason.

| Kind | Plugin content | Brief | Model and effort |
| --- | --- | --- | --- |
| `Authoring` | manifest, every `skills/*`, both agents with `model:` and `effort:` rewritten | `common_brief.md` + `## Starting point` + project state | spec or plan pair by `step_class` |
| `Summary` | manifest, `skills/summaries/*` only, both agents rewritten | `summary_brief.md` | the overview pair |
| `Pressure` | manifest, `commands/pressure.md`, `commands/address.md` | the caller's `brief` | the caller's `model`, `effort` |

`rewrite_agent` demands exactly one `model:` and one `effort:` line in the frontmatter and fails otherwise. The summary agent takes the document pair, the overview agent the overview pair. The launch passes `--model`, `--effort`, `--plugin-dir` and `--append-system-prompt-file` (`plan_session/launch/script.rs`); the opening message rides the bridge, not argv (`opening_on_argv: false`).

## `[plan]` authoring keys and step classes

`plan.spec_authoring_model|effort` and `plan.plan_authoring_model|effort` (defaults in [model-and-effort-config](../conventions/model-and-effort-config.md)) resolve per key, project tier then user tier then default (`GuidanceModels::resolve`). `step_class` (`start.rs`) maps a start to `Spec` or `Plan`: vision, vision-table, add-section, write-section and review offers, a refine of a path under `doc/product` and the knowledge and no-step starts are `Spec`; metaplan, write-plan, add-plan, new-round offers and a refine of any other path are `Plan`; a `Message` start is `Plan` only when its first attachment lies under `doc/plans`.

## The brief's starting-point block

`brief.rs::starting_point` is derived from the rendered opening message: label (`loom plan:` and a `step` prefix stripped), target line, and the skill named by `Load the skill <skill>,`. `AuthoringStart` (`planning/guidance/start.rs`) has four variants, `Kickoff`, `Offer`, `Refine` and `Message`; a start needs an offer or typed text (`plan_server/chat/dispatch.rs`), and the block always reads the skill line. A `Message` start renders `Step: any, Refine.` and `Skill: loom-plan:refine.` with no `Target:` line, because it carries attachment paths and `opening_message` returns an empty string for it (the caller supplies the user's text). `project_state` then lists vision (`absent` or counts), metaplans, plan counts by state and the locked plan files (`IN_PROGRESS-PLAN-`, `DONE-PLAN-`, `RETIRED-PLAN-`). `common_brief.md` copies the seven-bullet conventions section of the product spec verbatim, heading and introduction included.

## Questions and background work in the brief

Every subagent the session spawns runs in the background, and so does a shell started with `run_in_background`. An agent reports with an `<agent-message>` hand-back and a `<task-notification>`, a shell with a `<task-notification>`; each opens a turn that is not the user's reply. `common_brief.md` has a section "Questions and background work" for this: ask a question last in a reply and end the turn; in a hand-back or notification turn do only what it calls for and never ask a waiting question again or call it unanswered; when the user's reply skips a waiting question, answer what they wrote and name the skipped one at most once. The bridge does not confirm a delivery with such a prompt unless its text equals the pasted message ([Delivery](planning-session-bridge.md#delivery), [the mistake](../mistakes/planner-ui-repair.md#a-question-was-reported-unanswered-after-a-background-turn)).

## The turn-end duty

An authoring turn that edited documents ends with `loom plan check` and nothing more (`common_brief.md` "The turn-end duty"); the `vision`, `section`, `refine` and `plan` skills repeat "Do not rebuild summaries at the end of the turn". The summaries a turn made out of date are rebuilt by the server's background scheduler once their inputs have stayed unchanged for `QUIET_PERIOD` (5 minutes, counted from the turn's end for edits made inside a turn), or at once on the user's Rebuild press ([Summary Store](summary-store.md#rebuilds-and-cost)). The authoring plugin still ships `agents/summary.md` and `agents/overview.md` (pinned by `tests/planning_guidance_contracts.rs` `authoring_plugin_carries_skills_and_agents_only`) for the one rebuild a chat makes itself: a summary sentence that misstates a right document, rebuilt through `loom plan summary brief --note` and `seal` (product spec section 10, "When the summary is wrong"). `Bash(loom plan summary *)` is pre-allowed for the Open and Team scopes for that path and for status checks (`plan_session/sandbox/permissions.rs`).

## Server installation and brief route

`plan_server::routes` installs `guidance::routes()` and calls `guidance::install(ctx)` after chat installation. `install` takes the chat `GuidanceSlot` from the extensions, resolves `GuidanceModels::load` once (a missing slot logs and leaves the `ChecklistGuidance` default), builds one `PlanGuidance`, `set`s it into the slot and also inserts it into the typed extensions, so the bridge and the `GET /api/session/brief?session=<id>` route (viewer role) share one map of rendered `SessionBriefView`s. An unknown or missing session answers 404 `unknown_session`. Only `Authoring` starts reach `render_session_guidance` through `PlanGuidance::authoring`; the summary and pressure kinds are rendered by plans 10 and 11 (see [planning-guidance-followups](../concerns/planning-guidance-followups.md)).
