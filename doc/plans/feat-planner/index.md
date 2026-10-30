# `loom plan` Engineering Metaplan

This metaplan cuts the `loom plan` product specification (`doc/product/feat-planner/index.md` and
`doc/product/feat-planner/section_01.md` to `section_12.md`, agreed 2026-10-26) into thirteen loom plans and
orders them. It was written on 2026-10-26 on branch `feat-planner-ui` at `b0f91c00`. Every
entry names the product sections it implements, what it delivers, what it needs from earlier
plans, the files it owns, its traps, and its acceptance bar. File and line references come from
the spec and were taken at that revision; a plan author re-reads each named range before writing
stage briefs and anchors every brief by symbol as well as line.

This file is not a loom plan. Each entry heading names the plan file to author later with
`/loom-plan-writer` as `doc/plans/feat-planner/PLAN-<slug>.md`, checked with `loom plan verify`, committed
with its briefs under `doc/plans/feat-planner/briefs/<slug>/` (`loom run` refuses uncommitted plan inputs),
and run with `loom run`. Each plan carries an `Implements:` line under its title naming the
sections below (spec section 03, "Product-to-plan links").

## Objective and binding constraints

The objective is the product the vision describes: `loom plan` opens a browser app, served by
the `loom` binary, that takes a project from an empty `doc/` to reviewed, pressure-tested loom
plans, and lets a reviewer read the result in concept mode and file mode and send corrections
back through a chat with a Claude Code session.

Constraints every plan inherits from the spec:

- **The files in `doc/` are the truth** (vision, principle 1). No plan adds front matter, marker
  comments or sidecar state to documents. Summaries are the only new committed content, under
  `doc/loom/summaries/`, and they are disposable.
- **Subscription, not API** (principle 7). Every Claude session the app starts is interactive
  Claude Code in tmux. No plan uses `claude -p`, stream-JSON, or an API key.
- **Reuse what loom has** (principle 6). The server is built from `loom status --web`'s parts,
  the browser app is a second entry in the same `web/` Vite build, sessions use the tmux lane's
  free functions, plan validation calls `loom plan verify`'s code, search calls the retrieval
  library. A plan that writes a second implementation of any of these explains why the first
  cannot carry it.
- **Nothing in the project outside `doc/`.** Runtime state lives in
  `~/.loom/plan/<project-key>/` (section 02, "Runtime state"). `loom plan` needs no `loom init`.
- **No backwards compatibility or migration code** (project `CLAUDE.md`): loom is unreleased.

Rules for every plan:

- **The gate.** Every code stage runs, from `loom/`, the pre-push gate: `cargo build`,
  `cargo test --all-targets --no-fail-fast`, `cargo test --doc --workspace --all-features --
  --test-threads=1`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` and
  `RUSTDOCFLAGS="-D warnings" cargo doc --workspace --all-features --no-deps`
  (`conventions/git-and-build-workflow.md`). A stage that edits `web/` also runs the web
  project's typecheck, lint, tests and production build with `bun`, and commits the rebuilt
  embedded bundle the way `6eab78fd` did for the dashboard. A stage that edits `loom-hooks/`
  runs the hook suites.
- **Size limits.** 400 lines per file, enforced by the maintainability scanner and the exact
  ledger `loom/maintainability-baseline.txt`, which fails on growth and on shrinkage. New code
  goes in new modules. Moving code out of `commands/status/web/` (plan 2) updates the ledger
  rows of every file it touches in the same commit.
- **Dependencies** are added with `cargo add` and `bun add`, never by editing manifests. Each
  new dependency names the requirement it meets (Discipline B). Expected additions: a CommonMark
  parser with GFM for the browser (plan 3), CodeMirror 6 with `@codemirror/merge` and the
  Markdown and YAML modes as a lazy chunk (plan 3). The Rust side adds none that the spec does
  not already point at (`fs2`, `sha2` through `body_hash`).
- **Security review.** Plans 2, 4, 6 and 7 open write paths into the repository or into a
  running Claude Code process. Each gets a `loom-code-reviewer` security pass in its final
  stage, judged against section 12's threat model.
- **Installed assets.** A plan that edits `skills/`, `agents/`, `commands/` or the new
  `plan-plugin/` edits the repository copies; the operator runs `./dev-install.sh` after merge.
- **Doctrine sync.** Every new CLI surface (`loom plan`, `check`, `summary`, `hook`) gets its
  `completions/dynamic/` entries (except the hidden `hook`) and a section in `README.md` and
  `skills/loom-usage/SKILL.md` in the plan that adds it.

## State of the tree at b0f91c00

What exists, and the gaps the spec's "reuse" claims leave. `LS` is `loom/src`.

| Area | What exists | Consequence |
| --- | --- | --- |
| Dashboard server | `LS/commands/status/web/` (about 9,200 lines with tests): hand-rolled HTTP/1.1 and WebSocket on `httparse` and `tungstenite`, one thread per connection, no async runtime. `serve_with` hardwires the dashboard's `Broadcaster` and `connection.rs::route()`; `execute` calls `WorkDir::load` first. Upgrades route through `bootstrap.rs::route_upgrade`. | Plan 2 makes the accept loop take a router and a publisher, so the dashboard and the planning server differ only in what they pass. `mod.rs` (352), `connection.rs` (352) and `broadcast.rs` (359) leave little room; the extraction moves code out before adding any. |
| Browser launch | None. `--web` only prints its URL (`print_startup`). | Opener and headless detection are new code in plan 2. |
| Asset embedding | `loom/build.rs` → `loom/build/assets.rs::generate_web_assets` walks the committed `web/dist` into `OUT_DIR/web_assets.rs`; `status/web/assets.rs` `include!`s it. Vite writes fixed names (`assets/[name].js`), and the dev proxy targets `127.0.0.1:7373`. | The second entry needs distinct output names and its own dev proxy port. `web/dist` is committed, so every `web/src` change commits a rebuilt bundle. |
| Web wire contracts | `web/src/api/fixtures/*.json` are pinned by Rust tests (`model_tests.rs`, `schema_parity.rs`); zod is imported only from `@/api/zod` (jitless, for the CSP). | The planning wire model gets the same fixture-parity tests from its first plan. |
| tmux lane | `spawn_in_tmux(socket, session_name, cwd, command)` in `orchestrator/terminal/tmux/mod.rs` (355 lines); every `SessionBackend` spawn method takes a `Stage` and a signal path. No code calls `send-keys`, `load-buffer` or `paste-buffer`; the only live write into a pane is the web terminal's PTY running `tmux attach-session`. | Input delivery is new code in its own module (plan 7). Widening `PRESENTATION_OPTIONS` and `teardown_socket` must not grow `tmux/mod.rs`. |
| Session settings | `native/session_settings.rs::write_session_capsule` and `session_settings/contents.rs` (`capsule_hooks`, `SESSION_TOOL_DENIES`) build stage capsules; `native/launch.rs` (378) and `native/wrapper.rs` (377) are near the limit. | Plan 6's builder sits beside them and reuses the safe write, not the stage contents. |
| Hook events | `HookEvent` (`LS/hooks/config.rs:15-32`) has SessionStart, PostToolUse, PreCompact, SessionEnd, Stop, SubagentStart, SubagentStop, TeammateIdle. Loom registers no `UserPromptSubmit` per session, no `PermissionRequest`, no `Notification`. Hooks reach the daemon through files and spools, never a socket. | Plan 7's nine-event registration is written by the planning settings builder; `HookEvent` is not the place for it. |
| Transcripts | `LS/tokens/tail.rs::read_lines` (incremental tail), `tokens/claude_line.rs::parse_line`, `tokens/messages.rs::MessageTable`; `commands/usage/{discovery,transcript}.rs` walk `~/.claude/projects`. Nothing turns a transcript into chat messages. | Plan 7 reuses the tailer and the line parser and writes the chat mapper new. |
| `loom plan` | `PlanCommands` (`LS/cli/types_ops.rs:14-30`) has only `Verify`; `Commands::Plan` at `cli/types.rs:275`, dispatch `cli/dispatch.rs:104-113`. `commands/plan/verify.rs` is 504 lines, its JSON types are private, `soft_checks` is `pub(super)`, and `--json` mode calls `std::process::exit`. | Plan 1 makes the subcommand optional. Plan 3's verify split is a real refactor and moves the tests out to bring the file under the limit. |
| Plan lifecycle | `fs/plan_lifecycle.rs:23-24` knows `IN_PROGRESS-` and `DONE-`. `RETIRED-` exists in no Rust code; `plan/inputs.rs:140-141` and `commands/usage/discovery.rs:317` strip only the two prefixes. | Plan 1 adds `RETIRED-` to the lifecycle module and both strippers. |
| Markdown | No Markdown crate. `fs/knowledge/splice.rs` (`find_section`, `atx_heading`, `fence_mask`) and `fs/knowledge/chunker.rs` (`split_sections`, `fence_marker`, sha256 at `:43`) scan headings fence-aware. | Plan 1 builds heading sections on these scanners. The browser parses Markdown (plan 3). |
| Embedded skills | `build/assets.rs::generate_embedded_assets` embeds `agents/`, `commands/`, `skills/`; `assets/install.rs::install_all` writes them under `~/.claude`. Nothing injects skills per session. | Plan 8 adds `plan-plugin/` to the embedding and renders it per session; it never goes through `install_all`. |
| Pressure | `commands/pressure/{mod,spawn,fallback,models,paths}.rs` (363/169/295/192/129). Claude steps run in the foreground through `claude/session.rs::run_foreground`. `/pressure` and `/address` are slash commands in `commands/`. | Plan 11's shared pipeline keeps `run_foreground` as the terminal driver's step runner. |
| Singleton lock | `fs/daemon_state.rs::acquire_lock` and `inspect_lock` (`fs2` flock, `ProcessIdentity`, `LockState::{Held, Free, Indeterminate}`, fail-closed on symlinks and FIFOs). | Plan 2's server lock reuses this pattern, adding the URL and token to the record. |
| `loom init` | Refuses when `work_state.rs::holds_orchestration_state` is true unless `--clean`. | Plan 11's workspace-state table reads the same predicate. |

Files already over 400 lines that later plans touch: `orchestrator/terminal/emulator.rs` (611)
and `native/pid_tracking.rs` (452) are in the ledger; no plan here edits them.

## Plan map

| # | Plan file (to author)                 | Implements          | Depends on   | First usable result                                   |
| - | ------------------------------------- | ------------------- | ------------ | ----------------------------------------------------- |
| 1 | `PLAN-feat-planner-01-documents.md`              | 01, 03              | none         | `loom plan check` prints the checklist                |
| 2 | `PLAN-feat-planner-02-server.md`                 | 02, 12 (access)     | 1            | `loom plan` serves an authenticated empty shell       |
| 3 | `PLAN-feat-planner-03-file-mode.md`              | 06                  | 1, 2         | a reader for every document, with the editor          |
| 4 | `PLAN-feat-planner-04-summary-store.md`          | 04 (store, CLI)     | 1            | summaries sealed and evaluated from a terminal        |
| 5 | `PLAN-feat-planner-05-concept-mode.md`           | 05                  | 2, 3, 4      | the two overviews, expandable to source               |
| 6 | `PLAN-feat-planner-06-session-sandbox.md`        | 12 (sessions)       | 1            | a settings builder and a path guard, tested alone     |
| 7 | `PLAN-feat-planner-07-session-bridge.md`         | 07                  | 2, 6         | a Claude Code session driven from the server          |
| 8 | `PLAN-feat-planner-08-guidance.md`               | 09                  | 1, 4, 7      | sessions briefed per step with the `loom-plan` plugin |
| 9 | `PLAN-feat-planner-09-chat.md`                   | 08, 10              | 3, 5, 7, 8   | authoring and review from the browser                 |
| 10 | `PLAN-feat-planner-10-summary-rebuilds.md`      | 04 (rebuilds, cost) | 4, 7, 8, 9   | summaries kept current without a manual step          |
| 11 | `PLAN-feat-planner-11-execution-handoff.md`     | 11, 01 (record)     | 2, 7, 9      | pressure tests and runs started from the app          |
| 12 | `PLAN-feat-planner-12-ui-repair.md`             | 05, 06, 08          | 3, 5, 9      | every screen rebuilt to the agreed design             |
| 13 | `PLAN-feat-planner-13-planner-redesign.md`      | 01, 05, 07, 08, 12  | 12           | a spine that opens real content and a contextual chat |

```text
1 documents ─┬─> 2 server ──> 3 file mode ──┐
             ├─> 4 summary store ───────────┼─> 5 concept mode
             └─> 6 session sandbox ─> 7 bridge ─> 8 guidance ─> 9 chat ─┬─> 10 summary rebuilds
                                                                       └─> 11 execution handoff
3, 5, 9 ──> 12 ui repair ──> 13 planner redesign
```

One checkout runs one plan (`loom init` refuses while `.loom/work` holds state), so the plans
run in the order of the table. The order puts a read-only reviewer's app first (plans 1 to 5):
after plan 5 a reviewer can read a project whose summaries were sealed by hand, which proves the
document model and the summary format before any session writes through them. Plans 6 to 9 add
authoring; 10 and 11 close the loop with automatic rebuilds and execution. Plans 12 and 13 repair
and then redesign the web app around reading and a contextual chat, once the first eleven plans
are in use.

Files more than one plan edits, and their order:

| Plans          | Shared file or module                                                  | Order        |
| -------------- | ---------------------------------------------------------------------- | ------------ |
| 2, 3, 5, 7, 9, 10, 11 | the planning server's route table and wire model             | 2 first; each later plan adds its own route module |
| 3, 5, 8 (via 9) | the planning app's entry, shell and keyboard help (`web/src/routes/shell.tsx` pattern) | 3 first |
| 3, 5           | `web/src/index.css` (`--doc-*` tokens in 5; ledger table styles reused in 3) | 3 first |
| 1, 3, 6        | the shared path predicate (locked set, brief-to-plan rule)             | 1 defines, 3 and 6 call |
| 4, 8, 10       | `[plan]` keys in `loom/src/user_config/keys.rs`                        | 4 adds summary keys, 8 adds authoring keys |
| 7, 9, 11       | the session model sent to the browser (section 07, "What the chat receives") | 7 first |
| 2, 11          | `commands/status/web/` (moved by 2; `print_startup` pinned by 11)      | 2 first      |
| 12, 13         | the planning app's shell, feature screen and chat panel under `web/src/plan/` | 12 first |

## 1. Document model and project checklist (`PLAN-feat-planner-01-documents.md`)

Implements: section_01, section_03.

This plan delivers the library every other plan reads `doc/` through, and the first new CLI
surface. It discovers documents under `doc/` with section 03's walk (dotfiles, the summary store
and symlinks skipped, except a symlink whose resolved target is a regular file inside `doc/`;
reads through `fs::safe_read`), assigns each its kind and document id from path and name alone,
resolves plans by slug across the `PLAN-`, `IN_PROGRESS-`, `DONE-`, `archive/` and `RETIRED-`
spellings with the "most advanced state wins" rule and stray-copy flags, parses the vision's
`Specification sections` table, metaplan entry headings, `Implements:` lines, `## Pressure
tests` records and `Open questions` counts, resolves the three cross-reference forms, and
computes the project checklist, current step, and offers of section 01. It splits documents into
heading sections with GitHub anchors (including the app-only `#loom-metadata` and
`#stage:<id>` anchors) and hashes each span with `body_hash`
(`loom/src/context/source_graph/mod.rs:127`) after line-ending and trailing-whitespace
normalisation. The section scan reuses `section_end` (`loom/src/fs/knowledge/splice.rs:162`),
extended to level-1 headings. The polling watcher of section 04 ("The watcher": 1-second walk,
size/mtime/inode compare, batched delivery with rename detection by hash, editor temporaries
skipped) lives here too, because sections 01, 03, 04 and 06 all consume it. It ships
`loom plan check [--json]` (section 09, "Checking conventions"), which makes `loom plan`'s
subcommand optional in clap (section 02, "Command line") without yet adding the server.

Changes outside the new module: `RETIRED-` joins the prefixes in `loom/src/fs/plan_lifecycle.rs`,
and `loom/src/plan/inputs.rs:140-141` and `commands/usage/discovery.rs:317` strip it, so the app
and `loom run` agree on every slug; the heading scan builds on `fs/knowledge/splice.rs`
(`atx_heading`, `fence_mask`) rather than a new parser; and brief discovery
calls `collect_inputs` (`plan/inputs.rs:89`) and `extract_brief_paths`
(`plan/schema/structural_checks.rs:41`) rather than reimplementing them.

Nothing depends on a server, so every rule is unit-testable against fixture trees. Plans 2 to 11
depend on it. The traps: the checklist must be a pure function of the tree (no app state);
headings inside fenced code are not headings; a vision table the parser cannot read is its own
gap and the parser never guesses from prose; `doc/plans/DONE-*` and `REVIEW-*` are gitignored
in this repository and must still be discovered; the watcher must not report the engine's own
summary writes (it exposes a hook for plan 4 to register written hashes).

Acceptance: fixture trees for each row of section 01's state table and each case of "Skipped
and reordered steps", including this repository's own `doc/` (plans with no product spec before
this round). Tests pin the slug resolution table, the stray-copy rule, GitHub anchor numbering
(`## 4.2 Line Grammar` → `#42-line-grammar`, repeats `-1`, `-2`), section extents, the
`Implements:` token forms and their broken and duplicate cases, and the watcher's batch and
rename behaviour. `loom plan check --json` round-trips through a schema test, and exits non-zero
on each convention problem section 09 lists.

## 2. Server foundation and access (`PLAN-feat-planner-02-server.md`)

Implements: section_02, section_12.

This plan makes bare `loom plan` start a server that serves an authenticated, empty planning
app. It moves the dashboard's reusable parts out of `loom/src/commands/status/web/` into a
shared module both commands use: `listener.rs` (port search parameterised by the default port,
7272 here, `bind_first_available_port` widened from `pub(super)`), `access.rs`, `auth.rs`,
`bootstrap.rs`, `connection.rs`, `http.rs` (security headers, CSP), `ws.rs`, and static asset
serving. Because `serve_with` hardwires the dashboard's `Broadcaster` and `route()`, and
`execute` loads `.loom/work` before binding, the shared accept loop takes a router and a
publisher as parameters, and the work-directory load stays in the dashboard's `execute`. The
dashboard keeps its own routes, wire model and broadcaster and must behave identically after
the move. The server lock follows `fs/daemon_state.rs` (`acquire_lock`, `inspect_lock`,
fail-closed on symlinks and FIFOs) with the URL and token added to the record. The Vite config
gives the second entry distinct output names and a dev proxy to 7272, and the planning wire
model gets fixture-parity tests like `schema_parity.rs` from the start.

On top of the shared parts it builds what section 02 and section 12's "Access to the server"
and "People and authoring" require: project root from `git rev-parse --show-toplevel`, refusal
outside a repository and inside a `loom/<stage-id>` worktree; `--port`, `--host` (the
dashboard's parser), `--no-open` with headless detection (`SSH_CONNECTION`, `SSH_TTY`, no
`DISPLAY`/`WAYLAND_DISPLAY`) and the SSH tunnel hint; the runtime directory
`~/.loom/plan/<project-key>/` created 0700, with the project key from the directory name and a
short hash of its canonical path; the exclusive `fs2` lock file (pid, URL, token, mode 0600)
and the "already running" exit that prints the URL; the two port-scoped cookies
(`loom_plan_<port>`, `loom_plan_browser_<port>`), the access cookie on every API route and
WebSocket in every posture, exact `Origin` on loopback too, `Referrer-Policy: no-referrer`, no
CORS headers; the remote-bind warning in the terminal and as a banner; the authoring-holder
model bound to the browser id cookie, public presence ids, display names (32 characters,
control and bidi characters stripped, `inline_safe` before any terminal line), the presence
list, and the Share route; read-only mode when `tmux`, `claude`, the Claude Code login
(`stage_auth_status`, `loom/src/claude/auth.rs:87`) or the Linux sandbox prerequisites
(`sandbox_prerequisite_problem`, `commands/run/sandbox_preflight.rs:56`) are missing; the
lifetime rules (first Ctrl-C drains through a shutdown hook later plans register with, second
exits at once; one terminal line per lifecycle event); section 12's request limits, including
the new 256 KiB WebSocket inbound limit. In `web/` it adds the planning app as a second entry
page of the same Vite build, embedded by the same `build.rs` step, with the header (step
tracker fed by plan 1's checklist, the three tabs, presence, Share) and the 401 screen.

It depends on plan 1 for the checklist the header shows and the watcher it publishes. Every
later plan adds routes to it. The risks are regressions in the dashboard from the move (its
access and terminal tests must pass unchanged, and `print_startup`'s output format is pinned
here because plan 11 parses it), and access holes: the threat-model rows for a hostile web
page, another local user, and a read-only browser taking authoring are the plan's security
acceptance.

Acceptance: integration tests over a real socket for each refusal in section 12's ordered
checks (`Host`, `Origin`, cookie, content type, role), a token in another path's query doing
nothing, cookie names carrying the port, the lock file's mode and the second-instance exit, the
port fallback from 7272, `--port 0`, headless detection, and read-only mode for each missing
prerequisite. The dashboard's existing test suite passes with no edits beyond import paths.

## 3. File mode (`PLAN-feat-planner-03-file-mode.md`)

Implements: section_06.

This plan gives the app its reader and its editor. The browser side: the navigation tree in
section 06's four groups with gaps shown in place, plan states and lock glyphs, brief bundles
under their plans, Archive, the knowledge hierarchy read from `INDEX.md`, the open document's
outline, live updates from the watcher, and `/files/<path>#<anchor>` addresses; a Markdown
renderer in the planning entry bundle (CommonMark with GFM tables, task lists, strikethrough,
autolinks) that never renders raw HTML, drops comments, renders images as links, allows only
`http`, `https`, `mailto`, relative and `#` links, turns exact document paths in inline code
into links, and renders tables in the ledger style; the plan view (header with state, lock
notice, `Implements:` links, pressure date and validation counts; prose; a Stages panel drawn
with `web/src/lib/graph.ts` and a "Show YAML" control); the CodeMirror 6 editor as a lazy chunk
loaded the way `web/src/components/terminal/emulator.ts:134-161` loads xterm, themed from the
dashboard tokens, with local drafts per path and opened hash; the merge view for a file that
changed underneath; the three diff kinds; search (tree filter in the browser, full-text on the
server). The server side: document and tree routes, the save route (raw text body, 1 MiB,
opened-hash check, plan validation before write, exclusive no-follow temp file and rename
through the directory descriptor, the shared path predicate from plan 1), per-turn text pins
that plan 7 drives, uncommitted diffs through `git show HEAD:<path>` with paths after `--`, and
an in-memory search index built from `fs/knowledge/catalog/prose.rs`'s chunker and the BM25
scorer (`context/rank/corpus.rs:76`), widened to done plans and the archive, with quoted
literal queries.

The plan splits `loom plan verify`'s body (`loom/src/commands/plan/verify.rs:341-371`) into a
function over proposed text and a path that returns the structure `--json` serialises, with
`execute` as one caller and the save route as another. Today the file is 504 lines, its JSON
types are private, `checks::soft_checks` is `pub(super)`, and `--json` mode calls
`std::process::exit`; the split makes the result a returned value, moves the tests out first,
and leaves the exit to `execute`. Stage-specific errors map to the stage's
`- id:` line; YAML errors map to the block's line offset.

It depends on plans 1 and 2. Plans 5 and 9 reuse its renderer, its diff views and its "Add to
chat" selection control (which this plan builds and plan 9 wires to the composer). Risks: the
renderer is the main XSS surface, so it gets adversarial fixtures (scripts, event handlers,
`javascript:` links, images, HTML in tables and code); the editor chunk must not enter the
dashboard bundle or the planning entry's initial load; the search index must stay out of the
project (no cache under `.loom/`).

Acceptance: renderer fixture tests for every rule above; route tests for the save route's
refusals (not holder, locked plan, brief of a locked plan, knowledge, summaries, over 1 MiB,
hash mismatch returning the current text, symlink in any component) and for "Save with errors";
a test that the split verify function and `loom plan verify --json` agree on fixture plans; a
bundle check that CodeMirror is absent from both initial bundles; search tests for scope,
literal quotes, and the 50-result cap.

## 4. Summary store and CLI (`PLAN-feat-planner-04-summary-store.md`)

Implements: section_04.

This plan delivers the summary engine without any session: the store layout under
`doc/loom/summaries/` (overviews `product.md` and `engineering.md`, `product/section_<nn>.md`,
`plans/<slug>.md`), which documents get a summary and which overview scope they fall in, the
summary file format (front matter with `kind`, `of`, `file`, `cites`, `members`; one sentence
per line ending in relative links; at most 15 or 30 sentences), sentence ids computed from text,
evaluation of every sentence and summary state (current, incomplete, stale, rebuilding,
missing; conflict markers and failed validation count as missing), the path rewrite on a
same-hash rename with no agent, deletion on source removal, and the four subcommands
`loom plan summary seal|brief|status|release` working without a server. `seal` validates every
body rule, resolves every link and cited sentence, computes every hash, and writes through the
store lock (`fs2` on `summaries.lock`, as `loom/src/quota/shared.rs:130`) with a temp file and
rename through `fs::safe_fs`, so a symlink planted in the store cannot redirect a terminal
`seal`. `brief` creates claim files and prints the rebuild brief, with `--note`. It also adds
the `[plan] document_summary_model`, `document_summary_effort`, `overview_model` and
`overview_effort` keys to the typed registry (`loom/src/user_config/keys.rs`) with the defaults
in section 04, resolved with the existing precedence.

It depends on plan 1 for discovery, anchors, span hashes and the watcher. Plan 5 renders what
it evaluates; plans 8 and 10 drive its CLI from sessions. Clearing the claims of ended sessions
is the server's job and lands in plan 10 with the scheduler.

Risks: the format is the contract between agents and the reader, so the validator is strict and
its messages name the offending line; a rebuild that changes nothing must produce no diff (no
build times or models in front matter); overview `members` hashes ordered sentence ids so link
edits in a document summary do not invalidate overviews.

Acceptance: a fixture project with sealed summaries, then edits that drive each state
transition in section 04's tables; rename and delete cases; `seal` rejections for each body
rule, an unresolved anchor, a stale cited sentence, a duplicate sentence, and an uncited
in-scope document; a symlinked summary path refused; concurrent `seal` runs serialised by the
lock; `brief` refusing a claimed summary.

## 5. Concept mode (`PLAN-feat-planner-05-concept-mode.md`)

Implements: section_05.

This plan builds the reviewer's view: the Product and Engineering tabs at `/product` and
`/engineering`, document summary pages at `/product/<doc>` and `/engineering/<slug>`, the
document key with colour chips, per-sentence marks drawn from eight new `--doc-1` to `--doc-8`
tokens in `web/src/index.css` (light and dark values, away from the amber accent and warning
hues, 3:1 contrast), opening a level-0 sentence in place into its level-1 sentences and a level-1
sentence into inset source passages rendered with plan 3's renderer, fold handles, fold all,
roving keyboard focus with the keys in section 05's table joined to the `?` help dialog, view
state in a jotai atom mirrored to `sessionStorage`, the out-of-date marks, arrival flashes with
the reduced-motion rule, scroll anchoring, the open-state rule keyed to source sections, the
fold-back notes, `Implements:` links in both directions, the empty and partial states with
`EmptyState` and `BusyRoundel`, and the accessibility rules. The server side is the summary
data route and its WebSocket deltas from plan 4's evaluator, with the per-sentence citations
and per-summary state section 05's last section lists.

It depends on plans 2, 3 and 4. It has no session dependency: until plan 10, summaries come from
a hand-run `seal`, and the "build" offers in empty states are disabled with a reason. Plan 9
wires its "Add to chat" and `a` key to the composer.

Risks: the open-state rule across a rebuild (largest overlap of source sections, tie by reading
order) is the subtle part and needs unit tests on the pure function; flowing level-1 sentences
inside a paragraph while passages break it must keep screen-reader structure (`role="button"`,
`aria-expanded`, `role="group"`); no coloured left border or spine on insets
(`conventions/web-dashboard-typography.md`).

Acceptance: component tests for opening, folding, keyboard order into opened runs, the open-state
rule on synthetic rebuilds (kept, moved to the best overlap, folded with each note), marks and
key chips turning dashed on a stale state, an overview never rendering half-built, and the
`Implements:` links including "not stated" and broken names. A browser test on a fixture project
covers one expansion chain from overview to passage to file mode.

## 6. Planning session sandbox (`PLAN-feat-planner-06-session-sandbox.md`)

Implements: section_12.

This plan delivers the confinement every planning session runs under, testable before any
session exists: a settings builder beside `sandbox::settings` (not `generate_settings_json`,
which brings grants planning must not have) that writes section 12's settings file for each
session kind (authoring, summary, pressure) through `native/session_settings.rs:218-257`'s safe
0600 write; the three layers of the write table (OS `denyWrite` for every top-level entry but
`doc`, `.git` included, plus locked plans, run reviews and the archive as literal existing
paths with no nested denies; `allowWrite`/`allowRead` on `drafts/`, `claims/`,
`summaries.lock`; `denyRead` from `policy::mandatory_deny_read` and
`state_root::token_deny_paths` plus `~/.loom/plan`; `Edit(//…)` allow and deny rules in the
absolute form; the five allowed `Bash(...)` commands; `WebSearch` allowed, `WebFetch` asking;
`AskUserQuestion` denied; `disableBypassPermissionsMode`, `autoAllowBashIfSandboxed: false`,
`strictAllowlist` with no domains, `worktree.bgIsolation: "none"`); and `loom plan hook
PreToolUse`, in Rust, which decides every file-tool call against the current tree with the
same path predicate as plan 3's save route, refuses symlinks in any component below `doc/` or
the runtime directory, and fails closed. The pressure kind's narrower write set (plan under
test, its bundle, `planning-omissions.md`, the marker directory and `Bash(touch <marker>)`) is
built here too, so plan 11 only selects it.

It depends on plan 1 for the locked set and the brief-to-plan rule. Plan 7 writes these files
at launch.

Risks: this is the security boundary of the product. Known traps from stage confinement apply:
only `Edit(...)` rules bind (`mistakes/sandbox-write-rules-inert.md`), an `Edit` deny on
`doc/loom/knowledge/` or `doc/loom/summaries/` would become an OS deny and break the two CLIs,
no `Read(...)` deny anywhere, bubblewrap refuses a deny under a read-only bind
(`architecture/execution-containment.md`), and a single leading `/` resolves against the
settings file's directory. `loom map` and `loom knowledge context` must answer from existing
state when `.loom` is write-denied.

Acceptance: snapshot tests of each kind's settings file on a fixture root; the assertions of
`sandbox/config.rs:159` (no bypass, sandbox enabled, no unsandboxed escape) on every planning
settings file; hook tests for every row of the write table, a lock that appears mid-session, a
symlinked component, an unparsable payload (deny), and read denies; a confinement test that
runs `loom knowledge update`, `loom plan summary seal`, `loom map` and `loom knowledge context`
inside a real planning sandbox on Linux (bubblewrap) and macOS, and a raw write to code and to
`.git` that the sandbox refuses.

## 7. Session bridge (`PLAN-feat-planner-07-session-bridge.md`)

Implements: section_07.

This plan lets the server own Claude Code sessions. It starts each session in its own tmux
server through the lane's free functions (`spawn_in_tmux`, `PRESENTATION_OPTIONS`,
`teardown_socket`, `loom_socket_dir` in `loom/src/orchestrator/terminal/tmux/`), widening
their visibility, with `loom-plan-<12 hex>` sockets that `list_loom_sockets` skips; writes the
session directory (`session.json`, `launch.sh` 0700 exec'ing `claude` with the launch line of
section 07 under `agent_session_environment_from` plus `LOOM_PLAN_STATE` and
`LOOM_PLAN_SESSION`, the plan 6 settings file); records the pane's `ProcessIdentity` and uses
it for every liveness check and signal; implements `loom plan hook <event>` for the nine spool
events with the event-name check and locked append, and the blocking `PermissionRequest` with
answer files, the one-hour timeout and every deny reason; tails the transcript (path from
`SessionStart`, complete lines, keyed by `uuid`, unknown records skipped) and the subagent
transcripts into section 07's chat model (a new mapper on the existing tailer,
`tokens/tail.rs::read_lines`, and line parser, `tokens/claude_line.rs`; nothing in the tree maps
transcripts to messages today) with assistant blocks grouped by `message.id`, usage read with
`provider_data/claude.rs:13`, and changed files from edit results; runs the state
machine with its two honesty rules; delivers input, in a new module since no loom code sends
keys to a pane today, with `load-buffer`/`paste-buffer -p`/`send-keys Enter` (never
`load-buffer -w`), control characters stripped first, only while Idle,
with the queue, withdrawal, the 10-second delivery confirmation, the 100 KB cap and the slash
command refusal; interrupts with `Escape`; ends sessions with the closing message, the
10-minute bound and the grace period; recovers live sessions found at start; and serves the
model to browsers as a full snapshot plus deltas, accepting commands only from the holder's
connection. Turn boundaries drive plan 3's text pins.

It depends on plans 2 and 6. The closing message wording and the opening messages come from
plan 8; until then the plan uses fixed test briefs. Plans 9, 10 and 11 consume the model and
the spawn API.

Risks: state honesty (`mistakes/spurious-waiting-for-input.md`: subagent events never move the
session state; a transcript record after the last `Stop` means working); tmux failure modes
(`mistakes/tmux-backend.md`: stderr with exit 0 is failure, tear down a half-started server, a
destructive sweep treats missing evidence as "do not destroy"); delivery races (never type
while working); the pressure kind's in-process teammates are confirmed against the installed
Claude Code in plan 11, not here.

Acceptance: unit tests for the transcript mapper on recorded transcripts (text, thinking, tool
pairs, interrupts, compaction, subagents, `<loom-attachments>` parse-back), the state machine
on event and record sequences including the contradiction cases, the hook commands on mismatched
event names and spool errors, and delivery sanitising. An `#[ignore]` end-to-end test, run by
the operator with a logged-in Claude Code, starts a session in tmux, delivers two messages
(one queued), answers a permission request, interrupts, and ends with a closing write; the
stage records that run in its overview. A test kills the server mid-session and checks the next
start's recovery and its refusal to touch an unverifiable record.

## 8. Session guidance and the `loom-plan` plugin (`PLAN-feat-planner-08-guidance.md`)

Implements: section_09.

This plan writes what sessions are told. It adds `plan-plugin/` at the repository root
(`.claude-plugin/plugin.json` named `loom-plan`; skills `brainstorming`, `vision`, `section`,
`metaplan`, `plan`, `refine`, `retro`, `knowledge`, `summaries`; agents `summary` and
`overview`; the two pressure commands taken from the existing `commands/pressure.md` and
`commands/address.md` with no second copy), embeds it in the binary through
`loom/build/assets.rs::generate_embedded_assets` beside `agents/`, `commands/` and `skills/`
(never installed by `install_all`), and renders it per session into
`<session dir>/plugin/` with the agents' `model` and `effort` frontmatter filled from the
resolved `[plan]` keys and only the parts each session kind gets. It generates `brief.md` (the
common brief's six parts and the starting-point block from plan 1's checklist), the opening
message for each offer in section 09's table, the summary session's brief and opening message,
and the closing message. It adds `[plan] spec_authoring_model`, `spec_authoring_effort`,
`plan_authoring_model` and `plan_authoring_effort` to the typed registry and maps each offer to
spec or plan steps. Outside the plugin, `skills/loom-plan-writer/SKILL.md` and
`references/plan-template.md` gain the `Implements:` line in their skeletons, and
`commands/pressure.md` and `commands/address.md` gain the sentence forbidding edits to the
`## Pressure tests` section.

`skills/brainstorming/SKILL.md` is adapted from the superpowers plugin's `brainstorming` skill
and carries its full MIT notice (Copyright (c) 2025 Jesse Vincent) in the source and in every
rendered copy, per section 09; the adaptation writes only at section 03's paths, makes no
commits, and hands off to nothing.

It depends on plan 1 (checklist, offers), plan 4 (the summary CLI the skills and agents call)
and plan 7 (the launch that consumes the brief and plugin). Plans 9, 10 and 11 start sessions
with it.

Risks: guidance that contradicts the user's `CLAUDE.md` or loom's installed template
(section 09, "The user's `CLAUDE.md` files"): the brief restates only planning-specific rules;
the knowledge skill must carry `commands/distill.md`'s routing without reading
`loom memory pending`; text pinned in more than one surface follows
`patterns/doctrine-cross-surface.md`.

Acceptance: render tests for each session kind's plugin tree, agent frontmatter from resolved
config, the MIT notice present in the rendered brainstorming skill, the starting-point block on
fixture checklists, and each offer's opening message; a test that the pressure command files in
the plugin are byte-identical to `commands/`; `loom plan verify` passing on a plan generated
from the updated plan-writer template with an `Implements:` line. One operator-run session per
skill family (vision, section, metaplan, plan) on a scratch repository confirms the session
loads its skill and writes at the right path; the stage records the result.

## 9. Chat and the review loop (`PLAN-feat-planner-09-chat.md`)

Implements: section_08, section_10.

This plan builds the chat panel and connects review to authoring. The panel: layout, width and
collapse kept per browser, the narrow-screen sheet, message rendering with plan 3's Markdown
renderer, tool rows with summaries, statuses, expansion and grouping, nested subagent rows,
the state line for every session state with `WorkRoundel`, `ActivityRoundel`, `Stamp` and
`HazardPanel`, the tab-title and opt-in desktop notifications, permission cards pinned above
the composer with `y`/`n`, the composer with local drafts, `c` focus, the 100 KB and slash
command checks, queued messages, Stop, attachment chips (at most 10, "changed" and lock
states), turn footers with changed files, Diff links, rebuilt summaries and cost, starting a
chat from offers, New chat or a first message, ending a chat, read-only browsers, the Share
popover, and the connection error states. The review loop's server side: formatting attachments
into the fixed `<loom-attachments>` block (2,000-character quotes, escaping a literal tag in the
user's text), Restore and Restore all through plan 3's save path with the turn-start pin as the
text and the turn-end hash as the opened hash, the "Check these plans" and "Plan a follow-up"
composer fills from `Implements:` lines, the "Changed after … was built" marker from the run
review's `**Generated:**` date and `git log`, Send to author suggestions (1,000-character note,
20 pending, holder-only delivery, sender name from the server's record), open-question counts
on summary pages and tree entries, and "Changed since you looked" with per-browser checkpoints
in local storage, a fourth diff kind, and comparison with a typed ref.

It depends on plans 3 and 5 (selection controls, diffs, summary pages), 7 (the model and its
commands) and 8 (sessions worth talking to). Plans 10 and 11 add rows to its footer and its
pressure-run header.

Risks: the panel renders untrusted text everywhere (tool inputs, outputs, permission bodies);
nothing is inserted as HTML. Restore must be refused while a turn runs and for created or
deleted files. The attachment block is a parse contract with plan 7's mapper; both sides share
one implementation and one fixture set.

Acceptance: component tests for each item type, status, state line and permission outcome in
section 08's tables; the attachment block formatted and parsed back to the same structured
fields, including spans across levels, truncation and escaping; route tests for Restore's
refusals and the merge fallback, suggestions' limits and holder-only delivery, and a read-only
browser's commands refused. A browser test on a fixture session (recorded transcript replayed
through the bridge) covers attach, send, a turn footer, and Restore.

## 10. Summary rebuilds and cost (`PLAN-feat-planner-10-summary-rebuilds.md`)

Implements: section_04.

This plan keeps summaries current without a manual step. It adds the scheduler of section 04's
"Who rebuilds what": after each watcher batch it collects the summaries made stale, incomplete
or missing, starts one background summary session through plan 7 with plan 8's summary brief up
to 5, and above 5 asks every browser with the count, the list and an estimate, answerable only
by the holder (or anyone while authoring is free), counting only newly invalidated summaries
after a decline; one summary session at a time, none while an authoring turn or a pressure run
is active, stopped after its claims are sealed or released, and stopped after 10 minutes without
a seal with its summaries shown as failed. It adds the rebuild buttons, claim clearing for ended
sessions and stopped subagents, and the authoring turn's two-wave rebuild as the end of the
turn-end duty (the skill text is plan 8's; this plan verifies the chat names the rebuilt
summaries). Cost: each finished rebuild's tokens are read from the session and subagent
transcripts with `tokens/claude_line.rs:21` and the streamed-message merge
(`provider_data/mod.rs:10`), attributed per summary through the hook event of its `brief` call,
appended to `summary-cost.jsonl` with unknown fields absent, and estimated per level from the
log. The summary panel shows the running rebuild, the last cost, today and 7-day totals, and
the five-hour and weekly quota meters through the account-shared poller
(`quota/shared.rs:130`) rendered with `web/src/components/quota-meters.tsx`. Read-only mode
starts nothing and says why.

It depends on plans 4, 7, 8 and 9. Plan 11's pressure runs suspend the scheduler while they
run.

Risks: the anti-goal "background summarization whose token cost surprises the user": the
budget question must fire on a first start with no summaries; the scheduler must not race the
authoring session (the watcher sees a turn's edits as they are written); a failed rebuild must
leave the last sealed text readable.

Acceptance: scheduler tests with a stub session spawner for the threshold, the decline rule,
suspension during turns and runs, the 10-minute stop, and claim clearing; cost tests on recorded
transcripts that match `loom status`'s counts for the same rows; an estimate test with an empty
and a populated log. An operator-run rebuild of this repository's summaries records its token
cost in the plan's overview.

## 11. Execution handoff (`PLAN-feat-planner-11-execution-handoff.md`)

Implements: section_11, section_01.

This plan connects planning to `loom pressure` and `loom run`. It moves the pressure pipeline
(step list, model resolution, report path, record writer) out of `loom/src/commands/pressure/`
into a module both drivers use, and adds the `## Pressure tests` record section 11 specifies,
written by the shared pipeline after a run that completed every round, so terminal runs write
it too (section 01's detection depends on it). The app driver runs Claude steps as pressure
sessions through plan 7 with plan 6's pressure settings, `CLAUDE_CODE_EXPERIMENTAL_AGENT_TEAMS=1`
and in-process teammates (confirmed against the installed Claude Code by the implementing
stage), runs `codex exec` as a server child with its identity recorded, shows the run in plan 9's
panel with the Codex status item and teammate token counts, and stops cleanly on Stop, a failed
step, Ctrl-C, or a dead server found at the next start. The completion instruction's text
follows the marker path it is given. The run side: the run dialog (plan choice in section 01's
order, branch, uncommitted changes split at `doc/`, backend, `--clean` notice), the commit of
`doc/` changes with the user's git configuration and hooks and no `--no-verify`, `loom init` and
`loom run` as children with streamed output, the workspace-state offers read with
`get_plan_source_path` and `DaemonServer::is_running`, Abandon run with its two confirmations,
`loom stop`, `loom clean --all` with `LOOM_CLEAN_YES=1` only after the second confirmation, and
the rename to `doc/plans/archive/RETIRED-PLAN-<slug>.md` only once `.loom/work` and the
worktrees are confirmed gone; "Open dashboard" starting `loom status --web` as a child bound to
the same host, the token URL only for the holder; the run strip from `collect_status_data` and
`attention_entries` every 5 seconds; and the offers once plans complete. Outside the app, the
built-in default backend moves from native to tmux (`SessionBackendKind`'s `#[default]`,
`loom/src/models/session/types.rs:93-95`) and `loom init`'s prompt shows tmux as the default.

It depends on plans 2, 7 and 9, and on plan 6 for the pressure settings. Nothing depends on it.

Risks: the record writer edits plans that `loom plan verify` and `loom init` must still accept;
the abandon flow deletes state and branches, so every refusal (`loom clean` declining with exit
0, `refuse_unreviewed_move`) must stop the sequence before the rename; the default-backend
change alters terminal users' behaviour and needs a `README.md` note.

Acceptance: record-writer tests for each placement rule, multi-model rounds, a missing round
report, a moved plan (no write), and a recorded plan passing `loom plan verify` and `loom init`'s
plan preflight; a test that the terminal and app drivers produce the same step list; run-dialog
route tests for each workspace state and blocking condition; abandon tests on a fixture
workspace with merged and unmerged branches, including the declined second confirmation; a test
pinning `print_startup`'s first line. An operator-run pressure test from the app on a scratch
plan writes one record line; the stage records it.

## 12. UI repair (`PLAN-feat-planner-12-ui-repair.md`)

Implements: section_04, section_05, section_06, section_08, section_11.

This plan rebuilds the web app's screens to the agreed design after plans 1 to 11 shipped a UI
that did not match its mockup. It adds the card payload (title, pitch, counts and step labels)
and the screenshot script, moves summary generation to haiku at high effort with three new plan
keys, fixes the opening message that needed a second press, and adds a Codex summary spawner. It
then rebuilds the shell, the feature screens and the reading surfaces over disjoint web files, and
covers the chat panel, the pressure dialog and help.

It depends on plans 3, 5 and 9. Plan 13 depends on it.

Risks: the gates checked structure, CSS values and copy, and none walked a user task
(`doc/loom/knowledge/mistakes/planner-ui-repair.md`, "The Repair Passed Every Gate While the
Feature Screen Stayed Unusable").

Acceptance: the suites pass and a real-browser review in four themes reads each screen against
the mockup.

## 13. Planner redesign (`PLAN-feat-planner-13-planner-redesign.md`)

Implements: section_01, section_05, section_07, section_08, section_12.

This plan makes the feature screen a spine of documents that opens real content, so that concept
mode is the default way to read and a finished feature opens on the product overview. The wire
loses `ChecklistView.title` and the Review step, gains the plan rows, and carries short offer
labels, so a feature is named by its slug everywhere and no screen draws one offer twice. The chat
panel is redesigned, knows the viewed document, offers prompts for it and attaches it to the next
message. Authoring and pressure sessions run in auto mode with a notice for each blocked action,
every session gets an owner watch with a 15-minute grace, and the Rust tests stop starting real
sessions or writing to the operator's home. The spec sections it implements are rewritten to match.

It depends on plan 12 and on nothing else; nothing depends on it.

Risks: plan 12 passed every gate while the screen stayed unusable, so this plan is accepted by a
journey script that walks user tasks; the six-step wire changes fixtures and tests that pinned
seven steps and titles; auto mode moves Kickoff's and Team's asks onto explicit ask rules.

Acceptance: the journey script passes with no console error, the frozen contracts for auto mode,
the owner watch, the six-step wire and test isolation pass, and the operator's feedback on the
built result is taken in a follow-up round.

## Decisions

- **Metaplan location.** This file lives at `doc/plans/feat-planner/index.md`, the
  `doc/plans/<feature>/index.md` metaplan form of the feature model, so its entries count toward step 4.
- **Plan 9's size.** Plan 9 has four stages, under the split threshold, so the review loop stays
  in `PLAN-feat-planner-09-chat.md` and there is no separate review-loop plan.

## Open questions

- **Pressure record early.** The `## Pressure tests` writer is useful to terminal users now and
  is independent of the app. It could move out of plan 11 into a small plan run first.
  Recommendation: keep it in plan 11 unless a pressure test is run on these plans before plan 11
  starts, in which case run it first so their records exist.
