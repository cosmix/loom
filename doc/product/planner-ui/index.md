# Loom Plan — Product Vision

Status: agreed vision (2026-10-26). Each section listed under
[Specification sections](#specification-sections) becomes its own
`doc/product/<feature>/section_NN.md`.

## Problem

Planning a loom project happens in Claude Code today, in a sequence the user drives by hand:

1. Brainstorm a product vision (`doc/product/<feature>/index.md`) that names the sections the product
   spec needs.
2. Write each product section under `doc/product/<feature>/`.
3. Brainstorm a metaplan (`doc/plans/<feature>/index.md` or
   `doc/plans/<feature>/metaplan-NN.md`): one entry per loom plan, three or four paragraphs each.
4. Author the loom plans under `doc/plans/<feature>/`, usually with an agent team.
5. Pressure-test the first plans with `loom pressure`.
6. Review the plans as a human.
7. Start implementation with `loom run`.

Two things go wrong. The user has to know this sequence, the skills behind each step, and the
file conventions, and drive all of it from a terminal. And by step 6 the output is too large to
review: a vision, a dozen product sections, a metaplan, several plans, and a tree of briefs under
`doc/plans/<feature>/briefs/`. A PM or engineer asked to approve that either skims it or gives up. The
review step, the one where a human is supposed to catch a wrong direction before agents spend
days building it, is the step the current tooling serves worst.

## Vision

`loom plan` opens a browser on a React app served by the `loom` binary. From that app a user
takes a project from an empty `doc/` to approved, pressure-tested loom plans without opening a
terminal, and can come back at any later point to read, question, and refine what exists.

The app does two jobs.

**Authoring.** The user talks to Claude Code through a chat panel in the app. Loom starts and
owns those Claude Code sessions in tmux on the backend, relays the session's output to the chat
panel as structured messages, and delivers the user's messages back into the session. The
sessions run with skills and guidance that make them produce documents in loom's conventions for
whichever step the feature is at: vision, product sections, metaplan, plans, pressure review.

**Reviewing.** The user explores everything that planning produced, in two modes:

- **File mode.** Browse and read any document: product spec, metaplan, loom plans, briefs, and
  the knowledge base under `doc/loom/knowledge/`.
- **Concept mode.** Start from a short overview of a feature's product (built from its vision and
  sections) or of its engineering decisions (built from its metaplans and plans). Opening a
  sentence replaces it in place with the more detailed sentences it summarizes, set in the same
  type and flowing in the same paragraph, so the document grows where the reader is looking;
  opening one of those replaces it with the source passage it came from. Any opened passage
  folds back to the sentence it replaced. The user decides how deep to go and where. Nobody has to
  read the full text to form a view, and anybody can reach the full text from any claim in the
  summary.

Review and authoring connect: from either mode, the user can point at a passage and take it into
a chat ("this plan assumes the daemon is running; it should not"), and the session that answers
edits the documents in `doc/`.

## The screen

The project home (`/`) lists features, each with a title, a pitch from its vision, a seven-cell
progress strip and document counts. Standalone plans share a shelf with "New standalone plan"
and "Move into a feature…"; they have no seven-step checklist or per-plan workflow offers.
An invitation card offers "Start a conversation" at `/new`: the kickoff settles whether the
work needs a feature or a standalone plan before writing anything. A repository with exactly
one feature opens on `/f/<name>`.

The layout has a shared shell and two working regions:

- **Header**: Loom's logo, then the eyebrow "Planning · feature" above a feature switcher.
  On the right: the activity pill; a segmented view switch with icons for Home, Product,
  Engineering and Files; presence; Share; ThemeToggle; Settings; Help. Product and Engineering
  open `/f/<feature>/product[/<doc>]` and `/f/<feature>/engineering[/<plan>]`; file mode uses
  `/files/...`, with the file's feature named in the header breadcrumb.
- **Step rail**, on feature screens only: seven counter cells, each with a glyph, count and
  label. Done cells show a check in the ok tone; the current step has a tint and accent
  underline. Beside the rail, one next-step card carries a document mark, a sentence and one
  primary button. The feature screen (`/f/<feature>`) groups the remaining gaps by step in its
  checklist. The project home shows the header row only.
- **Reading column**, centred, at most about 70 characters wide: the overview or document being
  read, expanding in place as described above. Each overview sentence carries small colour marks,
  one per document it draws on, in that document's colour; a mark drawn dashed means that
  summary is out of date and being rebuilt.
- **Chat panel**, docked on the right: the current session's conversation. Selecting text in the
  reading column attaches it to the next message. Each turn lists the files it changed with a
  link to the diff, and names the summaries being rebuilt.

Inter sets the chrome and every title, including document headings and ordinary chat.
Source Serif 4 sets the reading body: concept prose, rendered Markdown paragraphs, lists and
tables, quoted chat passages and feature pitches. Settings also offers Literata and Piazzolla;
all three reading faces cover Greek. Reading size is 17, 19 (default) or 21px, with line height
1.6 and a measure of 68–70 characters. Code, paths and ids use Commit Mono Cosmix. The visual
design shares the dashboard's components and colour conventions.

### What the first screen shows

The app works out each feature's step from its directories under `doc/` on every start; it
stores no step of its own. It suggests the next step and never forces one:

| What exists for the feature                | Step | Screen and suggestion                                               |
| ------------------------------------------ | ---- | ------------------------------------------------------------------- |
| No features in the repository              | —    | Home's invitation to start a kickoff conversation                    |
| Feature documents without a vision         | 1    | The feature checklist; offers to write its vision                    |
| A vision whose listed sections are missing | 2    | The vision; offers to write the next missing section                 |
| A finished spec, no metaplan               | 3    | The product overview; notes there is no metaplan, offers to start one |
| Metaplan entries without a plan file        | 4    | Both overviews; offers to write the missing plans                    |
| Plans not yet pressure-tested              | 5    | Both overviews; offers to pressure-test them                         |
| Everything present                         | 6, 7 | Both overviews for review and refinement; offers to start a run      |

Accepting a suggestion opens a new session briefed for that step. Refining is available at every
step, including after everything exists. How "finished", "pressure-tested" and "listed sections"
are detected from the files is for section 01 to settle.

## Principles

1. **The files in `doc/` are the truth.** Every document stays plain, committable Markdown (and
   plan YAML where loom requires it) in the paths and conventions loom already uses. A person or
   an agent that has never heard of `loom plan` can read and edit them with any editor, and the
   app picks up those edits.
2. **Summaries are derived and disposable.** Concept mode reads from summaries that agents
   generate from the files (see Decisions for which agent rebuilds what). Each summary records content hashes of what it summarizes; a
   changed, added, or removed file invalidates exactly the summaries built on it, and only those
   get regenerated. Deleting every summary loses nothing but the cost of rebuilding them.
3. **Every claim traces to its source.** A summary sentence that cannot be expanded down to the
   text it came from is a defect. The expansion chain is the review: it is how a reviewer checks
   that a summary says what the documents say.
4. **Continuity across invocations.** `loom plan` on a project that already has a spec, a
   metaplan, or completed plans opens on that state, with summaries brought up to date, and lets
   the user carry on from there: refine a section, add a plan, revisit a decision after
   implementation.
5. **Loom drives the workflow; the user drives the content.** The app knows the seven steps,
   shows where each feature stands, and offers the next one. What the documents say comes from
   the user's conversation with the session.
6. **Reuse what loom already has.** `loom plan` runs its own server process on its own port, built
   from the same parts as `loom status --web`: the hand-rolled HTTP/WebSocket server, the embedded
   SPA build, the access model, and the tmux backend. The dashboard and the planning app should
   look and feel like one product.
7. **Subscription, not API.** Planning sessions and summary agents run as ordinary Claude Code
   sessions under the user's Claude Code login, the same way `loom run` stages do. Nothing in the
   app requires an Anthropic API key, and nothing uses `claude -p` (print, headless, or
   stream-JSON mode), which Anthropic has said may be billed at API rates. Every session the app
   starts, summary agents included, is an interactive session in tmux.

## Users

- **The author**: an engineer or PM who drives steps 1–5, today from Claude Code. They want to
  stop juggling skills, file paths, and step order.
- **The reviewer**: a PM, lead, or engineer who did not write the plans and has to approve them at
  step 6. They want to understand the shape of the work in minutes, drill into the parts they
  doubt, and send corrections back without reading everything.
- **The returning author**: the same person weeks later, after some plans have run, coming back to
  revise the spec or add plans in light of what was built.

## Goals

- A user can run all seven steps from the browser, with no terminal beyond `loom plan` itself.
- A reviewer who has never seen the documents can state the product's main decisions and the
  plan structure after a few minutes in concept mode, and can find the source text of any of
  them in a few clicks.
- Edits made outside the app (an editor, another Claude Code session, a `git pull`) show up in
  the app and in its summaries without any manual step.
- Chat with a session feels like a chat app: replies that appear block by block as the session writes them, readable tool activity, clear
  indication when the session is working, waiting on the user, or needs a permission decision.

## Non-goals

- Replacing Claude Code. Sessions are real Claude Code processes; the app is a front end to them.
- A general Markdown editor or wiki. Chat with a session is the primary way to change documents;
  file mode embeds a plain-text editor for direct fixes, and a saved edit goes through the same
  hash check and summary rebuild as any other change made outside a session.
- Replacing `loom status --web` for monitoring running stages. Step 7 hands off to `loom run` and
  the existing dashboard.
- Multi-user co-authoring. Several people can read through one `loom plan` server; one of them
  authors at a time.
- Hosting outside the developer's machine. The access model stays the dashboard's.

## Anti-goals

These would make the product a failure even if every feature worked:

- Summaries that drift from the documents, or that a reviewer trusts without being able to check.
  A wrong summary is worse than none, because it makes review feel done.
- Documents that only make sense inside the app: app-specific markup, sidecar state the files
  depend on, or structure a plain reader cannot follow.
- A chat that hides what the session did. The user must be able to see which files a session
  changed and review those changes.
- Background summarization whose token cost surprises the user. Regeneration is incremental and
  its cost is visible.

## Specification sections

Each becomes `doc/product/<feature>/section_NN.md`. Order is a first proposal for discussion.

| #   | Section                          | Covers                                                                                                                                                                             |
| --- | -------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 01  | Workflow model and project state | The seven steps as states; how the app infers the current state from the `doc/` tree; what "next step" the app offers in each; how a project that skipped or reordered steps reads |
| 02  | `loom plan` command and server   | CLI surface, flags, port 7272 and browser launch, lifetime, code shared with `loom status --web`, behaviour without `.loom/work`                                                   |
| 03  | Document model and conventions   | Which paths hold which documents (`doc/product`, `doc/plans`, briefs, knowledge); discovery rules; how a document declares its kind and its parent; cross-references               |
| 04  | Summary engine                   | Content hashing, the hash manifest, the invalidation graph, which agents regenerate what and on which model, the store under `doc/loom/summaries/`, cost                           |
| 05  | Concept mode                     | The two overviews, document summaries, sentence-to-source anchoring, product-to-plan traceability, what the reviewer sees at each level                                            |
| 06  | File mode                        | Navigation, rendering of Markdown and plan YAML, briefs and knowledge, the CodeMirror editor, save-time plan validation, diffs, search                                             |
| 07  | Session bridge                   | Starting and owning Claude Code sessions in tmux; reading transcript JSONL as structured messages; delivering input; permission and waiting states; several sessions; resume       |
| 08  | Chat interface                   | Message rendering, tool activity, session states, attaching document passages as context, showing the files a turn changed                                                         |
| 09  | Session skills and guidance      | Skills and instructions per step (vision, sections, metaplan, plans, pressure), how loom injects them, how they keep output in the conventions of section 03                       |
| 10  | Review and refinement loop       | Questioning a passage, taking it into a chat, seeing what a refinement changed; step 6 is the user reading, with no approval record                                                |
| 11  | Handoff to execution             | Pressure testing from the app, `loom init`/`loom run` from the app, returning to planning after plans complete                                                                     |
| 12  | Access and safety                | The app can start write-capable sessions: authentication, sandboxing of planning sessions, which paths they may write                                                              |

## Decisions

- **Features (01, 03; D1–D4).** A feature is the name shared by `doc/product/<feature>/` and
  `doc/plans/<feature>/`; either directory alone is enough. Membership comes from location.
  Anything with a product spec, a metaplan or several related plans belongs in a feature.
  The top level of `doc/plans/` holds standalone plans and their supporting files. A top-level
  vision, product section or metaplan raises `needs-feature`, with an offer to move it into a
  feature. Loom manages only `.loom/work` and `.worktrees` in a target project's ignores;
  this repository's ignore rules cover feature-directory plans too.
- **Kickoff (07, 08, 09, 12; D11).** Every new piece of work starts with a conversation at
  `/new`, with a "Taking shape" panel for its size, name, decisions, questions and proposed
  sections. The session starts with no write under `doc/`. After the user confirms size and
  name and asks to write, a one-time creation grant allows exactly one target that did not
  exist at session start: `doc/product/<name>/index.md` for a feature, or
  `doc/plans/PLAN-<slug>.md` plus its brief bundle for a standalone plan. The plan uses the
  `plan` step skill and `Implements: none`. The session keeps Kickoff scope: file-tool writes
  stay within the target and a standalone plan's brief bundle; `loom knowledge` and `loom plan summary`
  commands other than read-only `status` still ask the holder. Writing a vision opens `/f/<name>`.
- **Draft all (09; D10).** One authoring session leads an agent team of up to five to draft all
  sections or plans. The drafters coordinate so the documents stay consistent.
- **Reading face (05, 06, 08; D7).** Source Serif 4 is the default body face; Settings offers
  Literata and Piazzolla. Inter sets chrome and every title; Commit Mono Cosmix sets code,
  paths and ids. The kickoff composer and transcript use the reading face.
- **Themes (D6).** Theme choice stays per app: the planner on port 7272 and the dashboard on
  7373 and up each remember their theme in the browser, with no shared server preference store.
- **Bridge (07).** Sessions run as interactive Claude Code in tmux, as loom stages do. The app reads
  the session's transcript JSONL under `~/.claude/projects/` for messages and tool activity, and
  delivers input into the tmux pane. States the transcript does not carry (a permission prompt,
  waiting for input) come from hook events. No pane scraping, and no `claude -p` streaming mode
  (principle 7).
- **Summary storage (04).** Summaries are committed, together with a manifest of the content hashes
  each summary was built from, so a fresh clone opens in concept mode without regeneration. Both
  live under `doc/loom/summaries/<feature>/`, beside the committed knowledge base; plan summaries
  stay keyed by their repository-wide unique slug. (`.loom` is gitignored at
  any depth, so a store there would need a negation rule in every repository.)
- **Summary levels (04, 05).** Two summary levels above the raw text:
  - _Overviews_: two per feature: one for the product, built from
    `doc/product/<feature>/index.md` and its sections; one for the engineering decisions, built
    from its metaplans and plans.
  - _Document summaries_: one per product section and one per plan.

  An overview sentence links to the document summaries it draws on (one or several); a document
  summary sentence links to the heading section of the raw document it draws on. Briefs are
  reached through their plan's summary and through file mode, with no summaries of their own (my
  call). The knowledge base keeps its own hierarchy and is browsed in file mode.

- **Refinement (04, 05, 10).** A refinement edits only the documents under `doc/`. In the same
  turn, the session that made the edit has subagents rebuild the summaries built on the changed
  passages and the overviews that cite them. The background summary sessions handle edits made
  outside the app (an editor, a `git pull`). The reader's view updates where it stands: an open
  source passage changes as soon as its file changes; a summary sentence awaiting its rebuild
  stays readable and is marked out of date, and its replacement flashes once on arrival. Open
  passages are keyed to the source section they came from, so they stay open across a rebuild;
  one whose section was deleted folds back to its parent sentence with a note.
- **Sessions start cold (07, 09).** A new `loom plan` invocation never resumes an earlier
  conversation; it starts a fresh session against the current documents. Once a session has a
  document target, it ends by writing what the conversation settled into the documents: decisions
  into the section they affect, unresolved points as open questions in that same document. Nothing a later session
  needs may live only in a transcript. The final write happens when the browser closes (after a
  grace period, so a reload or a sleeping laptop does not end the session) and when `loom plan`
  is stopped, which waits for every session to finish writing before it exits; a second Ctrl-C
  exits at once. Because a crash or a kill allows no final write, sessions also record each
  decision in the documents as it is made (my call), so the closing write only adds the open
  questions and anything still pending.
- **Embedded editor (06).** File mode embeds CodeMirror 6, loaded as a lazy chunk the way the
  dashboard loads its terminal emulator: Markdown and YAML modes, `@codemirror/merge` for showing
  what a turn changed, themed from the dashboard's CSS tokens. Plan YAML is checked on save by
  loom's own plan validation on the server (my call); schema-aware completion in the editor is
  left out.
- **Concurrent writes (04, 06, 07), my call.** The editor saves only if the file's hash still
  matches the one it opened; otherwise it shows the newer text and the user's unsaved edit side by
  side. The summary store takes one writer at a time behind a lock. Each summary is its own file
  with its own manifest entry, so a git merge of two people's rebuilds conflicts only on the same
  summary, and any conflicted summary is resolved by regenerating it.
- **Spec location (03).** `doc/product/<feature>/`, with sections at
  `doc/product/<feature>/section_NN.md`. Metaplans and feature plans live in
  `doc/plans/<feature>/`; standalone plans live at the top of `doc/plans/`.
- **Approval (10).** Not recorded. Step 6 is the user reading; starting execution is their act.
- **Server (02).** Its own process, default port 7272, well clear of the dashboard's 7373 and up.
- **Command (02).** Bare `loom plan` starts the app; the existing `loom plan` subcommand
  (`verify`) keeps working unchanged. It opens the browser unless it runs headless
  (no display, for example over SSH on a VM), in which case it prints the URL.
- **Rebuild budget (04).** Rebuilds after changes made outside the app run automatically up to a
  threshold of 5 summaries; above it the app shows how many summaries are stale and asks before
  starting the rebuild sessions. Turning off Settings' "Rebuild summaries automatically" in the
  holder's browser sends `POST /api/summaries/auto` and sets the threshold to 0, so every batch
  asks first.
- **One authoring session per project (07, 08).** At most one session can edit a project's
  documents at a time. A project has one `loom plan` server; a second `loom plan` refuses to
  start and prints the running server's URL. Several people can open that URL. While nobody
  holds authoring, any browser that starts a chat, sends a message, saves a file, or starts a
  pressure run takes it; every other browser is read-only: it can read, expand and browse, sees who else is connected and who holds authoring, and can send a passage with a
  note to the holder (section 10), but cannot chat or save.
  Summary sessions run alongside the authoring session.
- **Locked plans (01, 06, 11).** Plan files with the `IN_PROGRESS-`, `DONE-` or `RETIRED-`
  prefix, everything under `doc/plans/<feature>/archive/` or `doc/plans/archive/`,
  run reviews (`REVIEW-*.md`), and the brief
  bundles of locked plans are read-only in the app, in the editor and to sessions, with a notice
  saying why.
- **Product-to-plan links (03, 05, 09).** Each plan names the product sections it implements in a
  plain line, `Implements: section_07` (several separated by commas), which the plan-writing
  skill adds. Concept mode's links between the two overviews come only from these lines, never
  from inference.
