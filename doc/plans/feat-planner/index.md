# Loom Plan — Metaplan

This metaplan cuts the Loom Plan product into loom plans. The product is the browser app that
`loom plan` serves, described in the [vision](../../product/feat-planner/index.md) and its twelve
sections under `doc/product/feat-planner/`. The app takes a project from an empty `doc/` to
reviewed, pressure-tested loom plans: Claude Code sessions in tmux write the documents, and the
reader reviews them in concept mode and file mode.

## Starting point

The plans assume that none of the planner exists yet. Each plan builds its part from the product
spec, reusing only what loom had before the planner: the dashboard behind `loom status --web`
(its HTTP/WebSocket server, embedded SPA build, access model and components), the tmux backend
that runs stages, `loom plan verify`, `loom pressure`, and the brief discovery that `loom run`
uses. The user decided this on 2026-11-19, so that the metaplan plans the whole product instead of
a gap against an implementation that no tracked plan records.

## How the plans are cut

The plans are cut by layer, following the product sections: each plan builds one layer of the
app, and most plans cover whole sections, so each `Covers:` line is short and a plan's scope is
easy to check against the spec. The user chose this on 2026-11-19 over two alternatives. A walking
skeleton first, thickened by later plans, would have shown the risk of joining the tmux bridge,
transcript and chat early, but would have split most sections across two plans and rebuilt parts
of the skeleton. Foundations followed by vertical slices would have given a usable screen sooner
from each plan, with less one-to-one mapping to sections. The accepted cost of the layered cut is
that nobody can author through the app until the bridge, guidance and chat plans are done.

Three placements differ from a strict one-section-per-plan cut, each agreed with the user:

- Section 04's hashing and watcher go to the first plan, because `Settled:` lines and discovery
  need them before any summary exists.
- The shared safe Markdown renderer (section 06, "Rendering Markdown") goes to the web shell,
  because the vision page renders Markdown before file mode exists.
- Concept mode comes before the sessions, because the repository already holds committed
  summaries it can be built and tested against.

Thirteen plans run in the order below. Merging the summary store into concept mode, or kickoff
into chat, was offered and declined: each plan stays one layer.

## Order and what each plan needs

| #   | Plan              | Needs         |
| --- | ----------------- | ------------- |
| 1   | Document model    | none          |
| 2   | Server and access | 1             |
| 3   | Web shell         | 1, 2          |
| 4   | File mode         | 1, 2, 3       |
| 5   | Summary store     | 1             |
| 6   | Concept mode      | 1, 3, 5       |
| 7   | Session bridge    | 1, 2          |
| 8   | Session guidance  | 1, 7          |
| 9   | Chat              | 3, 4, 6, 7, 8 |
| 10  | Summary rebuilds  | 5, 7, 8, 9    |
| 11  | Kickoff and team  | 7, 8, 9       |
| 12  | Review loop       | 4, 6, 9, 10   |
| 13  | Handoff           | 7, 9, 12      |

From plan 9 on a user can author documents through the app; from plan 10 summaries rebuild
themselves.

## 1. Document model and workflow state (`PLAN-planner-document-model.md`)

Builds the server-side model of the `doc/` tree with no UI: discovery of every document, its kind
from path and file name, plan identity by slug across lifecycle spellings, brief discovery through
the code `loom run` uses, run reviews attached by slug, the edge lines (`Covers:`, `Implements:`,
`Implements (stage:<id>):`), `Settled:` lines with their body, stub and upstream hashes, the
findings ledger parse with its checks against the documents, cross-references and GitHub-style
heading anchors, and the planning trace built from all of these. On top of it sit each feature's
six-step checklist, the detection rules, the pressure-test detection from a plan's
`## Pressure tests` section, the progress and settle states of each document, and the recompute
on every watcher change. It also delivers the heading-section hashing and the polling watcher of
section 04, and the `loom plan check` command that prints the checklist and every convention
problem.

Covers: section_03, section_01#the-model-in-one-paragraph, section_01#the-six-steps, section_01#detection-rules, section_01#document-maturity, section_01#skipped-and-reordered-steps, section_01#when-the-state-is-recomputed, section_04#hashing, section_04#the-watcher, section_09#checking-conventions-loom-plan-check

It implements section 03 in full and the state-computing parts of section 01; the offers, the
frontier, the Coverage and Findings pages and the landing rule are drawn by plan 3 and plan 12
from the state this plan computes. It also implements section 04's "Hashing" and "The watcher"
and section 09's "Checking conventions: `loom plan check`". It needs nothing from another plan.
Every later plan reads its model: the server's path checks, the spine, file mode's tree, the
summary engine's citations, the sessions' write limits and the planning sessions' turn-end check.

The risk is that every later plan builds on its types, so a wrong shape here costs the most to
change later. The checklist must stay a pure function of the file tree, with no stored state, and
the hashing must ignore line-ending and trailing-whitespace changes or every editor save would
read as a revision. The test suite should pin each detection rule and each convention problem with
fixture trees, including the skipped and reordered cases section 01 lists.

## 2. Server and access (`PLAN-planner-server.md`)

Builds the `loom plan` command and its server: bare `loom plan` beside the unchanged `verify`
subcommand, the project root at the git top level, the refusal inside a stage worktree, port 7272
with the step to the next free port, one server per project under an OS lock, the browser launch
and headless detection, the runtime state in the per-user directory, the foreground lifetime with
the two-stage Ctrl-C, start without `.loom/work`, and read-only mode when tmux, the `claude` CLI, a
login or the sandbox prerequisites are missing. The dashboard's listener, access, connection and
HTTP parts move into a module both servers share. On the access side it delivers the per-start
token and cookies required by every data route and WebSocket, the write checks (`Host`, exact
`Origin`, constant-time cookie, content type, holder role), remote binds behind their warning, the
authoring holder recorded by browser cookie, display names, presence, the Share link's server
side, the path checks against discovered documents, the symlink rule under `doc/`, and the
request limits.

Covers: section_02, section_12#access-to-the-server, section_12#sharing-access, section_12#browser-identity-and-the-authoring-holder, section_12#display-names, section_12#paths-in-requests, section_12#symlinks-under-doc, section_12#request-limits

It implements section 02 in full and the server-access parts of section 12. It needs plan 1 for
the discovered documents the path checks compare against and for the checklists it pushes to
tabs. Plans 3 and 4 build their pages on its routes and WebSocket; plan 7 needs its holder,
runtime directory and read-only mode before it starts sessions.

The risk is security: this server will start write-capable sessions, so the token, cookie, origin
and holder checks must hold on loopback too, with tests for each refused request. Moving the
dashboard's parts into a shared module must leave `loom status --web` behaving as before, which
its existing tests have to show.

## 3. Web shell, home and spine (`PLAN-planner-web-shell.md`)

Builds the React app the server embeds and its chrome, as the vision's "The screen" describes:
Home with its feature cards, standalone-plan shelf and invitation card; the feature page's header
(logo, eyebrow and switcher, activity pill, Home, Files, presence, Share, theme, Settings, Help),
the spine with its groups, glyphs, counts, settle states and badges, the frontier hints at its
head, and the three-column layout; the primary offer and offer queue of each feature, the pace
rule, and the landing redirect of `/f/<feature>`; the Coverage page; the vision page rendered as
source; the typefaces, reading sizes and Settings; and the shared Markdown renderer that escapes
raw HTML, drops comments, shows unsafe link schemes as text and loads no document images.

Covers: section_01#what-the-app-offers, section_01#the-frontier, section_01#the-coverage-and-findings-pages, section_01#where-a-feature-opens, section_06#rendering-markdown, section_12#untrusted-content-in-the-browser, section_08#share

It implements the offer, frontier and landing parts of section 01, the Coverage page from
section 01's "The Coverage and Findings pages" (plan 12 builds the Findings page), the Share
control of section 08, section 06's "Rendering Markdown" and section 12's "Untrusted content in
the browser". It needs plan 1's checklists and trace and plan 2's server and access. Until plan 7
exists every offer is drawn disabled with the read-only reason the server reports, which is the
spec's own read-only behaviour, so the shell needs no placeholder. Plans 4, 6, 9 and 12 add their
pages and panels to this shell.

The risk is visual drift from the dashboard: the vision asks that both apps look like one product,
so the shell reuses the dashboard's components and colour tokens instead of restyling them. The
renderer is the one place untrusted text becomes markup, so its escaping needs direct tests with
hostile Markdown.

## 4. File mode (`PLAN-planner-file-mode.md`)

Builds file mode: path-based addresses with anchors, the navigation tree grouped into Product,
Plans, Knowledge, Other and Archive and updated live from the watcher, the plan view with its
shared header, Stages panel from the server's parse and "Show YAML", the Edit control for the
authoring browser only with the server deciding editability, the CodeMirror 6 editor loaded as a
lazy chunk with live preview and source modes, saves that write exactly the editor's text after
the hash check and plan validation and atomically, the merge view on a hash mismatch or a watcher
change underneath, plan validation through the code of `loom plan verify` with "Save with
errors", the three diff kinds, and search over titles and document text.

Covers: section_06#the-model-in-one-paragraph, section_06#layout-and-addresses, section_06#the-navigation-tree, section_06#how-plans-render, section_06#viewing-and-editing, section_06#the-editor, section_06#plan-validation, section_06#diffs, section_06#search, section_12#saves

It implements section 06 except "Rendering Markdown" (plan 3) and "Taking a passage into chat"
(plan 9), and section 12's "Saves". It needs plan 1's tree and validation, plan 2's holder and
path checks, and plan 3's shell and renderer. It builds the diff viewer for all three kinds; turn
diffs get their data once plan 9 records what a turn changed. Plan 9 adds "Add to chat" to its
reading column, and plan 12's restore writes through its save path.

The risk is the editor rewriting text the user did not touch, which the vision names as the
reason the rich-text editor was rejected: a test must show that a save of an unedited file leaves
its bytes unchanged. A save racing an outside change must never lose either side.

## 5. Summary store and staleness (`PLAN-planner-summary-store.md`)

Builds the store under `doc/loom/summaries/`: one Markdown file per overview, vision, product
section, metaplan and eligible plan, a summary id as its path without the extension, the summary
file with one cited sentence per line and its front matter of source hashes, the sentence states
and the summary states (current, incomplete, stale, rebuilding, missing) recomputed from hashes on
start and on every watcher change, the eligibility rule that leaves unnamed archived plans
unsummarized, and sealing: agents write only drafts, and `loom plan summary seal` checks the body
rules, resolves every link, computes every hash and writes the file under the store lock, with
claims from the brief command refusing a summary another session holds. Read-only mode keeps
hashing and marking states without starting rebuilds.

Covers: section_04#the-model-in-one-paragraph, section_04#the-store, section_04#the-summary-file, section_04#what-out-of-date-means, section_04#sealing-a-summary, section_04#in-read-only-mode

It implements section 04 except "Hashing" and "The watcher" (plan 1) and the rebuild sessions,
budget and cost (plan 10). It needs plan 1's hashing, watcher and anchors. Plan 6 reads its states
and files; plan 10 drives its commands from summary sessions. Its tests seal hand-written drafts,
since no session exists yet.

The risk is a summary that passes sealing but misstates its source; sealing cannot judge meaning,
so it must at least refuse every sentence whose link does not resolve to a heading section, which
keeps the expansion chain intact (vision principle 3). One writer at a time behind the lock, and
one file per summary, keep git merges of two people's rebuilds to that summary alone.

## 6. Concept mode (`PLAN-planner-concept-mode.md`)

Builds the reader of everything with a summary: the fixed routes per feature for the vision,
product overview, section summaries, metaplan and plan summaries; the metaplan page beside its Run
panel, plan gaps, pace and queue; the document summary page header with breadcrumb, Summary and
Source switch, settle state, cross links and a plan's brief list; one colour mark per cited
document, dashed while out of date; opening a sentence in place into what it cites and a level-1
sentence into source passages headed by chips into file mode; folding back by cap, Esc or the left
arrow; the keyboard model and per-browser view state; marks that turn dashed at once on a source
change and runs that stay open across a rebuild; product-to-plan links from `Implements:` lines
only; the empty and partial states; several viewers; and the visual design and accessibility
rules.

Covers: section_05#levels, section_05#the-overview-and-metaplan, section_05#sentences-and-their-marks, section_05#opening-and-folding, section_05#rebuilds-while-reading, section_05#product-to-plan-links, section_05#empty-and-partial-states, section_05#several-viewers, section_05#visual-design, section_05#accessibility, section_05#what-concept-mode-needs-from-the-summary-data

It implements section 05 except "Selecting text for chat" (plan 9). It needs plan 1's trace and
edge lines, plan 3's shell and plan 5's store and states, and it is built against the summaries
already committed under `doc/loom/summaries/`. Plan 9 adds attaching sentences to chat; plan 10
makes rebuilt summaries arrive without a manual seal; plan 12's review loop reads through it.

The risk is the in-place expansion: an opened sentence must flow in the same paragraph and type,
keep its place across a rebuild, and fold back cleanly, which is where the reading experience the
vision promises succeeds or fails. Every sentence must expand to its source; a sentence that
cannot is a defect the page shows, never hides.

## 7. Session bridge (`PLAN-planner-session-bridge.md`)

Builds the server's ownership of Claude Code sessions in tmux: the four session kinds and their
scopes, the prerequisites check, starting a session with a fresh UUID as its Claude Code session
id, loom's minimal environment and the folder-trust mark, naming, the plan hooks writing to an
event spool under a file lock, tailing the transcript JSONL by uuid, the session state machine
from Starting to Ended and Failed, delivering input only while Idle with queueing and the
UserPromptSubmit confirmation, permission requests through the PermissionRequest hook answered
only by the holder, the web fetch policy, interrupt and end with the bounded final write,
reattaching within the grace period, crash handling, and the item stream the chat receives. On the
safety side it builds the planning-session sandbox: what a session may read and write, the three
enforcement layers (OS sandbox, permission rules, the planning hook that fails closed), the command
and network rules, the guarantee that no session can approve itself, the modes per kind, the
generated settings file, and the checks on paths the server reads that sessions can write and on
input pasted into the pane.

Covers: section_07#session-kinds, section_07#prerequisites, section_07#starting-a-session, section_07#naming, section_07#hooks-and-the-event-spool, section_07#the-transcript, section_07#session-state, section_07#delivering-input, section_07#permission-prompts, section_07#interrupt-and-end, section_07#reattaching-and-resume, section_07#crashes-and-kills, section_07#what-the-chat-receives, section_12#what-a-session-may-read, section_12#what-a-session-may-write, section_12#three-layers-enforce-the-table, section_12#commands, section_12#network-and-web-tools, section_12#no-way-for-a-session-to-approve-itself, section_12#modes-per-kind, section_12#the-settings-file, section_12#reused-from-stages-and-what-differs, section_12#sandbox-prerequisites, section_12#paths-the-server-reads-that-sessions-can-write, section_12#input-delivered-to-the-pane

It implements section 07 except "Kickoff" and "Draft-all team" (plan 11), and the planning-session
and pane parts of section 12 except "Pressure sessions" (plan 13). It needs plan 1's model for the
write table and locked plans, and plan 2's holder, runtime directory and read-only mode. It starts
each session with the brief, opening message and plugin directory its caller passes; plan 8
generates them, and this plan's tests pass fixed ones. Plans 8 to 13 all run on it.

This is the riskiest plan. The transcript format and hook events belong to Claude Code and can
change under it; delivery into a tmux pane must never lose or double a message; and the sandbox
must hold even against a session that tries to write code, approve its own request or reach the
server's port. Tests must start real interactive sessions in tmux (never `claude -p`, vision
principle 7) and show every refused write.

## 8. Session guidance (`PLAN-planner-session-guidance.md`)

Builds what each authoring session is told: the per-session `loom-plan` plugin generated from
sources in the repository and embedded in the binary, the delivery through the appended system
prompt, positional prompt and plugin directory, the common brief with its starting-point block,
the opening messages and the table mapping each offer to its step, target and skill, the
recording-as-you-go rule and the closing message with its `loom-closing` block, the write limits,
the models and efforts with their config resolution, the treatment of the user's `CLAUDE.md`
files, and the step skills for brainstorming, vision, section, metaplan, plan, refine and
knowledge, including the conventions for stubs, link lines and settled documents.

Covers: section_09#the-model-in-one-paragraph, section_09#what-each-session-kind-gets, section_09#delivery, section_09#the-plugin, section_09#the-common-brief, section_09#loom-planbrainstorming-the-discussion-procedure, section_09#loom-planvision-the-vision-discussion, section_09#loom-plansection-one-product-section, section_09#loom-planmetaplan-the-metaplan-or-its-follow-ups, section_09#loom-planplan-one-loom-plan, section_09#loom-planrefine-refining-any-document, section_09#stubs-link-lines-and-settled-documents, section_09#loom-planknowledge-changing-the-knowledge-base, section_09#opening-messages, section_09#recording-as-you-go-and-the-closing-write, section_09#write-limits, section_09#models-and-effort, section_09#the-users-claudemd-files-and-other-ambient-guidance

It implements section 09 except `loom plan check` (plan 1), the summary sessions and summaries
after a turn (plan 10), the kickoff skill (plan 11), and the review and address-findings skills
(plan 12). It needs plan 1 for the starting-point state and `loom plan check`, and plan 7 to start
sessions with what it generates. Plan 9 starts chats with its opening messages; plans 10 to 13 add
their own skills to the same plugin.

The risk is guidance that conflicts with the user's own `CLAUDE.md` or with third-party planning
skills, producing documents outside the conventions of section 03. Each skill's output should be
checked by running `loom plan check` on what a real session writes for each step.

## 9. Chat interface (`PLAN-planner-chat.md`)

Builds the chat panel: its layout as a fixed, resizable column at 1280px and wider and a sheet
below that, message rendering block by block with the typing bubble, tool rows and their folded
groups, the state words in the head, permission cards pinned above the composer for the holder and
a waiting line for others, the composer with Enter to send, drafts that survive a reload, queued
messages and Stop, attachment chips with their source, change and lock warnings, the context line
and prompt pills from the viewed document, the turn footer of changed files with diff links and
the summaries left for rebuild, starting a chat from an offer or a first message, ending a chat
with the closing write, read-only browsers following live, connection errors and accessibility.
It also adds "Add to chat" to concept mode and file mode, and the fixed block the session
receives with each attached passage.

Covers: section_08#the-model-in-one-paragraph, section_08#layout, section_08#message-rendering, section_08#tool-activity, section_08#session-states, section_08#permission-prompts, section_08#composing-and-sending, section_08#attachments, section_08#context-what-the-reader-is-viewing, section_08#what-a-turn-changed, section_08#starting-a-chat, section_08#ending-a-chat, section_08#read-only-browsers, section_08#connection-and-server-errors, section_08#accessibility, section_05#selecting-text-for-chat, section_06#taking-a-passage-into-chat, section_10#what-the-session-receives

It implements section 08 except the `/new` kickoff paragraphs of "Layout" (plan 11), "Share"
(plan 3) and "Pressure runs" (plan 13); section 05's "Selecting text for chat"; section 06's
"Taking a passage into chat"; and section 10's "What the session receives". It needs plan 3's
shell, plan 4's diff viewer and reading column, plan 6's sentences, plan 7's item stream and
delivery, and plan 8's opening messages. Once it is done a user can author every step through the
app; plans 10 to 13 extend the panel.

The risk is a chat that hides what the session did, an anti-goal of the vision: every turn must
list the files it changed with a working diff, and the state words must never say Idle while a
turn still writes. Rendering must keep up with long tool-heavy turns without freezing the page.

## 10. Summary rebuilds (`PLAN-planner-summary-rebuilds.md`)

Builds the sessions that keep summaries current: which agent rebuilds what on which model, the
background summary session that rebuilds only the ids its opening message lists and writes the
overviews itself, the `loom-plan:summaries` skill and the summary and overview agents, the quiet
period of five minutes counted from a turn's end, the threshold of five automatic starts and the
question above it, Rebuild and "Rebuild all" presses that start at once, incomplete summaries
waiting for a press, the Settings toggle and `POST /api/summaries/auto`, the cost line per finished
rebuild and the labelled estimate, and the chat footer's naming of the summaries being rebuilt.

Covers: section_04#who-rebuilds-what, section_04#asking-before-a-large-rebuild, section_04#cost, section_09#summaries-after-a-turn, section_09#summary-sessions

It implements the rebuild, budget and cost parts of section 04 and section 09's two summary
headings. It needs plan 5's store and seal, plan 7's sessions in Manual mode, plan 8's plugin, and
plan 9's footer. Plan 12's "When the summary is wrong" rebuilds through it with a note.

The risk is token cost that surprises the user, an anti-goal of the vision: a rebuild must touch
only summaries whose cited hashes changed, a large batch must ask first, and the cost lines must
add up to what the sessions actually used.

## 11. Kickoff and draft-all team (`PLAN-planner-kickoff.md`)

Builds how new work starts and how a step is drafted in bulk. Kickoff: the `/new` form with its
Feature, Standalone plan and Follow-up switch, the kickoff conversation in the main column with
Taking shape beside it fed by the `loom-kickoff` block, the one-time creation grant armed by the
holder's confirmation for exactly one new target, drafts kept under the runtime directory that
outlive the session and a restart, Home's "Unfinished" list with Continue, Set aside and Discard,
and the `loom-plan:kickoff` skill. Draft-all team: one authoring session leading up to five
in-process teammates to draft all missing sections or plans, with the bridge reading their
transcripts for tokens and assigned documents.

Covers: section_07#kickoff, section_07#draft-all-team, section_09#loom-plankickoff-starting-new-work, section_08#layout

It implements section 07's "Kickoff" and "Draft-all team", section 09's kickoff skill, and the
`/new` and kickoff-draft paragraphs of section 08's "Layout" (plan 9 builds the rest of that
heading). It needs plan 7's bridge and scopes, plan 8's plugin and step skills (the team drafts
with the section and plan skills), and plan 9's transcript rendering and composer.

The risk is the creation grant: it must allow exactly one target that did not exist at session
start and nothing else, so a kickoff can never write over an existing feature. Team drafting must
keep the documents consistent with each other, which the lead checks with `loom plan check`
before ending.

## 12. Review and refinement loop (`PLAN-planner-review-loop.md`)

Builds the review side of the app: the loop from a doubted passage to a revised document, the
session rebuilding a misstating summary with a note while leaving a correct document alone, the
four views of a turn's effect including downstream impact grouped by plan state, Restore of a
modified file to its pre-turn text through the save path, Mark settled writing the `Settled:` line
and committing it (with a plan's brief bundle) through a queue, the Findings page with Dismiss,
Reopen, Address the findings and Review the feature, refinement after plans ran with locked plans
kept read-only, refinement across product and plans, Send to author from read-only browsers and
the holder's handling of suggestions, open-question counts per document, and the per-browser
"what changed since you last looked" checkpoint with comparison against a git ref. It adds the
`loom-plan:review` and `loom-plan:address-findings` skills.

Covers: section_10#the-loop-in-one-paragraph, section_10#the-steps, section_10#when-the-summary-is-wrong, section_10#seeing-what-a-refinement-changed, section_10#approval, section_10#settling-a-document, section_10#triaging-findings, section_10#refinement-after-plans-ran, section_10#refining-across-product-and-plans, section_10#read-only-viewers, section_10#open-questions-in-documents, section_10#what-changed-since-you-last-looked, section_12#suggestions-and-restores, section_09#loom-planreview-reviewing-the-feature, section_09#loom-planaddress-findings-addressing-the-findings, section_01#the-coverage-and-findings-pages

It implements section 10 except "What the session receives" (plan 9), section 12's "Suggestions
and restores", section 09's review and address-findings skills, and the Findings page from
section 01's "The Coverage and Findings pages" (plan 3 builds the Coverage page). It needs plan 1's
`Settled:` lines and ledger, plan 4's save path and diffs, plan 6's reading column, plan 9's chat
and footer, and plan 10's rebuilds. Plan 13 feeds finished plans' findings into the ledger this
plan's page triages.

The risk is that Mark settled commits to git, the one place the app writes history for the user:
it must commit only the document and, for a plan, its brief bundle, and a slow pre-commit hook must
hold up only the settles queued behind it. A settle must never be written by a session.

## 13. Handoff to execution (`PLAN-planner-handoff.md`)

Builds the step from reviewed plans to running ones: the pressure test offered on a written plan's
page and disabled with its reason, the dialog with rounds and each step's model and effort, the
server running the same step pipeline as `loom pressure` with Claude steps as pressure sessions and
the Codex step as a background process, pressure sessions confined to the plan under test, its
brief bundle, `planning-omissions.md` and the completion marker, the chat showing each step and its
plan edits, early ends that write no record, and the change to `loom pressure` that appends the
dated line under `## Pressure tests`. Then the run: the checks `loom init` and `loom run` need, the
run offer following the workspace state, abandoning an unfinished run with its second confirmation,
the run dialog that commits the changed paths under `doc/` and runs `loom init` and `loom run` in
order, the handoff to the `loom status --web` dashboard, plan states and the one-line run summary
while plans run, and the import of a finished plan's open findings and questions into the
feature's ledger without duplicates.

Covers: section_11, section_12#pressure-sessions, section_08#pressure-runs

It implements section 11 in full, section 12's "Pressure sessions" and section 08's "Pressure
runs". It needs plan 7's sessions and sandbox, plan 9's chat for the pressure steps, and plan 12's
Findings page for the imported findings. Nothing depends on it; it closes the six steps.

The risk is that the app's pressure driver and `loom pressure` drift apart, so both must run one
shared pipeline. Starting a run commits and launches a daemon, so every refusal condition (dirty
tree, uncommitted plan or briefs, wrong base branch) needs a test, and abandoning a run must never
discard unmerged `loom/*` commits without the second confirmation.
