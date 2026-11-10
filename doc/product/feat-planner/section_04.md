# 04 — Summary Engine

Part of the [Loom Plan product vision](index.md).

This section defines how summaries are stored, how the engine knows which of them are out of
date, who rebuilds them and on which model, the file watcher the rest of the app relies on, and
how rebuild cost is measured and shown. What the reader sees of all this is section 05; how a
session is started and owned is section 07.

## The model in one paragraph

Every summary is a Markdown file under `doc/loom/summaries/`, one sentence per line, each
sentence ending in links to what it draws on. A front matter block at the top of the file is that
summary's manifest entry: the content hashes of everything it cites, taken when it was written.
The engine stores no other state. On start and on every change the watcher reports, it hashes
the sources again and compares: a sentence whose cited sources still hash the same is current,
any other sentence is out of date, and a summary with an out-of-date sentence or with uncited
changes in its source is queued for rebuild. Agents write only the sentences and links; a loom
command validates them and computes every hash, so no agent ever writes a hash.

## The store

```text
doc/loom/summaries/
  overview/<f>/product.md          one product overview per feature with a vision
  overview/<f>/engineering.md      one engineering overview per feature with a metaplan
  <f>/product/vision.md            one per feature's vision
  <f>/product/section_01.md         one per feature's product section, named after the section file
  <f>/metaplan/index.md            one per feature's metaplan
  plans/<slug>.md                  one per eligible plan, named by the plan's slug
```

A summary's **id** is its path under `doc/loom/summaries/` without `.md`:
`overview/<f>/product`, `overview/<f>/engineering`, `<f>/product/vision`,
`<f>/product/section_<nn>`, `<f>/metaplan/index`, or `plans/<slug>`.
Here `<f>` is a feature name (section 03), and `<nn>` has at least two digits. `plans` and
`overview` are reserved feature names, so plan and overview ids cannot collide with a feature's
section ids. For example: `overview/feat-planner/product`, `feat-planner/product/section_01`,
`plans/delivery-truth`.

Which documents get a summary:

| Source                                                                                                   | Summary                    |
| -------------------------------------------------------------------------------------------------------- | -------------------------- |
| `doc/product/<f>/index.md`, the feature's vision                                                         | `<f>/product/vision`       |
| `doc/product/<f>/section_<nn>.md`, listed or unlisted (section 01)                                       | `<f>/product/section_<nn>` |
| A plan directly in its feature home, `doc/plans/<f>/`, in any lifecycle state                            | `plans/<slug>`             |
| A plan directly in the standalone home, `doc/plans/`, in any lifecycle state                             | `plans/<slug>`             |
| A plan in `doc/plans/<f>/archive/` named by the metaplan of feature `<f>`                                | `plans/<slug>`             |
| An archived plan the metaplan of its feature does not name, including every plan in `doc/plans/archive/` | none                       |
| `doc/plans/<f>/index.md`, the metaplan of feature `<f>`                                                  | `<f>/metaplan/index`       |
| Any other `*metaplan*.md` file of `<f>` (for example `metaplan_02.md`), an Other document (section 03)   | none                       |
| A misplaced top-level `doc/plans/index.md`, briefs, knowledge                                            | none                       |

Briefs have no summary of their own: their plan's summary covers them (see the bundle rule under
[The summary file](#the-summary-file)).
Plan summaries are keyed by slug, unique repository-wide, so lifecycle renames and moves between
homes keep the same id; a move into an archive keeps the summary only while the plan is eligible.
Archived plans the metaplan of their feature does not name are left out because an older project
can hold dozens of them and summarizing them would cost more than any reader gains; they stay
readable in file mode. Standalone plans have no metaplan, so their archived plans get no summary.
The vision's summary is built and rebuilt like a section summary. It is not a member of the
product overview and not listed among the documents outside it: the product overview cites the
vision's heading sections directly. A feature's metaplan, `doc/plans/<feature>/index.md`, has a document summary with id
`<feature>/metaplan/index`, stored at `doc/loom/summaries/<feature>/metaplan/index.md`. Its
sentences cite that metaplan's passages only. Like the vision summary it is a member of neither overview: the engineering overview
still cites the metaplan's raw passages and the plan summaries (below).

**Overview scope.** Each feature has its own scope. Its product overview covers its vision and
the product sections that vision's table lists (section 01); its engineering overview covers
its metaplan and the plans in that feature that the metaplan names. The product overview is
expected only when the feature has a vision; the engineering overview only when it has a
metaplan, even if either has no members. An unlisted section, or an eligible plan the metaplan of
its feature does not name, is outside that feature's overview scope: it still gets its document summary,
and section 05 lists it under the overview instead of in it. An overview's members are the
document summaries in its scope. Standalone plans belong to no feature's overview scope.

Each summary is its own file with its own manifest entry, so a git merge of two people's
rebuilds conflicts only when both rebuilt the same summary (vision, Decisions). There is no
shared index file. (My call: a single manifest file would put every rebuild's hash changes in
one file and turn unrelated rebuilds into merge conflicts.)

## The summary file

A document summary, `doc/loom/summaries/feat-planner/product/section_01.md`:

```markdown
---
kind: document
of: doc/product/feat-planner/section_01.md
file: sha256:9c1f…
cites:
  doc/product/feat-planner/section_01.md#the-model-in-one-paragraph: sha256:4a7e…
  doc/product/feat-planner/section_01.md#detection-rules: sha256:e210…
  doc/product/feat-planner/section_01.md#pressure-tests: sha256:07bd…
---

# 01 — Workflow Model and Project State

The app keeps no workflow state and recomputes each feature's checklist from its documents on every change. [§](../../../../product/feat-planner/section_01.md#the-model-in-one-paragraph)
The feature's current step is the earliest step with a gap, and no gap blocks later work. [§](../../../../product/feat-planner/section_01.md#the-model-in-one-paragraph)
A plan counts as pressure-tested once the `loom pressure` driver has appended a dated line to its `## Pressure tests` section. [§](../../../../product/feat-planner/section_01.md#pressure-tests)
```

The feature's product overview, `doc/loom/summaries/overview/feat-planner/product.md`:

```markdown
---
kind: overview
of: overview/feat-planner/product
members:
  feat-planner/product/section_01: sha256:51c0…
  feat-planner/product/section_02: sha256:d93a…
cites:
  doc/product/feat-planner/index.md#vision: sha256:aa04…
---

# Product overview

`loom plan` is a browser app that takes a feature from a vision to approved loom plans. [vision](../../../../product/feat-planner/index.md#vision)
It derives the feature's workflow step from the files alone and never blocks out-of-order work. [01](../../feat-planner/product/section_01.md#s-3fa9c2d1) [01](../../feat-planner/product/section_01.md#s-77e0b412)
```

Rules for the body, enforced when the summary is sealed (below):

- **One sentence per line.** Markdown joins consecutive lines into one paragraph, so the file
  reads as prose in any viewer; a blank line starts a new paragraph. `#` and `##` headings may
  group sentences and carry no links. Lists, tables, code blocks, and HTML are not allowed.
- **Every sentence ends in one or more links**, plain Markdown links relative to the summary
  file, so a reader on GitHub or in an editor can follow them. The app ignores the link text;
  agents use a short label (`§`, a section number, a plan slug).
- **A document summary cites only its own source**: heading sections of that file, by the
  anchors section 03 defines. A link with no fragment cites the text before the first heading.
  A plan summary's source is the plan together with its brief bundle (below).
- **A plan summary has two levels.** The opening level comes first: the title, optional `##`
  headings, and about one sentence per topic, each citing every sentence of the detail
  paragraph or paragraphs it opens, whole and in order, by a link to its own sentence id
  (`[d](#s-3fa9c2d1)`) and nothing else. The opening sentences are grouped by topic into
  paragraphs of at most 3 sentences, separated by blank lines, so the Summary page reads as
  paragraphs and the seal refuses a longer one. The detail level follows: short paragraphs whose
  sentences cite the plan's heading sections, with no heading among them. An opening sentence
  says what its paragraphs add up to and never restates one of their sentences. Sentence ids are
  hashes an agent cannot compute, so every refusal about the levels lists the draft's detail
  sentences by id with their first words: the agent writes the detail level, seals, copies the
  ids into the opening level and seals again. Overviews cite only sentences that cite no
  sentence, so they cite a plan summary's detail sentences, never its opening level.
- **A plan summary covers its brief bundle.** The bundle is every file under the plan's
  `briefs/<slug>/` directory in its plan home. The detail level cites the plan and its brief
  files: each stage gets its own paragraph citing `PLAN-<slug>.md#stage:<id>` and that stage's
  briefs (`briefs/<slug>/<stage-id>/…`), and `common.md` and the other briefs are cited where the
  plan relies on them. The seal refuses a plan summary that leaves a brief uncited, and refuses a
  link to any file outside the plan and its bundle, another plan's briefs included. A brief with
  no Markdown text to cite (a JSON mockup, an unreadable file) need not be cited. These rules
  add to the two levels and relax none of them: an opening sentence still cites only detail
  sentences.
- **An overview cites document-summary sentences and raw passages.** It links a document
  summary's sentence by the sentence's id as fragment (`#s-3fa9c2d1`), and links heading
  sections of its feature's vision (product overview) or metaplan (engineering overview) directly;
  the vision's own summary is not one of its members. Every document in the overview's scope must be cited by
  at least one overview sentence, so no section or plan drops out of the overview silently.
- **An overview sentence draws on at least two member sentences.** A sentence that cites member
  sentences cites at least two of them and says what they add up to; the seal rejects a sentence
  that cites exactly one, unless that member's summary holds a single citable sentence.
- **Length.** A section summary holds at most 15 sentences, a plan summary 8 to 12 opening and
  15 to 60 detail sentences, scaled to the lines of the plan and its briefs, a vision summary at most 8 in at most 3 paragraphs, an overview at most
  30. (My call: the overview has to be readable in the few minutes the vision's reviewer goal
  allows.)

A **sentence id** is `s-` plus the first 8 hex digits of the SHA-256 of the sentence's text with
its links removed and its whitespace collapsed. The id is never written into the file; anyone can
compute it. Two identical sentences in one summary are rejected. Because the id follows the
text, a rebuild that keeps a sentence word for word keeps the overview links into it.

### The front matter

| Key       | In       | Value                                                                                                           |
| --------- | -------- | --------------------------------------------------------------------------------------------------------------- |
| `kind`    | both     | `document` or `overview`                                                                                        |
| `of`      | both     | the source file's path (document), or `overview/<f>/product` / `overview/<f>/engineering` (overview)            |
| `file`    | document | hash of the whole source file                                                                                   |
| `bundle`  | plan     | brief path to the hash of that whole brief file, for every file of the plan's brief bundle                      |
| `cites`   | both     | `path#anchor` to the hash of that span, for every raw passage the body links                                    |
| `members` | overview | summary id to the hash of its ordered sentence ids, for every document summary in that feature's overview scope |

`bundle` appears only in a plan summary whose plan has briefs, and is written only when it holds
an entry. An overview or a non-plan document summary that carries it fails validation; a plan
summary whose map differs from its plan's briefs, a map for a plan with no briefs included, reads
incomplete. A brief with no Markdown text is recorded by the fingerprint the planning trace keeps
for it (a hash of its bytes, or why it could not be read). Like `file`, `bundle` is computed when
the summary is sealed and never taken from the draft: a draft carries no front matter at all.

`members` hashes the ordered list of sentence ids, so rewriting a link or the front
matter of a document summary does not make the overview out of date. Front matter carries no
build time and no model; git history and the cost log (below) have both. (My call: every key
here changes only when the content does, so a rebuild that changes nothing produces no diff.)

## Hashing

All hashes are `sha256:<hex>` from `body_hash` (`loom/src/context/source_graph/mod.rs:127`),
the one hash definition the source graph and knowledge bootstrap already use.

The unit is the **heading section**: the heading line and everything up to the next heading of
the same or a shallower level, nested subsections included, ignoring `#` lines inside fenced
code. This is the span `loom/src/fs/knowledge/splice.rs:162` (`section_end`) already computes for
`loom knowledge replace-section`; the engine reuses that scan and extends it to level-1
headings. Where section 03 defines anchors finer than headings (for example a plan stage), the
engine hashes the span section 03 assigns to the anchor the same way. Before hashing, line
endings become `\n` and trailing whitespace on each line is dropped, so an editor that rewrites
line endings does not invalidate summaries. (My call.)

The engine also hashes the whole source file, which is how it notices a change in a passage no
sentence cites, and a new or removed section. For a plan it hashes every brief file of the
bundle as well, which is how it notices an edit to a brief no sentence cites, and a brief that
joins or leaves the bundle.

## What "out of date" means

The engine evaluates every summary from the files alone, and pushes the result to every open
browser over the server's WebSocket. Each sentence has one state:

| State   | A document-summary sentence                          | An overview sentence                                                              |
| ------- | ---------------------------------------------------- | --------------------------------------------------------------------------------- |
| current | every cited span hashes as recorded                  | every cited summary sentence exists and is current; every cited raw span matches   |
| stale   | a cited span changed, or its anchor no longer exists | a cited summary sentence is gone or stale, or a cited raw span changed or is gone |

Each summary has one state:

| State      | When                                                                                                    |
| ---------- | ------------------------------------------------------------------------------------------------------- |
| current    | every sentence current, and the source's `file` hash and, for a plan, its `bundle` map (document) or every `members` hash (overview) matches |
| incomplete | every sentence current, but the source changed outside the cited spans, a plan's brief changed, joined its bundle or left it, or a member summary changed, joined the overview's scope or left it |
| stale      | at least one sentence stale                                                                             |
| rebuilding | claimed by a session (below); its last sentences stay readable with their states                        |
| missing    | no file, a file that fails validation, or a file with git conflict markers                              |

Incomplete and stale summaries both get rebuilt; the difference tells the reader whether
anything already on screen is wrong. A stale sentence stays readable and marked until its
replacement arrives (vision, Refinement); section 05 decides how each state looks.

A summary whose file fails validation, including a hand edit that broke the rules or a merge
that left conflict markers, counts as missing and is regenerated, which resolves the conflict
(vision, Concurrent writes). A hand edit that passes validation is kept and judged by its hashes
like any other.

When an eligible source file disappears and another appears with the same file hash (a plan's
lifecycle rename, a move into its feature's archive while a metaplan there names it), the engine
rewrites the paths in the affected front matter and links itself, with no agent and no rebuild.
When a source is deleted,
its document summary is deleted and the overview sentences that cite it turn stale. A new section
or plan eligible for a summary gets a missing summary, which queues it.

## The watcher

One watcher in the server reports changes under `doc/`, and sections 01, 05, 06, and this
engine all consume its events. It polls: every second it walks `doc/`, compares each file's
size, modification time, and inode with the last walk, and hashes only the files that differ.
`doc/` holds hundreds of files at most, so a walk is cheap, and polling needs no new dependency
and behaves the same on macOS, Linux, and network mounts. The dashboard already polls rather than
subscribing to file events: it reads its state on a 2-second timer and compares snapshots
(`loom/src/commands/status/web/broadcast.rs:23`). (My call.)

- **Batching.** Changes are held until a walk finds nothing new, then delivered as one batch of
  added, changed, removed, and renamed (same hash, new path) files. A `git pull` that touches
  thirty files arrives as one batch and one rebuild decision.
- **Skipped entries.** Hidden files and directories and editor temporaries (`*~`, `*.swp`,
  `.#*`) are ignored.
- **The engine's own writes.** The engine remembers the hash of every summary it seals or
  rewrites; a summary file that matches that hash is not reported as an outside change. Any
  other change to a summary file (a pull, a hand edit) is re-evaluated as above.
- **Start.** The first walk at server start is a batch like any other, so edits made while no
  server ran are caught up by hashing (vision, principle 4).

## Sealing a summary

Agents never write into `doc/loom/summaries/` directly. They write the body of a summary to a
draft file in the runtime directory and run:

```text
loom plan summary seal <summary-id> [--force]
```

`seal` reads `~/.loom/plan/<project-key>/drafts/<summary-id>.md`, checks every body rule above,
resolves every link (anchors must exist, cited summary sentences must exist and be current),
computes the hashes, takes the store lock, writes the summary file with its front matter by
temp file and rename, releases the lock, and deletes the draft and the summary's claim. A rule it
rejects is printed with the offending line, and the agent fixes the draft and seals again. Sealing turns "every claim
traces to its source" (vision, principle 3) into a mechanical check.

The **store lock** is an exclusive OS lock on `~/.loom/plan/<project-key>/summaries.lock`, held
only for the write, through `fs2` as the quota cache's shared lock does
(`loom/src/quota/shared.rs:130`). One writer at a time therefore holds for the server, its
sessions, and a `seal` run by hand from any terminal.

The other subcommands sessions use:

```text
loom plan summary status [--json]      every summary's state, stale sentences, claims
loom plan summary brief <summary-id>… [--note <text>]  claims the summaries and prints the rebuild brief
loom plan summary release <summary-id>… gives up claims without sealing
```

`brief` creates a **claim** file per summary under `~/.loom/plan/<project-key>/claims/`, naming
the claiming session, and refuses a summary that already has a claim file. It decides from
`claims/` alone. It accepts a current summary as well as an out-of-date one, so a session that
finds a sentence misstating its source can rebuild it (section 10); `--note <text>` names the
sentence to replace and why, and the brief prints the note beside that sentence. The brief
lists, per summary, the source paths (for a plan, the plan and every file of its brief bundle),
the current summary text with each stale sentence marked,
which sentences may stay word for word, the body rules, the draft path, and the `seal` command.
Keeping current sentences word for word keeps their ids, so overview links into them survive.

**Clearing claims.** Whether a claiming session is alive is the server's check. The server
deletes a session's claim files when that session ends: its registry state becomes Ended or
Failed, or the process identity check finds the process gone (section 07). A refinement
subagent's claims are deleted when its `SubagentStop` arrives or when its parent session ends.
`release` deletes claims early, and `seal` deletes the claim of the summary it writes. At start,
after recovering live sessions, the server deletes the claims of every session that is neither
live nor left alone. A claim made from a terminal names no session; it never expires on its own,
and the summary panel offers to release it. `seal` refuses a summary another session has claimed,
and from a terminal refuses any claimed summary unless given `--force`.

All of these subcommands work without a running server, since they need only the project root
and the runtime directory. (My call; section 02 owns the CLI surface, and these sit beside
`loom plan verify`.)

Overviews are sealed after the document summaries they cite, because `seal` refuses an overview
that links a stale or missing sentence.

## Who rebuilds what

| Trigger                                                   | Who                                                                     | Budget check |
| --------------------------------------------------------- | ----------------------------------------------------------------------- | ------------ |
| A refinement turn edited documents                        | a background summary session, 5 minutes after the turn's end            | yes          |
| Outside edit, file-mode save, `git pull`, start           | a background summary session, 5 minutes after the last change           | yes          |
| The user presses rebuild on a summary or a batch          | a background summary session, at once                                   | no           |
| The user answers "Rebuild all" to the question            | a background summary session, at once                                   | no           |

**Refinement.** An authoring turn rebuilds no summary: the authoring skill (section 09) ends a
turn that edited documents with `loom plan check` alone. While an authoring turn runs, the
scheduler starts no background session; when the turn ends, whatever it left stale or missing
goes to the background queue, and what the turn changed waits a full quiet period from the
turn's end (below). (My call: a reader working through open questions and "My call" items edits
the same documents turn after turn, and a rebuild after every turn would pay for summaries the
next turn makes stale again; the watcher also sees the session's edits the moment they are
written, and must not start a rebuild of what the turn is still editing.)

**Background.** The scheduler keeps every summary the engine evaluates stale or missing in the
background queue. An incomplete summary is out of date too, but its sentences are all still
right, so it waits for a press. A queued summary starts once 5 minutes (its quiet period) have
passed with no change to its inputs: its source and, for a plan, its brief bundle; for an overview, its raw
sources and its members' inputs and sentences. An edit made during an authoring turn counts from
the end of that turn, since the reader's next turn often edits the same document again. Settling
a document changes no summary's inputs: every hash a summary records or compares is taken over
the text without its `Settled:` lines (section 03, "Settled marks", defines the line). A
summary out of date when the server starts has no change on record and waits for nothing. The
summary panel lists the waiting summaries apart. A press or a "Rebuild all" answer starts its summaries at once, quiet period or
not. Of the summaries that are ready, up to 5 start at once; above 5 the scheduler asks first
(vision, Rebuild budget), and the summaries still waiting neither start with them nor count
toward the 5. Turning off Settings' "Rebuild summaries automatically" in the holder's browser
sends `POST /api/summaries/auto` and sets that limit to 0, so every ready batch asks first.
One background summary session runs per project at a time, as an interactive
Claude Code session in tmux started through the session bridge (section 07), never `claude -p`.
It works its batch leaves first: one subagent per document summary, each running `brief` and
`seal` for its document, then the overviews itself. Summaries invalidated while it runs form the
next batch. The server stops the session once every claim it held is sealed or released. A
session that seals nothing for 10 minutes is stopped, its claims are released, and the
summaries it held are shown as failed with the last `seal` error. (My call.)

**Models.** Document summaries (one per product section, one per plan) are built on Haiku at
high effort by default. Overviews (product and engineering) and the background summary session
default to Sonnet at high effort, because an overview joins many documents. The user selects the background summary backend with
`[plan] summary_backend`: `claude` (default) or `codex`. The Claude backend uses the interactive
session above. The Codex backend runs `codex exec` in the background with the workspace-write
sandbox rooted at the project, plus write access to `drafts/` and `claims/` in the runtime
directory (`~/.loom/plan/<project-key>/`); the rest of that directory stays read-only to the job,
which takes the store lock through a read-only descriptor on `summaries.lock`. It has no tmux
attach link; the running rebuild's record in the summary panel shows the job's log path
(`<runtime>/codex-logs/<session>.log`) instead, and it records no cost. A job exits by itself, so
a clean exit that leaves summaries unsealed requeues them once and a second one marks them
failed. When the backend is `codex`,
`[plan] summary_codex_model` (default `gpt-6.1-terra`) and `summary_codex_effort` (default `high`)
select its model and effort.

For the Claude backend, config keys override the defaults per level:
`[plan] document_summary_model` and `document_summary_effort` for document summaries,
`[plan] overview_model` and `overview_effort` for overviews and the background summary session.
All summary keys resolve with loom's existing precedence (project `.loom/work/config.toml`
when present, then `~/.loom/config.toml`, then the default;
`doc/loom/knowledge/conventions/model-and-effort-config.md`). The project tier reads these keys
from a `[planning]` table of `.loom/work/config.toml`, because `[plan]` there holds the run's
identity (`source_path`, `plan_id`, `plan_name`, `base_branch`); `~/.loom/config.toml` keeps the
`[plan]` section. The model and effort keys follow the one
`[plan] <role>_model` / `<role>_effort` naming scheme section 09 uses for the authoring keys. The
subagent tool takes no effort parameter, so the server writes the resolved model and effort into
the `model` and `effort` frontmatter of the `loom-plan:summary` and `loom-plan:overview` agent
definitions when the session starts (section 09), and each subagent runs on its level's model
without the session that spawns it choosing one. Cost is an anti-goal concern.

## Asking before a large rebuild

Above the threshold, the app shows how many summaries are out of date, which ones, and an
estimate of the cost (below), with two actions: rebuild them all, or not now. The question goes
to every connected browser; only the browser holding authoring, or any browser while nobody holds
it, can answer. (My call.) Declining leaves the summaries marked out of date with a rebuild
action on each; the next batch counts only the summaries it newly invalidates, so one declined
pull does not turn every later edit into a question.

A first `loom plan` on a project with no summaries (one feature with a vision, a metaplan,
twelve sections and eight eligible plans means 22 summaries) asks the same question.

## Cost

Summary sessions run under the user's subscription, so cost is measured in tokens and shown
against the subscription's quota.

- **Measuring.** Every rebuild is a Claude Code transcript: the background session's own JSONL
  and its subagents' `subagents/agent-<id>.jsonl` files, or the authoring session's summary
  subagent. The engine reads them with the token tracker's parser
  (`loom/src/tokens/claude_line.rs:21`) and its streamed-message merge
  (`loom/src/provider_data/mod.rs:10`), so the counts match `loom status`. Which subagent of
  the authoring session did the rebuild comes from the hook event of its `brief` call (section
  07). Costs are per summary when one subagent built one summary, and per batch for the
  overviews.
- **Recording.** Each finished rebuild appends one line to
  `~/.loom/plan/<project-key>/summary-cost.jsonl`: time, trigger, summary ids, model, and the
  token vector (fresh input, cache writes, cache reads, output), unknown fields left absent,
  never zero. The log is runtime state; nothing about cost is committed.
- **Estimating.** The estimate shown before a large rebuild sums each summary's last recorded
  cost, and for a summary with no record scales the median cost per source byte across the log,
  taken per level since document summaries and overviews run on different models (or a fixed
  starting figure while the log is empty). It is labelled an estimate.
- **Showing.** Each chat turn lists the summaries it rebuilt with their tokens (section 08). A
  summary panel shows the running rebuild, the cost of the last rebuild, and totals for today
  and the last 7 days. The five-hour and weekly limits show in the footer only, not in the panel.
  `loom plan` polls quota through the account-shared poller (`loom/src/quota/shared.rs:130`),
  which adds no requests when a daemon already polls for the same account.

## In read-only mode

When `loom plan` runs read-only (section 02, "Missing prerequisites": `tmux` or `claude`
missing, Claude Code logged out, or the sandbox's prerequisites missing), the engine
still watches, hashes, and marks every sentence's state, so concept mode shows exactly which
committed sentences no longer match the documents; it starts no rebuilds and asks nothing. The
summary panel says why and how to fix it. A missing summary stays missing;
section 05 says what concept mode shows in its place. `loom plan summary seal` still works from
any terminal, so a session started by hand can rebuild summaries without the app.

## Out of scope for this section

- Document paths, kinds, heading anchors, and stage anchors in plans: section 03.
- How sentences, marks, out-of-date states, and expansion look to the reader: section 05.
- The file-mode editor's save path and its hash check: section 06.
- Starting, owning, and stopping tmux sessions, transcript reading, and hook events: section 07.
- How a turn's rebuilt summaries and their cost appear in chat: section 08.
- The authoring and summary skills, and how they are injected: section 09.
- Which runtime paths sessions may write (drafts, claims): section 12.
