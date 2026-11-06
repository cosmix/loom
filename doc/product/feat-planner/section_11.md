# 11 — Handoff to Execution

Part of the [Loom Plan product vision](index.md).

This section defines how the app runs a pressure test, the record a pressure test leaves in the
plan, how the app starts `loom init` and `loom run`, how it hands monitoring to `loom status
--web`, what it shows while plans run, and what it offers once they are done. Section 01 defines
the states these actions move the project through; section 07 the sessions they run in.

## The model in one paragraph

Pressure testing and starting a run are buttons, never automatic, and each is the user's act.
A pressure test runs the same pipeline as `loom pressure`, with the server as the driver: its
Claude steps are sessions in tmux that the chat shows, its Codex step a background process, and
at the end the driver appends a line to the plan's `## Pressure tests` section. Starting a run
executes `loom init` and `loom run` as child processes, after showing what they require and
fixing what the app can fix. From then on the daemon owns the run, the dashboard shows it, and
the planning app shows plan states from file names, a one-line run summary, and a link to the
dashboard. Plans that finish bring the project to the returning author's offers.

## Pressure testing from the app

### Where it is offered

Pressure testing is optional (section 01, "The six steps"). The app offers it:

- on every written plan's page, `/f/<feature>/engineering/<slug>`, as a **Pressure-test** button, whether or not the
  plan was tested before (`run/plan-run-actions.tsx`). The page draws the offer for its own plan, so the spine's
  next-step line shows its sentence only there;
- in the spine's next-step line, when the page is not that plan: the first untested written plan's offer is
  the step's offer, beside the offer to start a run. The spine has no Pressure test row and the app has no
  Pressure page. Every plan row in the spine except a retired plan's carries a badge, "Pressure-tested <date>" or "Not pressure-tested",
  and a tested plan's record is the `## Pressure tests` section in file mode. A feature with no plan gets one
  sentence naming what comes first.

It is offered only for plans in the written state, `<plan home>/PLAN-<slug>.md`. The plan home is `doc/plans/<feature>/`
for a feature, or `doc/plans/` for a standalone plan; its brief bundle is `<plan home>/briefs/<slug>/`.
Running, done and retired plans are locked (vision, Decisions, "Locked plans"). One pressure run at a time per project.

The offer is disabled, with the reason, when:

- the `codex` CLI is missing (`find_codex_path`, `loom/src/codex.rs`) or Codex's `pressure` skill is not installed under
  `$CODEX_HOME/skills/pressure/` (default `~/.codex`): `loom pressure` needs both lanes, and the app does the same
  (`loom/src/commands/pressure/mod.rs:349`). (My call: a Claude-only test is a different, weaker test, and the record would have to say so.)
- the server is in read-only mode (section 02, "Missing prerequisites");
- the browser does not hold authoring, or another browser does (section 02).

### The dialog

Starting a pressure test opens a dialog with a hero row: the still pressure-tested flask in a tinted badge, "Pressure-test a plan",
and "Two reviewers read the plan at once, then an editor addresses both reviews."
The plan card carries its DocMark, title, file name, state chips, and "Change" to choose another plan.

"What happens" starts with the rounds control: 1/2/3 quick picks and a number field, default 1, as `loom pressure --rounds`.
Below it, a dashed "Each round, N times" frame draws one round: Review (Claude `/pressure`) and Second review
(Codex `$pressure`, marked "background") both connect to Address (Claude `/address`). Each node carries its model and
effort pickers, set to the values `[pressure]` configuration resolves (`loom/src/commands/pressure/models.rs:44`) and
changeable for this run from the same vocabularies the CLI flags accept. A note below the frame says "The record line is
written at the end." (My call: the CLI exposes these as flags, so the app exposes them per run.)

The consequences sit in two columns: estimated time grows with the chosen rounds, with no minute figure; the run edits
the plan and its briefs; it writes `planning-omissions.md` at the repository root; and, when an authoring chat is open,
it ends that chat first with its closing write (section 09), in the warning tone. The footer says "Holds authoring until
it finishes", followed by "Cancel" and "Start pressure test".

A pressure run edits the plan, so it takes the project's authoring slot (vision, Decisions, "One authoring session per
project") and holds it until the run ends; a step's session ending does not release it. With an authoring chat open,
starting ends the chat first with its closing write (section 09). (My call.)

### How the server runs it

The pipeline is the one `loom pressure` runs (`plan_steps`, `loom/src/commands/pressure/mod.rs:78`):
per round, delete the Codex report, run Claude `/pressure` and Codex `$pressure` at once, then
Claude `/address`; after the last round, write the record, then delete the report. The step list,
the model resolution, the report path and the record writer move out of `commands/pressure/` into
a module both drivers use; each driver keeps only its way of running a Claude step. (My call:
one pipeline, so the app and the terminal cannot drift.)

| Step               | Terminal driver (unchanged)                     | App driver                                                     |
| ------------------ | ----------------------------------------------- | -------------------------------------------------------------- |
| Claude `/pressure` | foreground `claude` on the user's terminal      | a **pressure session** through the session bridge (section 07) |
| Codex `$pressure`  | `codex exec` in the background, output to a log | the same process, the same log, under the session directory    |
| Claude `/address`  | foreground `claude`                             | a second pressure session                                      |
| Completion         | a marker file the agent creates, then SIGTERM   | the same marker, under the session directory; then teardown    |

A **pressure session** is the fourth session kind in section 07's table. It starts as section
07 describes, with these differences:

- The opening message is the command, `/loom-plan:pressure <plan path>` or
  `/loom-plan:address <plan path>`, carried by the session's plugin from the same command files
  the terminal uses (section 09, "The plugin"). The plugin copy matches the binary, so a stale
  `~/.claude/commands/` cannot change what the app runs.
- The brief is `loom pressure`'s completion instruction (`completion_instruction`,
  `loom/src/commands/pressure/spawn.rs:21`) with the marker path under `<session dir>/done/`, and
  nothing else: the commands carry their own procedure. The instruction's text says the marker
  sits in `.loom/work/`; the shared pipeline makes that explanation follow the marker path it is
  given, so the app's sessions are not told a wrong location.
- Model and effort are the run's `/pressure` or `/address` slots.
- The launch environment adds `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`, as the terminal driver
  does (`AGENT_TEAMS_ENV`, `loom/src/claude/session.rs:12`), since `/pressure` uses an agent team;
  the session's settings keep teammates inside the session's own process (Claude Code's
  `teammateMode`, `in-process`) so they open no tmux panes the bridge does not watch. The
  implementing stage confirms that setting against the installed Claude Code.
- The session ends when the marker appears: the server tears the tmux server down
  (`teardown_socket`, section 07), with no closing message. A pressure session has nothing to
  record beyond the plan edits it already made.
- Permission mode and write rules come from section 12 ("Pressure sessions"): the session may
  edit the plan under test and its brief bundle, write `planning-omissions.md` at the repository
  root, and create its marker under `<session dir>/done/`; every other plan, locked plans and
  their brief bundles included, stays read-only. It runs in auto mode, as the terminal driver does
  (`--permission-mode auto`, `loom/src/commands/pressure/spawn.rs:38`): allow rules run, a rule
  that asks reaches the holder in the chat, and an action the classifier blocks shows as a notice.

The Codex process is a child of the server, with its process identity recorded in the run's
session record so a later server can find it (below). Codex runs headless (`codex exec`) under the
user's Codex login; principle 7 forbids headless Claude, and no Claude session here runs headless.

### What the chat shows

A pressure run has no strip under the app header. The activity popover from the header notification pill carries the
run's row, titled "Pressure test · round x of y": the plan's file name and the current step (`/pressure`, `$pressure`,
`/address`), the elapsed time, and a determinate progress bar counting the finished steps.
The plan's header carries "Being pressure-tested" and locks Edit. In the chat, each running step (Codex, `/pressure`,
`/address`) turns its own rotating arrow, even when two run at once; a finished step shows its done glyph and a step not
yet started a quiet still glyph. The Codex card's tile (a magnifier) and the run header's flask are still glyphs. Motion
stops when work ends and freezes with `prefers-reduced-motion`.

The run occupies the chat panel the way an authoring session does. Its run header names the plan, round and step, with
elapsed time, tokens and "Stop". The step timeline groups each round's Codex and Claude `/pressure` side by side,
then Claude `/address`; the record line follows the last round:

- each Claude step is a run of turns rendered like any session's (section 08), with its tool activity and its permission requests;
- the Codex step is one status item, whose tile is a still magnifier while it reviews: the model actually
  running (after any fallback, `loom/src/commands/pressure/fallback.rs:54`), the elapsed time, and on finish either
  "Review written" with a link to the `codex-<plan file>` report or "No review", with the log's tail on a failure, labelled "Codex failed";
- each step lists the plan edits it made with their diffs (section 07's changed-file list);
- token counts per Claude step come from the transcripts, as for any session, and include the step's teammates: the bridge
  reads each teammate's transcript for its token counts only and renders none of its messages; Codex tokens are not shown.

The input box stays open to the browser holding authoring: the terminal driver lets the user type into the foreground
session, and so does the app. The stop button stops the whole run (below). Read-only browsers watch the same chat.

On recorded completion, a "Summary of this pressure test" card shows rounds, duration and tokens, every edited file with
its diff, "Review the changes", and "Test the next plan". The latter opens the dialog for another eligible plan, or is
disabled with the reason when none remains. The plan header shows "Pressure-tested today" for a record dated today;
the record follows the placement rules below.

The Codex report (`codex-PLAN-<slug>.md`, `codex_report_path`, `loom/src/pressure/paths.rs:77`) sits beside the plan in its
plan home while the run lasts, so file mode lists it under other documents; the run deletes it at the end, as the terminal does.

### Stopping, failure, and server stops

- **Stop button**: the server denies a pending permission request, interrupts the running Claude
  step, tears its session down, terminates the Codex process, deletes the Codex report, and writes
  no record. The plan keeps whatever edits the steps made. (My call: a partial pressure test is
  not a test, as section 01 says.)
- **The holder goes away**: when the authoring browser has been gone for the grace period
  (section 07), the run is stopped as by the stop button, and the header says "Stopped: the
  holder was away for 10 minutes". Nobody is left to answer its permission prompts.
- **A step fails** (a session dies, Codex exits with an error the terminal driver treats as
  fatal, `should_stop`, `loom/src/commands/pressure/spawn.rs:129`): the run stops the same way and
  the chat says which step failed and why.
- **`loom plan` gets Ctrl-C**: the run is stopped as by the stop button, before the server
  waits on other sessions' closing writes.
- **The server dies**: the next `loom plan` finds the pressure session's record (section 07,
  "Crashes and kills"), tears down a live Claude session, terminates a Codex process whose
  identity verifies, deletes the report, and prints that the run on that plan was interrupted.

After the run ends, by any path, the plan's summary is out of date and goes to the background
rebuild queue (section 04). While the run lasts, the summary scheduler starts no background
session, as during an authoring turn. (My call: the plan changes several times during a run, and
one rebuild at the end costs less than one per step.)

### The pressure record: a change to `loom pressure`

Section 01 defines the record; this is the change that writes it. It lives in the shared pipeline,
so a terminal run and an app run write the same line.

**When.** After the last round's `/address` step finishes and no step asked the pipeline to stop.
A run that stops early writes nothing.

**The line.**

```markdown
- 2026-10-26: Claude opus, Codex gpt-6.1-sol; 2 rounds, review addressed
```

- the local date, `YYYY-MM-DD`;
- `Claude <model>`: the `/pressure` slot's model;
- `Codex <model>`: the model that ran after fallback (`model_used`); when rounds ran different
  models, each in round order, joined with ` / `;
- `<n> round` or `<n> rounds`;
- the outcome: `review addressed` when every round's Codex report existed when its `/address` ran;
  otherwise `Codex review missing in round <k>` (comma-separated when several), since `/address`
  changes nothing without a report (`commands/address.md:8`) and that round's only review was
  Claude's.

Section 01's examples use this format. Detection needs only a line under the heading, so the
round count and the outcome add information without changing the rule.

**Where.** In the plan file, after the `<!-- END loom METADATA -->` marker:

1. If a `## Pressure tests` heading follows the marker, append the line after the last line of
   that heading's section, its list items included.
2. Otherwise append, at the end of the file, a blank line, `## Pressure tests`, a blank line, and
   the line.
3. A plan with no end marker is handled by rule 2.

The write is a read, an append in memory, and a temp file renamed over the plan in the same
directory, keeping the file's mode. If the plan file is no longer at the path the run started
with (someone renamed or moved it during the run), the driver prints a warning naming the line it
would have written and writes nothing. The terminal driver prints the line it wrote; `--dry-run`
lists the record step.

**Validation.** Plan parsing reads only the metadata block (`loom/src/plan/parser/extraction.rs:75`)
and brief-path scanning finds no `<plan home>/briefs/` path in a record line, so a recorded plan
parses as before. The change carries a test that runs `loom plan verify` and `loom init`'s plan
preflight on a plan with a `## Pressure tests` section after the end marker.

**The commands.** `commands/pressure.md` and `commands/address.md` gain one sentence each: never
edit the `## Pressure tests` section. Both tell the agent to edit the plan in place, and the
record is the driver's.

## Starting a run

### What loom requires

`loom init <plan>` and `loom run` refuse unless these hold. The app reads them before it runs
anything, so the user sees every problem at once, and shows the commands' own output when they
still refuse.

| Requirement                                                                                                     | Where loom checks                                                                                                               |
| --------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------- |
| No initialized workspace holding a run, unless `--clean` is given                                               | `loom/src/commands/init/execute.rs:126`                                                                                         |
| No modified or staged tracked files (untracked files are allowed)                                               | `check_for_uncommitted_changes`, `loom/src/commands/run/checks.rs:217`; `loom/src/git/branch/status.rs:23`                      |
| The plan and every brief it reads committed                                                                     | `require_committed_plan`, `loom/src/commands/run/plan_inputs.rs:17`                                                             |
| The current branch is the plan's base branch                                                                    | same function                                                                                                                   |
| `jq`, Claude Code logged in, git 2.40 or later, the sandbox prerequisites, signing when `commit.gpgsign` is set | `prepare_repo_for_run` (`loom/src/commands/run/checks.rs:16`) and `run_startup_preflights` (`loom/src/commands/run/mod.rs:148`) |

Nothing requires a daemon to be running: `loom run` starts it (`execute_background`,
`loom/src/commands/run/mod.rs:45`), and the daemon is its own process group, independent of
`loom plan`'s lifetime. `loom run` renames the plan to `IN_PROGRESS-` and commits the rename
itself (`mark_plan_in_progress`, `loom/src/commands/run/plan_inputs.rs:56`).

### The workspace's state

The run offer depends on what `.loom/work` holds:

| `.loom/work`                                                | Offer                                                                    |
| ----------------------------------------------------------- | ------------------------------------------------------------------------ |
| absent                                                      | Start a run: `loom init <plan>`, then `loom run`                         |
| initialized, its plan still `PLAN-` (init ran, run did not) | Start the run for that plan: `loom run`; or another plan, with `--clean` |
| its plan `IN_PROGRESS-`, daemon running                     | No run offer; the plan is running; open the dashboard                    |
| its plan `IN_PROGRESS-`, daemon stopped                     | Resume: `loom run`; open the dashboard                                   |
| its plan `DONE-`                                            | Start the next plan: `loom init <plan> --clean`, then `loom run`         |

The app reads the active plan with the function the daemon uses (`get_plan_source_path`,
`loom/src/fs/plan_lifecycle.rs`) and the daemon's liveness with `DaemonServer::is_running`.
A running or stopped unfinished plan also gets "Abandon run" (below).

One plan runs at a time per project, as `loom init` allows one active plan per workspace.

### Abandoning an unfinished run

"Abandon run" appears beside "Resume" and "Open dashboard" while the workspace holds a plan that
is `IN_PROGRESS-`, running or stopped. Only the browser holding authoring may use it, and not in
read-only mode.

**First confirmation.** A dialog names what is lost: the plan file moves to
`<plan home>/archive/RETIRED-PLAN-<slug>.md` and closes its metaplan entry; the daemon stops; every
running stage session is killed; `.loom/work` (stage state, memory journals, the run's records)
is deleted; every stage worktree under `.worktrees/` and its `loom/*` branch is removed. It lists
the stages merged so far, whose work stays on the base branch, and the stages that are not. The
user confirms by pressing "Abandon run" in the dialog.

**Unmerged work.** Before running anything, the server lists the `loom/*` branches with commits
ahead of the run's base branch, as `loom clean` does (`confirm_branch_deletion`,
`loom/src/commands/clean/worktrees.rs:71`; a git error counts as unmerged). When there are any,
the dialog shows each branch with its commit count and asks a second, separate confirmation
naming each branch and its commit count; the request carries that list, the server re-checks it
before `loom stop` and again after it, and stops with the new list if it changed:
"Delete N branches with unmerged commits". Declining cancels the whole abandon and changes
nothing; the dialog says the user can merge or push those branches first and come back.

**The commands**, as child processes the way "Start" runs them, stopping at the first failure:

1. `loom stop`, which stops the daemon.
2. `loom clean --all`, which removes worktrees and their branches, kills loom sessions, and
   deletes `.loom/work`. The server runs it with stdin closed, so `loom clean` would refuse to
   delete unmerged branches (`loom/src/commands/clean/worktrees.rs:128`); the server sets
   `LOOM_CLEAN_YES=1` only after the second confirmation, or when the listing found no unmerged
   branch. `loom clean` also refuses while the target branch holds a move loom did not accept
   (`refuse_unreviewed_move`, `loom/src/commands/clean/mod.rs:99-101`); the dialog shows that
   refusal and stops.
3. The rename: the server moves `<plan home>/IN_PROGRESS-PLAN-<slug>.md` to
   `<plan home>/archive/RETIRED-PLAN-<slug>.md` in the same feature or standalone home, with a file
   rename, and commits nothing (`RunningPlan::source` and `RunningPlan::retired`,
   `loom/src/plan_server/run/abandon/plan.rs`). The rename shows under "Uncommitted changes" in the
   next run dialog, and the user commits it there.

`loom clean` exits 0 when it declines to delete (`loom/src/commands/clean/mod.rs:60-64`), so the
server renames the plan only after confirming `.loom/work` and the run's worktrees are gone; if
they remain, the dialog shows `loom clean`'s output and the plan keeps its `IN_PROGRESS-` name.

Before `loom stop`, the app writes an abandon marker for the plan in the runtime directory. If the
server stops after `loom clean` removed the run but before the rename, the next `loom plan` finds
that marker with an `IN_PROGRESS-` plan, no workspace, no `loom/*` branch and no worktree, and
offers "Finish abandoning", which runs the rename alone. Without the marker, such as for a plan
running from another clone, nothing is offered.

### The run dialog

The dialog shows:

1. **The plan.** Preselected: the first written plan in metaplan order, then unlisted plans by
   file name (section 01's ordering). Any written plan can be chosen.
2. **The branch.** The current branch, which the run merges into.
3. **Uncommitted changes**, split in two lists:
   - changes under `doc/`, which the app can commit (below);
   - modified or staged tracked files outside `doc/`, which block the run. The dialog lists them
     and says to commit or stash them; the app does not touch files outside `doc/`.
4. **The backend.** `tmux` by default, passed as `loom init --backend tmux`; `native` is offered
   unless the server runs headless (section 02, "Opening the browser"). `loom init` without a
   terminal skips its backend prompt and falls back to the configured value
   (`loom/src/commands/init/backend.rs:32`); the native backend opens windows on the server's
   desktop, which the browser user may not be sitting at; and the dashboard's stage terminals work
   only with tmux.

   **A change to loom outside the app:** loom's own built-in default backend becomes tmux, so
   `loom init` and `loom run` started from a terminal default to the same backend the app does.
   Today the built-in default is native (`SessionBackendKind`'s `#[default]`,
   `loom/src/models/session/types.rs:93-95`), which `terminal.backend` in `~/.loom/config.toml`
   and the workspace `[terminal]` section override; the change moves `#[default]` to `Tmux`, and
   `loom init`'s prompt shows tmux as the default.
5. **A `--clean` notice**, when the workspace holds a finished plan: init stops the daemon if it
   still runs, reaps the old run's sessions, archives its memory, and removes its worktrees and
   workspace (`loom/src/commands/init/execute.rs:185`).

**Committing planning documents.** The dialog offers "Commit changes under `doc/`" with every
changed path under `doc/` listed and checked, including new files, and a message field prefilled
with `docs(plan): planning documents for <plan file name>`. The app runs
`git --literal-pathspecs add -- <paths>` and `git --literal-pathspecs commit -m <message> --
<paths>`, so the commit holds exactly the listed paths and nothing else that was already staged,
in the project root with the user's own git configuration, hooks,
and signing; it never passes `--no-verify` and never pushes. A failed commit shows git's output and
starts nothing. `loom run` refuses uncommitted plans and modified files, and the vision asks for
no terminal beyond `loom plan`, so the app commits planning documents itself. The summary store
under `doc/loom/summaries/` is project content (section 04) and is listed like any other path.

**Start.** The button is enabled once nothing blocks. It runs, in order, `loom init <plan> --backend
<backend>` (with `--clean` when required) and `loom run`, as child processes of the server: the
running `loom` binary, the project root as working directory, the server's own environment, stdin
closed. Their output streams into the dialog. The first non-zero exit stops the sequence and
leaves its output on screen; `loom init` cleans up after its own failure
(`loom/src/commands/init/execute.rs:47`). (My call: the app adds no checks of its own beyond the
table above; loom's commands stay the authority on what a run needs.)

Starting a run does not end an authoring chat. The chat may keep refining documents while the
plan runs; the running plan becomes read-only the moment its file is renamed.

## Handing off to the dashboard

After `loom run` succeeds, and whenever the workspace holds a running or stopped plan, the app
offers "Open dashboard". The first time, the server starts `loom status --web` as its own child
process, in the project root, bound to the same host as `loom plan`, with `--terminals` when the
run's backend is tmux. It reads the dashboard's URL from its first output line (`print_startup`,
`loom/src/commands/status/web/mod.rs:179`), whose format gains a test that pins it, and opens it
in a new browser tab. The dashboard picks its own port from 7373 up, as it always does.

- The URL carrying the dashboard's token goes only to the browser holding authoring; read-only
  browsers get the URL without it, which in the loopback posture shows the dashboard without its
  write lanes.
- Over SSH, the app shows the tunnel hint of section 02 for the dashboard's port too.
- The dashboard child lives as long as `loom plan`; stopping `loom plan` stops it and leaves the
  daemon running. A dashboard the user started separately is unaffected; the app does not look
  for one. (My call: a second dashboard on the next port costs nothing, and discovering one would
  need a lock file the dashboard does not have.)

## While plans run

The planning app does not monitor stages; the dashboard does (vision, Non-goals). It shows:

- **Plan states from file names** (section 01, "Plans"): written, running, done, retired, in the
  spine, in file mode, and in the engineering overview. The spine marks its Run group active while
  any plan is `IN_PROGRESS-`.
- **The Run page**, `/f/<feature>/run`: a line for the overall state (nothing running, running with
  stages merged out of the total, or done), the action that starts a run, the dashboard link when
  the dashboard is up, and one row per plan with its state, each linking to the plan's concept
  page. The global run state covers only the active plan, so the page shows no per-plan progress
  beyond state. (My call; revisit if per-stage progress is wanted here.)
- **The run row** in the header's activity popover, while `.loom/work` exists: while the daemon
  runs, a "Plan running" row with the active plan's file name, the count of stages merged out of
  the total, the count needing attention in the warning tone, and the dashboard link. A stopped
  run leaves one line in the popover's foot with the same plan, count, attention and link. No
  strip sits under the app header. The server reads it every 5 seconds with the code the
  dashboard's file-poll lane uses (`data::collect_status_data`, `render::attention_entries`;
  `doc/loom/knowledge/architecture/web-dashboard.md`, "Server"). (My call: one line tells a
  planner whether to look at the dashboard, without rebuilding it.)
- **Locked notices**: a running plan's file and briefs open read-only with "running; open the
  dashboard" (section 09, "Write limits").

Planning edits during a run are safe for the run. The daemon merges stage branches into the
checkout with the user's uncommitted changes carried along, and blocks a merge, without touching
the checkout, when a change overlaps a path the merge touches
(`doc/loom/knowledge/architecture/merge-checkout-state.md`). The paths that can overlap are the
plan's own renames and knowledge-stage writes under `doc/loom/knowledge/`; planning sessions write
neither directly.

When the daemon renames the plan to `DONE-`, the watcher sees the rename (section 04 keeps its
summary), the run review `REVIEW-<plan file stem>.md` appears beside it (section 03), and the
checklist moves on.

## After plans complete

Before a plan finishes, its integration-verify stage reads the product sections the plan's
`Implements:` line names. It fixes the code where the build lacks or contradicts a section. When
the build is right and the section is wrong, it edits the section to match and records a
`spec-conformance:` decision; a gap it neither fixes nor corrects becomes a `spec-conformance:`
question. The run review (`REVIEW-<plan file stem>.md`, section 03) lists those decisions and
questions under `## Spec Conformance`, then the stage-level `## Open Questions` and
`## Open Findings`: reviewer suggestions no stage implemented and entries deferred during the run.
Each bullet names the stage and its id.

When a plan completes, the server imports the review's `## Open Findings` and `## Open Questions`
items into the feature's findings ledger, `doc/plans/<f>/findings.md` (a plain Markdown file of
`## F-<n>` entries with About, Source and Status lines, section 09, "`loom-plan:review`"). An
item already in the ledger is not added twice, and a standalone plan imports nothing. The import
runs in the host job on the main checkout, so `findings.md` is an uncommitted change there, like
the summaries the planner rebuilds; the user commits it. The run review stays the immutable
evidence; the ledger holds the triage state.

A finished feature, where every metaplan entry's plan is done, has no primary offer (section
01) and has no page of its own: it opens where an unfinished feature opens (section 05, the
landing rule), and its Run page says every plan is done. Follow-ups start on `/new` (section 08):
Follow-up with the feature picked lists the ledger's open findings and questions as suggestions
under the field, and Start sends the typed text, with any picked suggestions, to the
`Plan follow-ups` session (`loom-plan:metaplan`, section 09). The session appends entries to
`doc/plans/<f>/index.md` under a `## Follow-ups` heading and leaves finished entries unchanged;
the entries keep the level-2 entry form (section 01). Once an entry has no plan, step 4's gap offers `Write plan <slug>` and drives
the rest. Work that is no follow-up of the feature is a new feature, started from the home page.

**The Findings page.** `/f/<f>/findings` is the wave screen: the ledger's findings grouped by the
run they came from, newest first, then reviews, each with its About nodes as links, its status and
the actions of section 10, "Triaging findings" (Plan this, Fold into entry, Dismiss with a
reason). The spine's Run group links to it with the open count.

**Review the feature.** Once any plan of the feature is running or done, a secondary offer,
`Review the feature` (`loom-plan:review`, section 09), is available at any time, not only when
the feature is finished. The session asks what the user saw, reads code and documents without
writing them, and appends findings to the ledger. Reviewing after wave 1 therefore feeds the plans
of wave 2.

**Revisit before writing.** A finding folded into an entry, and an open finding whose About names
a heading that entry covers, mark the entry "revisit before writing" while its plan is unwritten,
or "revisit before running" while the plan is written and has not started. The mark names the
finding ids; the author reads them before the plan is written or run. An entry whose plan is
running or done carries no mark: its follow-ups are new entries.

Another offer stays on the Run page while any written plan remains:

- **Run the next plan** stays offered while any written plan remains: the workspace state table
  above gives `loom init <plan> --clean`.

A returning author who opens `loom plan` weeks later lands here with no step of the app's own:
the checklist is computed from the files (section 01), and the summaries catch up by hashing
(section 04).

## Out of scope for this section

- The pressure-test record's detection and the step states: section 01.
- The pressure session's launch and transcript reading, beyond the differences above: section 07.
- How the chat renders a pressure run's turns and status items: section 08.
- The skills, the plugin, and the closing message: section 09.
- Whether a product section changed after the plans that implement it ran: section 10.
- Permission mode, write rules for pressure sessions, and the commit lane's access rules:
  section 12.
- Stage monitoring, stage terminals, and daemon control: `loom status --web`.
