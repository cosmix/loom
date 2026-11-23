# Planner Redesign

>

## Offer Ownership Was Keyed on the Page, Not on the Offer's Id

**What happened:** the spine hid an offer nothing else drew while two surfaces drew another. Contracts and journey J6 passed.

**Why:** the stage decided who draws the primary offer by which page was showing; each contract seeded the one case where the pace choice holds the primary.

**Prevention:** decide ownership by comparing offer ids, and test a primary that differs from what the page draws. Read a contract's seeds for the cases it leaves out.

**Fix:** `paceOfferIds` and `pageDrawsPrimary` (`feature/page-offer.ts`) compare ids ([Planner Feature Screens](../architecture/planner-feature-screens.md#offers-have-one-owner-each-decided-by-id)).

## Page Text Was Accepted Without Reading It Against Live State

**What happened:** the Pressure page's sentence used `checklist.untested`, which counts written plans only, so a running untested plan read "Every plan has a pressure test on record" above its own "Not tested". The Run page was accepted with run state read once on mount, a failed request read as "Nothing is running", and Start enabled while a run held the workspace. Unit tests and contracts passed; the review caught the Run page.

**Why:** fixtures seeded the atoms, so nobody read the sentence against a real server; the brief treated `runStateAtom` as a value that stays current, and the server sends no `run_state` frames.

**Prevention:** read each page's screenshot against its own rows before accepting a page worker. A brief for a page that reads a workspace atom names what refreshes it and what a failed read says.

**Fix:** the lede counts live plans with a date; `useRunRead` reports a failed read and the Start control reads the run state ([screens](../architecture/planner-feature-screens.md#run-and-findings-rows)).

## A View Was Derived From Data the User Edits

**What happened:** the concluded-chat fix swapped the composer view by the draft text. Review found it unmounted the focused box and let a stale draft start a chat on Enter.

**Why:** the view followed data the user types into.

**Prevention:** key a view swap to user intent (the session they chose to write after), which survives edits to the draft.

**Fix:** the swap keys on the chosen session.

## Integrity Disputes Claimed Guards That Were Not Followed Through

**What happened:** two disputes dropped guards that still protected tree behaviour (the "Nothing to do next" sentence and active or optional marks staying distinct from summary glyphs), and another claimed the `api_error` tone guard was kept when `transcript.test.tsx` pinned only the class every notice gets. The adjudicator rejected the last.

**Why:** guards were audited against removed features without checking that each dropped assertion's behaviour was also gone, and a guard that moved from a class to a CSS selector was not followed to the rule it depends on.

**Prevention:** for each removed assertion name its successor test or the deleted behaviour before filing. When a guard moves to a selector, pin the attribute and the rule through `?raw`.

## An Acceptance Run Scoped to One Directory Hid Consumer Breakage

**What happened:** the chat stage's acceptance vitest run covered `src/plan/chat` and `src/plan/attachments`, so four consumer tests broke unseen until the impact step: `help/chat-scene`, `files/doc-heading-view`, `concept/concept-route` and a context line that repeated "Engineering overview".

**Why:** nobody ran the wider `src/plan` suite after shared chat parts changed. A fix brief also dictated a selector (scope "Engineering overview" to the reading column, then to the first `.plan-eyebrow`) without reading `concept-route.tsx`; the eyebrow sits outside the column and is not first on the page.

**Prevention:** run the consumers' tests before the review round, and read the rendering site before dictating a query in a brief.

## A Journey Check Was Written Before the Layout Settled

**What happened:** J1 measured `main` after the spine moved inside it, so the spine's text met the 200-character bar, a menu toggle defeated the stale-page check, and `/pressure` read as content at 390px. `expandGroups` also matched a hidden "Documents" menu button.

**Why:** the journey stage wrote its checks while the shell stage was still deciding the layout.

**Prevention:** a journey that judges page content measures the routed page, and a difference between widths for one route is a defect to chase.

## Codex Units Reported Success Over Unchanged or Reformatted Files

**What happened:** unit B2 reported `succeeded` yet left `sandbox/assert.rs` unchanged, so auto-mode settings would have failed the self-check; other units ran rustfmt without the crate config and rewrote unrelated lines in `driver.rs`, `bridge/mod.rs` and `chat/mod.rs`.

**Why:** the forward evidence trailer proves the run ended, not that the owned file changed.

**Prevention:** after each wave diff every owned file against the brief, not just `git status`; fixes go to a Claude lane. Brief units to run no formatter and run `cargo fmt --all` after the wave. A test that spawns the `loom` binary goes through the sanctioned spawner: a raw `env!(CARGO_BIN_EXE_loom)` spawn passed its narrow check and failed `binary_spawn_guard` in the full suite, so the brief names `tests/integration/binary_spawn_guard.rs` and the `LOOM_*` scrub.

## A Server Announced Its URL Before Installing Its Stop Handler

**What happened:** `plan_server::run` printed the URL and published the lock before `serve_until_stopped` installed the signal handler, so a SIGINT in that window killed the server with no drain; a test failed with `signal: 2` under gate load.

**Prevention:** a server installs its stop handler before it announces anything a client can act on. `install_stop_handler` now runs first ([plan server](../architecture/plan-server.md)).

## Tooling Slips

- **Piped or backgrounded gates.** `loom check` was run through `run_in_background` twice more (the relay hook reads only a foreground call's stdout, so the request stayed "pending relay" and the next call answered `refused`), and cargo output was piped through `rg`, `tail` and `cut` twice against a brief that forbade it. This repeats [the live-state entry](live-state-pollution.md); the hook proposal is in [concerns](../concerns/code-quality-and-hook-debt.md#no-hook-refuses-a-relayed-loom-command-run-in-the-background-or-through-a-pipe).
- **A full gate after a fix round.** One full `loom check` runs beside the final review and failures are re-checked with `--criterion <n>`. The stage-completion command reuses each pass cached for a byte-identical tree (`host_verify/tests_child_reuse.rs:50`), but the cache key hashes the whole repo manifest (`verify/criteria/cache_fingerprint.rs:89`), so any edit, prose included, voids every pass; the waste is a check run beside a review that returns findings. At a final gate, implement only suggestions that fix a defect.
- **Guessed paths.** Four searches named a module or glob that does not exist (`tmux/native.rs`, a recovery glob, permission directories), and zsh rejects an unmatched glob. Locate with `loom map --find-all` or `rg --files` first.
- **A subagent ran `git rm --cached`** to delete a tracked file; subagents delete with `rm` and leave the index to the main agent.
- **Restoring mutated source with `shutil.copy2`** kept the old mtime, so cargo reused the mutated test binary and a racy test looked flaky. Touch files after a restore and check the run printed `Compiling`.
- **A consumer imported a private re-export** (`daemon::protocol::Response`); use `daemon::Response`.

## Visual feedback met by contract tests while the look stayed bland

**What happened**: planner-polish stage V2 was to restyle the Settings summary statistics (user issue 12). Its acceptance tests only checked that the Token totals region exists and the limit meters are gone; the stage passed and the user found the block as bland as before. The same review also found summary sentences that share one table each expanding into the same passage, so the table showed twice.
**Why**: A visual request was turned into DOM-presence criteria, so nothing tested the look; and the summary passage unit (a heading section per sentence) was never checked against sentences that cite the same section.
**Prevention**: A visual-quality item needs a design brief stating the concrete treatment (layout, type, colour, motion) and a screenshot check by the stage before completion. A summary expansion shows each source passage once: sentences in one paragraph that cite the same section open as one unit.
**Fix**: The plan planner-follow-ups drew the statistics as two figure cards (stacked bar, legend, count-up) from a design brief and checked them by screenshot at 1440px, where a container-query slip showed ([the slip](planner-follow-ups-delivery.md#a-container-query-was-put-on-its-own-container)); consecutive sentences that open to the same content merge into one fragment that opens the table once ([reading column](../architecture/planner-web-shell.md#reading-column)).

## The chat sidebar redesign shipped without visible change

**What happened**: planner-polish V3 gave a fable worker with frontend-design the whole chat sidebar redesign (issue 15). The user then found the chat unchanged in look; the ⋯ menu still held End chat, the idle row still read 'Waiting for you', and New chat still started a session that spent 51.8k tokens on a 'no step chosen' opening before the user typed.
**Why**: The brief named the goal ('radically improve') without a concrete treatment or a before/after screenshot gate, and New chat's eager session start was outside the visual worker's files, so nobody owned its cost.
**Prevention**: A redesign starts from a mockup the user approves; the stage gates on screenshots matching it. A chat starts a model session only when the user sends the first message.
**Fix**: The plan planner-follow-ups built the chat from the approved `alt-d-hearth` mockup: no menu, no idle line, New chat starts no session, one roundel per pane and a designed ending ([Hearth head and states](../architecture/planner-web-shell.md#chat-home-and-the-chat-panel)). Integration-verify compared the five states with the mockup's frames, and a contract pins each rule (`new-chat-starts-no-session`, `live-chat-has-no-menu`, `idle-state-line-is-empty`, `one-roundel-per-pane`).

## Mockup briefs named a retired theme

**What happened**: The mockup briefs for the overview headers, findings page and settings modal told three design agents the dark theme was walnut. Walnut is retired; the stored name resolves to Aubergine. Two agents drew Aubergine and labelled it walnut, one drew Graphite, and the main agent reported the mix back as if the themes matched. The index pages also opened on PNG grids, though the user wanted HTML to inspect.
**Why**: The brief named the themes from memory, without checking `web/src/plan/theme-default.ts` and `aurora-ui/shared/styles/tokens-dark.css`.
**Prevention**: A mockup brief takes its theme names from `theme-default.ts` (light: ledger; planner dark default: Graphite, `data-dark-theme="gray"`) and says the deliverable is live HTML, with screenshots only for the agent's own check.
**Fix**: The findings and settings mocks were rethemed to the Graphite tokens, and each index opens the live HTML directions.
