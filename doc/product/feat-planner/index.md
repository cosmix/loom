# Loom Plan — Product Vision

A browser app that takes a project from an empty `doc/` to reviewed loom plans without a terminal.

Status: agreed vision (2026-10-26). Each section listed under
[Specification sections](#specification-sections) becomes its own
`doc/product/<feature>/section_NN.md`.

## Problem

Planning a loom project happens in Claude Code today, in a sequence the user drives by hand:

1. Brainstorm a product vision (`doc/product/<feature>/index.md`) that names the sections the product
   spec needs.
2. Write each product section under `doc/product/<feature>/`.
3. Brainstorm the feature's one metaplan (`doc/plans/<feature>/index.md`): one entry per loom plan,
   three or four paragraphs each.
4. Author the loom plans under `doc/plans/<feature>/`, usually with an agent team.
5. Pressure-test the first plans with `loom pressure`.
6. Review the plans as a human.
7. Start implementation with `loom run`.

Step 6 is the user reading. It records nothing and has no state, so the app has no step for it:
the app numbers six steps (vision, product spec, metaplan, plans, pressure test, run), and the
reading is what its screens exist for.

Two things go wrong. The user has to know this sequence, the skills behind each step, and the
file conventions, and drive all of it from a terminal. And by the review the output is too large
to read: a vision, a dozen product sections, a metaplan, several plans, and a tree of briefs under
`doc/plans/<feature>/briefs/`. A PM or engineer asked to approve that either skims it or gives up. The
review, the moment a human is supposed to catch a wrong direction before agents spend
days building it, is what the current tooling serves worst.

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
  sections) or of its engineering decisions (built from its metaplan and plans). Opening a
  sentence replaces it in place with the more detailed sentences it summarizes, set in the same
  type and flowing in the same paragraph, so the document grows where the reader is looking;
  opening one of those replaces it with the source passage it came from. Any opened passage
  folds back to the sentence it replaced. The user decides how deep to go and where. Nobody has to
  read the full text to form a view, and anybody can reach the full text from any claim in the
  summary. Concept mode is where every document with a summary opens: the feature's spine links
  each section, plan, metaplan and the vision straight to it, with Source on the same page.
  The metaplan has a document summary of its own (Summary levels, below).

Review and authoring connect: from either mode, the user can point at a passage and take it into
a chat ("this plan assumes the daemon is running; it should not"), and the session that answers
edits the documents in `doc/`.

## The screen

The project home (`/`) lists features, each named by its slug, with a pitch from its vision, a
six-cell progress strip and document counts. A feature is named by its slug everywhere in the
app: switcher, eyebrow, cards, breadcrumbs, chat and the browser tab. A document keeps its own
heading. Standalone plans share a shelf with "Add plan" and "Move plan";
they have no six-step checklist or per-plan workflow offers. An invitation card offers "Start a
conversation" at `/new`: the kickoff settles whether the work needs a feature or a standalone
plan before writing anything. A repository with exactly one feature opens on `/f/<name>`.

```text
Logo  feat-planner ▾                     Idle   Home   Files   Settings
───────────────────────┬───────────────────────────┬───────────────────
✓ Vision               │                           │  Chat
✓ Product spec  12/12  │   Reading column          │
    Overview           │   concept mode by default │
    01 Workflow model  │   Source · Edit · Discuss │
    …                  │                           │
    Coverage           │                           │
✓ Metaplan             │                           │
✓ Plans         12/12  │                           │
    Overview           │                           │
    01 documents  done │                           │
    …                  │                           │
✓ Run        12 done   │                           │
    Findings   2 open  │                           │
```

A feature page has one header row above three columns:

- **Header**: one row. Loom's logo, then the identity (the eyebrow "Planning · feature" above a
  feature switcher that shows the slug), the activity pill, Home, Files, presence, Share,
  ThemeToggle, Settings and Help. File mode uses `/files/...`, with the file's feature named in
  the header breadcrumb.
- **Spine**, on the left of every `/f/<feature>` page: a navigation named "Feature documents"
  with one row per group in workflow order (Vision, Product spec, Metaplan, Plans, Run). Each
  row shows a glyph for its state and a count where it has one. Product spec and Plans expand
  into an Overview row and one row per section or plan, a plan with its state and a badge,
  "Pressure-tested <date>" or "Not pressure-tested". The
  rows open the real content, and the spine is the only place a feature's progress is drawn. Each
  document row shows its settle state beside its glyph (draft, revised, settled; section 01).
  Product spec also holds a Coverage row, and Run holds a Findings row with the count of open
  findings. At its head, the **next step** is one sentence and at most one button. A feature with
  no gap and nothing to start shows the sentence only. While a page lists missing sections or
  plans (the pace and queue of section 01), that page owns the button and the spine keeps the
  sentence. Under the sentence sit the **frontier hints**: at most five short lines, one link
  each, naming the documents that need attention next, such as a settled section that changed. A
  hint is a link, never a button, and starts no session. (My call.)
- **Reading column**, centred, at most about 70 characters wide: the vision or a metaplan as
  source, or the overview or document being read in concept mode, expanding in place as described
  above. Each overview sentence carries small colour marks, one per document it draws on, in that
  document's colour; a mark drawn dashed means that summary is out of date and being rebuilt.
- **Chat panel**, a fixed column on the right at 1280px and wider (resizable, 20 to 40rem) and
  a sheet behind the header's Chat button below that; it is never collapsed. It shows the current
  session's conversation. It knows which
  document the reader is viewing, shows it as a context line, offers prompts that fit it, and
  attaches the document to the next message (section 08). Selecting text in the reading column
  attaches the selection to the next message. Each turn lists the files it changed with a
  link to the diff, and names the summaries being rebuilt. The panel offers no step to start,
  only New chat and the context prompts: each offer appears once, in the spine or on the page
  that owns it.

Inter sets the chrome and every title, including document headings and the chat's own chrome and
user messages. Source Serif 4 sets the reading body: concept prose, rendered Markdown paragraphs,
lists and tables, assistant chat text, quoted chat passages and feature pitches. Settings also offers Literata and Piazzolla;
all three reading faces cover Greek. Reading size is 17, 19 (default) or 21px, with line height
1.6 and a measure of 68–70 characters. Code, paths and ids use Commit Mono Cosmix. The visual
design shares the dashboard's components and colour conventions.

### What the first screen shows

The app works out each feature's step from its directories under `doc/` on every start; it
stores no step of its own. A repository with no feature opens on Home's invitation to start a
kickoff conversation. Opening `/f/<feature>` lands on the page where the feature's work is:

| What exists for the feature     | The feature opens on                        |
| ------------------------------- | ------------------------------------------- |
| No product spec and no metaplan | The vision page, with the offer to write it |
| A product spec                  | The product overview                        |
| No product spec, a metaplan     | The engineering overview                    |

A feature with a product spec opens on the product overview, where its reading starts. The
spine's next-step sentence suggests what to do and never forces it; accepting an offer opens a
new session briefed for that step. Refining is available at every step, including after
everything exists. How "finished", "pressure-tested" and "listed sections" are detected from the
files is for section 01 to settle.

## Principles

1. **The files in `doc/` are the truth.** Every document stays plain, committable Markdown (and
   plan YAML where loom requires it) in the paths and conventions loom already uses. A person or
   an agent that has never heard of `loom plan` can read and edit them with any editor, and the
   app picks up those edits. Edge lines (`Covers:`, `Implements:`), `Settled:` lines and the
   findings ledger are plain Markdown lines in those files that a reader can follow the same way.
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
5. **Loom drives the workflow; the user drives the content.** The app knows its six steps,
   shows where each feature stands in the spine, and offers the next one once. Reading, the
   review a human does, has no step: the screens exist for it. What the documents say comes from
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
- **The reviewer**: a PM, lead, or engineer who did not write the plans and has to approve them
  before a run. They want to understand the shape of the work in minutes, drill into the parts they
  doubt, and send corrections back without reading everything. They settle the documents they
  have agreed to, and see later which of those changed.
- **The returning author**: the same person weeks later, after some plans have run, coming back to
  revise the spec or add plans in light of what was built. They see what changed since each
  document was settled, which spec headings no plan covers, and what the runs and their own
  feature reviews found.

## Goals

- A user can run all six steps from the browser, with no terminal beyond `loom plan` itself,
  and read what each step produced without leaving the feature's spine.
- Every offer appears once on a screen, with a short label; the sentence beside it explains.
- The chat knows which document the reader is viewing and adapts its prompts to it.
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
  file mode embeds a Markdown editor for direct fixes (live preview over the source text, with
  a toggle to plain source; it never rewrites text the user did not touch), and a saved edit goes through the same
  hash check and summary rebuild as any other change made outside a session.
- Replacing `loom status --web` for monitoring running stages. The Run step hands off to
  `loom run` and the existing dashboard.
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
| 01  | Workflow model and project state | The six steps as states; how the app infers the current state from the `doc/` tree; what "next step" the app offers in each; how a project that skipped or reordered steps reads   |
| 02  | `loom plan` command and server   | CLI surface, flags, port 7272 and browser launch, lifetime, code shared with `loom status --web`, behaviour without `.loom/work`                                                   |
| 03  | Document model and conventions   | Which paths hold which documents (`doc/product`, `doc/plans`, briefs, knowledge); discovery rules; how a document declares its kind and its parent; cross-references               |
| 04  | Summary engine                   | Content hashing, the hash manifest, the invalidation graph, which agents regenerate what and on which model, the store under `doc/loom/summaries/`, cost                           |
| 05  | Concept mode                     | The two overviews, document summaries, sentence-to-source anchoring, product-to-plan traceability, what the reviewer sees at each level                                            |
| 06  | File mode                        | Navigation, rendering of Markdown and plan YAML, briefs and knowledge, the CodeMirror editor, save-time plan validation, diffs, search                                             |
| 07  | Session bridge                   | Starting and owning Claude Code sessions in tmux; reading transcript JSONL as structured messages; delivering input; permission and waiting states; several sessions; resume       |
| 08  | Chat interface                   | Message rendering, tool activity, session states, attaching document passages as context, showing the files a turn changed                                                         |
| 09  | Session skills and guidance      | Skills and instructions per step (vision, sections, metaplan, plans, pressure), how loom injects them, how they keep output in the conventions of section 03                       |
| 10  | Review and refinement loop       | Questioning a passage, taking it into a chat, seeing what a refinement changed; reading is the review, with no step; the reader settles what they agreed to                        |
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
  A kickoff persists as a draft that outlives its session and a server restart: `/new` always
  starts a fresh one, Home lists the rest as "Unfinished", and a draft can be renamed, set aside,
  continued (a new kickoff carrying its name, size, shape and attachments) or discarded.
- **Draft all (09; D10).** One authoring session leads an agent team of up to five to draft all
  sections or plans. The drafters coordinate so the documents stay consistent.
- **Reading face (05, 06, 08; D7).** Source Serif 4 is the default body face; Settings offers
  Literata and Piazzolla. Inter sets chrome and every title; Commit Mono Cosmix sets code,
  paths and ids. The kickoff composer and transcript, and the assistant's text in every chat,
  use the reading face.
- **Themes (D6).** Theme choice stays per app: the planner on port 7272 and the dashboard on
  7373 and up each remember their theme in the browser, with no shared server preference store.
- **Bridge (07).** Sessions run as interactive Claude Code in tmux, as loom stages do. The app reads
  the session's transcript JSONL under `~/.claude/projects/` for messages and tool activity, and
  delivers input into the tmux pane. States the transcript does not carry (a permission prompt,
  waiting for input) come from hook events. No pane scraping, and no `claude -p` streaming mode
  (principle 7).
- **Permission mode (07, 12).** Authoring and pressure sessions run in auto mode: allow rules
  run, explicit ask rules reach the holder, and a classifier decides what is left. A block shows
  in the chat as a notice. Summary sessions stay in Manual, where an unlisted tool is denied at
  once.
- **Sessions cannot outlive their server (07).** A watch started with every session tears it
  down when no `loom plan` server has held the project for 15 minutes. A server that restarts
  within that time keeps the session.
- **Summary storage (04).** Summaries are committed, together with a manifest of the content hashes
  each summary was built from, so a fresh clone opens in concept mode without regeneration. Both
  live under `doc/loom/summaries/<feature>/`, beside the committed knowledge base; plan summaries
  stay keyed by their repository-wide unique slug. (`.loom` is gitignored at
  any depth, so a store there would need a negation rule in every repository.)
- **Summary levels (04, 05).** Two summary levels above the raw text:
  - _Overviews_: two per feature: one for the product, built from
    `doc/product/<feature>/index.md` and its sections; one for the engineering decisions, built
    from its metaplan and plans.
  - _Document summaries_: one for the vision (at most 8 sentences in at most 3 paragraphs), one per
    product section, one for the feature's metaplan and one per plan. No overview cites a metaplan's summary: an
    engineering overview sentence that draws on a metaplan links to its source passage. A plan's
    summary covers the plan's brief bundle: it has one detail paragraph per stage, citing the
    stage and the briefs that stage's work rests on.

  An overview sentence links to the document summaries it draws on (one or several); a document
  summary sentence links to the heading section of the raw document it draws on, and a plan
  summary sentence also to the stage and briefs it draws on. Briefs are listed on their plan's
  pages and opened in file mode; they have no spine rows of their own, because the spine's
  column is too narrow to hold a bundle (my call). Summaries are built for drafts as well as
  settled documents, since a long plan needs one while it is still a draft. The knowledge base
  keeps its own hierarchy and is browsed in file mode.
- **Layers (01, 04, 05).** A document has three layers. The _stub_ exists before the document and
  sets its scope: a product section's stub is its row in the vision's table, a plan's stub is its
  metaplan entry. The _body_ is the document. The _summary_ is generated from the body for human
  review. A stub never replaces a summary and a summary never replaces a stub. A session that
  changes a body also updates its stub in the same turn, or asks; no agent judges in the
  background whether a body has outgrown its stub (my call).

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
  dashboard loads its terminal emulator: Markdown and YAML modes, a live preview mode that draws
  the Markdown styled while the user edits the text itself, with a toggle to plain source
  (decided 2026-11-06; a rich-text editor that rebuilds Markdown on save was rejected because it
  rewrites untouched text), `@codemirror/merge` for showing
  what a turn changed, themed from the dashboard's CSS tokens. Plan YAML is checked on save by
  loom's own plan validation on the server (my call); schema-aware completion in the editor is
  left out.
- **Concurrent writes (04, 06, 07), my call.** The editor saves only if the file's hash still
  matches the one it opened; otherwise it shows the newer text and the user's unsaved edit side by
  side. The summary store takes one writer at a time behind a lock. Each summary is its own file
  with its own manifest entry, so a git merge of two people's rebuilds conflicts only on the same
  summary, and any conflicted summary is resolved by regenerating it.
- **Spec location (03).** `doc/product/<feature>/`, with sections at
  `doc/product/<feature>/section_NN.md`. The metaplan and feature plans live in
  `doc/plans/<feature>/`; standalone plans live at the top of `doc/plans/`.
- **Approval (01, 03, 10).** Approval is a document-level settled mark. The reader writes it from
  the document's page as a `Settled:` line in the document (section 03), covering the document's
  text and, for a plan, its whole brief bundle: a plan is settled together with its briefs. Only
  the reader's request writes the line; sessions never do. A settled document that changes reads
  as revised, and its page offers the changes since the settled version. Reading itself has no
  step in the app, and starting execution is the user's act.
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
- **Product-to-plan links (03, 05, 09).** Links may point at sections or at headings
  (`section_04#rebuild-budget`). Each plan names what it implements in a plain line,
  `Implements: section_07, section_04#rebuild-budget` (several separated by commas), which the
  plan-writing skill adds. A metaplan entry carries a `Covers:` line naming what its plan will
  deliver. A stage that delivers a narrower part carries an `Implements (stage:<id>): ...` line in
  the plan's prose, because stage ids live in the plan's YAML, which gains no field (my call).
  The link from a brief to its stage comes from the bundle path, `briefs/<slug>/<stage-id>/`.
  Concept mode's links between the two overviews, and the planning trace, come only from these
  lines, never from inference.
- **Findings (01, 10, 11).** Each feature keeps one ledger, `doc/plans/<f>/findings.md`, in plain
  Markdown. A completed plan's run review adds its open findings and open questions to it, and a
  Review the feature session adds what the reader saw while running or reading the feature. The
  reader triages each finding on the Findings page: plan it, fold it into a metaplan entry whose
  plan has not started running, or dismiss it with a reason. Run reviews stay the unchanged
  evidence; the ledger holds the triage state. Review the feature can start at any point once a
  plan has run (my call: it is a secondary offer, owned by the Findings page).
- **Planning trace (01, 03, 05).** The trace of how the documents relate (vision rows, sections
  and their headings, metaplan entries, plans, stages, findings) is built from the edge lines,
  `Settled:` lines and ledger on every load. It stores nothing outside the documents. It is
  separate from `loom map`'s source graph: no code symbol is a node in it and it reads no state
  of that graph.
