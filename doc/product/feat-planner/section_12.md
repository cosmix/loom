# 12 — Access and Safety

Part of the [Loom Plan product vision](index.md).

`loom plan` serves documents that may be confidential and starts Claude Code sessions that
write into the repository. This section defines who can reach the server and what they can
do, how a browser holds authoring, what a planning session may read, write, run, and reach,
how the server guards its own file access, and how untrusted content is shown in the browser.
It ends with the threat model. Section 02 fixed the frame: the dashboard's access model
(`Host` and `Origin` checks, a per-process token, a port-scoped cookie from `GET /?token=`),
with every write lane needing the cookie in every posture.

## Access to the server

### One token, every data route

Every start mints a fresh token (`terminal/token.rs:12`, 64 lowercase hex characters) and
prints and opens the URL that carries it (section 02). `GET /?token=<token>` with the right
token answers with a redirect to `/` and sets two cookies, both `Path=/; HttpOnly;
SameSite=Strict` and without `Max-Age`, as the dashboard's bootstrap does
(`auth.rs:37-45`):

| Cookie                     | Value                    | Purpose                          |
| -------------------------- | ------------------------ | -------------------------------- |
| `loom_plan_<port>`         | the token                | access                           |
| `loom_plan_browser_<port>` | a random 128-bit id      | which browser this is (below)    |

The names carry the port because cookies have no port scope, the reason the dashboard names
its cookie `loom_dashboard_<port>` (`architecture/web-terminal.md`). A wrong token answers 403;
a token in the query of any other path does nothing.

The access cookie is required on **every API route and every WebSocket, in every posture,
loopback included**: viewing, browsing, search, the presence list, chat, saves, permission
answers, rebuild approval. Only the embedded static build (the SPA shell and its assets) is
served without it, since it holds no project data. A shell that gets 401 from the API shows
one screen: "Open the link `loom plan` printed. Running `loom plan` again in this project
prints it." (Section 02: a second `loom plan` prints the running server's URL.) (My call.) When
the page had been connected before the refusal (the server restarted), the screen leads with
"loom plan restarted. Open the URL it printed in the terminal."

This tightens viewing beyond the dashboard, which leaves viewing open on a remote bind
(`architecture/web-dashboard.md`, Remote access). Product specs and plans are often more
sensitive than stage status, and requiring the cookie costs nothing in the normal flow: the
opened URL already carries the token. On loopback it also keeps other local users of a shared
machine out of the documents.

### Requests that change something

A request that changes anything (an HTTP `POST`, or a command on the WebSocket) passes, in
order:

1. `Host` check against the accepted socket's own address (`AccessPolicy::host_allowed`,
   `access.rs`), which blocks DNS rebinding;
2. `Origin` present and byte-equal to `http://<Host>` in every posture (the dashboard's
   remote rule, applied on loopback too); every WebSocket upgrade, read-only or not, also
   needs it, since WebSockets ignore the same-origin policy;
3. the access cookie, compared in constant time (`token::matches`, `terminal/token.rs:19`);
4. for `POST`, `Content-Type: application/json` (or the save route's text type, below), so a
   cross-site form cannot send it without a preflight;
5. the role check: only the authoring holder's connection may send session commands, saves,
   restores, run and pressure actions, the commit of `doc/`, and "Abandon run" (sections 06, 07,
   10, 11). The one write a read-only browser may send is a suggestion (below).

The server sends no `Access-Control-Allow-*` header on any route. With the cookie required on
every write and `SameSite=Strict`, the dashboard's double-submit CSRF token adds nothing, and
the planning server has none. (My call.)

### Remote binds

`--host` follows the dashboard's rules (section 02): IP literals and `localhost` only, the
`Host` check against the accepted socket, a wildcard bind treated as remote, plain HTTP, no
reverse proxy. A non-loopback bind is allowed, with these additions:

- Startup prints a warning in the manner of `print_remote_warning`
  (`commands/status/web/mod.rs:243`): the server is plain HTTP; anyone who can see the
  traffic can read the token and then read every document and write through sessions; the
  SSH tunnel is the safer way to share.
- The app shows the same warning as a banner to every browser for the life of the process.

A remote bind keeps every write lane (chat, saves, permission answers, runs) over plain HTTP,
behind the same cookie and checks as on loopback. Over SSH, section 02 already prints the
tunnel hint and keeps loopback the default. Nothing in the app suggests `--host`. Refusing
remote binds would break a team reading one server on a trusted LAN; the warning puts the cost
in front of the person who chose it.

### Token lifetime

The token lives as long as the process. There is no expiry and no rotation: restarting
`loom plan` revokes every cookie, and that is the way to cut off everyone who has the link.
The cookies are session cookies, so closing the browser drops them. The token is stored only
in the lock file (mode 0600, section 02) and the process's memory; the runtime directory
`~/.loom/plan/<project-key>/` is created mode 0700.

## People and authoring

### Sharing access

There is one token and one level of access. A second person gets the tokenized URL from
someone who has it. Any authenticated browser can copy it from a "Share" control beside the
presence list; the server returns it only to a request that carries the access cookie, so
sharing reveals nothing that browser did not already hold. Beside the link the
control states where it works:

- on a loopback bind: "Works on this machine. From another machine, tunnel first:"
  followed by `ssh -L <port>:127.0.0.1:<port> <hostname>`;
- on a remote bind: the link as bound, with the plain-HTTP warning.

Everyone with the link has the same rights. Read-only is a role: it means someone else holds
authoring. A separate read-only link was considered and left out: the
vision's reviewer sends corrections back through chat, so a reviewer needs to be able to take
authoring when it is free.

### Browser identity and the authoring holder

The authoring holder (sections 02, 07) is the browser whose `loom_plan_browser_<port>` cookie
value the server recorded when it took authoring. That value is the browser id section 07
refers to. Every tab of one browser shares it, so every tab of the holding browser holds
authoring; the holder is "away" when its last WebSocket closes (section 07's grace period).

The browser id never leaves the server and the browser that owns it. Presence messages carry
a separate public id per browser, random and unrelated to the cookie, so the presence list can
mark the holder without telling anyone the value that would let them claim to be it. A
browser that has the access cookie but no browser cookie (it was cleared) gets a new one on
its next request, which makes it a new browser.

A read-only browser cannot take authoring while it is held, including while the holder is
away within the grace period: the server compares the requesting connection's browser id with
the recorded one. Authoring frees only on the events section 07 lists. Display names are
only labels: a browser that names itself the same as the holder gains nothing.

### Suggestions and restores

Two routes from section 10 sit on this model:

- **Send to author** is a WebSocket command any authenticated browser that does not hold
  authoring may send. It passes checks 1 to 3 and carries the passage with its provenance (a
  path that must pass "Paths in requests" below), a note of at most 1,000 characters, and
  nothing else: the sender's name comes from the server's record of that browser, never from
  the message. The server refuses it while nobody holds authoring, keeps at most 20 pending in
  memory, and sends it only to the holder's connections. A suggestion reaches no session: the
  holder's **Attach** builds an ordinary attachment from the document as the server reads it,
  and the text is delivered only when the holder sends a message. The passage and the note are
  untrusted and render as text (below, "Untrusted content in the browser").
- **Restore** and **Restore all** are saves: holder only, refused while a turn runs, each file
  through the save route below with the turn-end hash as the opened hash, so the shared path
  predicate refuses a locked plan, a path outside the write table, and a symlink exactly as it
  refuses a typed save. The server takes the text to restore from the pin section 06 takes when the
  turn starts (section 07), never from the request.

### Display names

A display name (section 02) is at most 32 characters after the server strips control, format,
and bidirectional characters; an empty or duplicate name is shown as "Viewer N". Names render
as text in the browser and pass through `inline_safe` (`context/untrusted.rs:123`) before
reaching any terminal line. (My call.)

## Planning sessions

Planning sessions run in the main checkout on the user's branch (section 02), as interactive
Claude Code under the user's login (section 07). Their settings file is written by the server
per session (section 07: `settings.json`, mode 0600) and is the only `--settings` source;
`--setting-sources user,project` keeps the user's and the repository's committed settings in
force, as for stages (`orchestrator/terminal/native/capsule.rs:14-28`).

The launch line of section 07 carries `--strict-mcp-config`, as stage launches do
(`capsule.rs:35-37`): no MCP server from the project's `.mcp.json` or the user's
configuration starts in a planning session, since MCP servers run outside the sandbox.

### What a session may read

A session reads the whole repository: writing plans means reading code. It may also read its
own drafts and claims in the runtime directory. It may not read:

- the credential directories and files every stage capsule denies
  (`policy::mandatory_deny_read`, `sandbox/settings/policy.rs:17`, with the codex credential
  included since no planning session runs codex);
- loom's daemon tokens and terminal history, when the project has `.loom/work`
  (`token_deny_paths`, `fs/permissions/state_root.rs:158`, and the `terminal-history`
  directory);
- the planning runtime directory of any project (the `plan/` directory under loom's user
  directory, which follows `LOOM_HOME`, and `~/.loom/plan`) except its own `drafts/`, `claims/` and
  `summaries.lock`: the lock file holds the token, and `sessions/` holds every session's
  spool, launch script, and permission answers.

### What a session may write

`<home>` is `doc/plans/<feature>` for a feature's plans and `doc/plans` for standalone plans.
The locked set covers both, including each home's archive, run reviews and locked brief bundles.

| Path                                                                                                                                        | Authoring                             | Summary                          |
| ------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------- | -------------------------------- |
| `doc/`, outside the rows below                                                                                                              | file tools per scope; commands        | no                               |
| locked set: `IN_PROGRESS-`, `DONE-`, `RETIRED-` plans, `<home>/archive/`, run reviews `<home>/REVIEW-*.md`, and the briefs of a locked plan | no                                    | no                               |
| `doc/loom/knowledge/`                                                                                                                       | only through `loom knowledge`         | no                               |
| `doc/loom/summaries/`                                                                                                                       | only through `loom plan summary seal` | only through `seal`              |
| `~/.loom/plan/<key>/drafts/`                                                                                                                | yes                                   | yes                              |
| `~/.loom/plan/<key>/claims/`, `summaries.lock`                                                                                              | only through `loom plan summary`      | only through `loom plan summary` |
| `~/.loom/plan/<key>/` elsewhere (`lock`, `sessions/`)                                                                                       | no                                    | no                               |
| Anything else in the repository: code, `.git`, `.claude`, `.loom`, root files                                                               | no                                    | no                               |
| The per-user temporary directory Claude Code grants                                                                                         | yes                                   | yes                              |

Briefs of a locked plan follow section 06's reasoning: `loom run` reads briefs from the
committed tree. Code is not writable at all: planning changes documents, and code changes go
through `loom run`. (My call: a planning session that edits code produces changes no stage
reviewed and no plan records.)

Authoring sessions have one of three scopes (`plan_session/scope.rs`):

- **Open** uses the authoring column above and pre-allows `loom knowledge *` and
  `loom plan summary *` in Bash.
- **Kickoff** starts without a target and permits no file-tool write under `doc/` until the
  holder confirms the size and name through the app. The one-time creation grant then permits
  exactly one target: `doc/product/<name>/index.md` for a feature, or
  `doc/plans/PLAN-<slug>.md` plus paths inside `doc/plans/briefs/<slug>/` for a standalone plan.
  The name or slug follows section 03's feature-name rule: `[a-z0-9][a-z0-9-]*`, at most
  64 characters, excluding `archive`, `briefs`, `plans` and `overview`. Confirmation checks
  both the directory snapshot taken at session start and a fresh scan, case-insensitively;
  the target must exist in neither. Symlinked `doc/`, `doc/product`, `doc/plans` and brief
  bundles are refused. An existing feature directory, plan or bundle cannot be reused, even
  if removed during the session. Only one confirmation is accepted. The session continues
  writing its vision or plan within this grant; loading another skill does not widen it.
  Bash pre-allows only read-only `loom plan summary status*` among the `doc/loom/` commands;
  `loom knowledge *`, `loom plan summary seal*`, `brief*` and `release*` ask the holder through
  explicit ask rules (see "Commands").
- **Team**, used by draft-all, permits file-tool writes only to the missing documents the run
  starts for and paths inside their plan brief bundles. Written sections and other documents
  outside that set remain unwritable. The lead and its in-process teammates share the scope
  (section 09). Bash pre-allows `loom plan summary *` for turn-end summaries of the documents
  it drafted; `loom knowledge *` asks the holder through an explicit ask rule.

The server stores the scope and grant in `sessions/<id>/scope.json` in the runtime session
directory. That file is outside the sandbox's writable set and the planning guard denies
file-tool writes to it; the session cannot arm or widen its own grant. The guard reloads the
scope for every document write (`plan_session/guard/mod.rs`). These scopes constrain file
tools. A holder-approved Bash command is bounded by the sandbox's locked set and other OS
write denies, not by the scope; Kickoff and Team do not narrow the OS write set.

Hook commands are outside this table. Claude Code runs them in its own process, outside the
Bash sandbox, so `loom plan hook` writes the session's spool in `sessions/<id>/` (section 07)
while the session's own tools cannot.

### Three layers enforce the table

Claude Code checks the native file tools (Read, Edit, Write, Grep, Glob) against permission
rules and hooks, and runs Bash and everything Bash starts inside its OS sandbox. A rule in
one layer does not bind the other (`sandbox/control_surfaces/session_denies.rs:1-5`), so each
row of the table is enforced where the write can come from.

**1. The OS sandbox (Bash and its children).** Claude Code always lets a sandboxed command
write the working directory, so the settings take that back: at spawn the server lists the
project root and writes a `denyWrite` entry for every top-level entry except `doc`, `.git`
included. It adds the locked plan files, the run reviews in each plan home, each home's
`archive/`, and the briefs of locked plans as they exist at spawn, and
grants `allowWrite` on `drafts/`, `claims/` and `summaries.lock`, which the server creates
before the first session starts. `doc/loom/knowledge/` and `doc/loom/summaries/` stay
OS-writable because `loom knowledge` and `loom plan summary seal` run as Bash subprocesses
inside this sandbox; stages keep the knowledge directory writable for the same reason
(`sandbox/config.rs:12-19`). Every entry is a literal existing path, and no entry lies beneath
another denied directory, since bubblewrap refuses to mount a deny inside a read-only bind
(`architecture/execution-containment.md`). The read denies above go into `denyRead`, with
`allowRead` on the session's own three runtime grants. On Linux srt covers each `denyRead`
directory with a private tmpfs for the length of one command and binds those grants back into it,
so a command can create files under the planning root; they vanish when it exits and never reach
the host's runtime directory.

**2. Permission rules (file tools).** Rules use the absolute `//` form throughout, because
the settings file lives outside the project and a single leading `/` would resolve against
its directory (`architecture/execution-containment.md`, path syntax). Only `Edit(...)` rules
bind file writes; `Write(...)` rules are inert (`mistakes/sandbox-write-rules-inert.md`).

- `allow`: `Edit` on each directory directly under `doc/` except `loom`, enumerated at spawn,
  plus `doc/product/**` and `doc/plans/**` whether or not they exist yet, plus `Edit` on
  each existing top-level regular file of `doc/` by its literal path; `Edit` and `Read` on
  `drafts/**`; `Read` on `claims/**`. No allow rule covers `doc/loom/`, so a file-tool write
  there can never pass silently. `Edit` rule paths match with gitignore semantics, where a
  pattern matching a directory covers everything beneath it, so no wildcard over `doc/`'s
  entries is written: `doc/*` would match `doc/loom`. A top-level file created later prompts.
- `deny`: `Edit` on each locked plan file, on `<home>/REVIEW-*.md`, on
  `<home>/archive/**`, and on each locked plan's brief bundle, in the standalone home and
  every existing feature home. Deny rules on
  `doc/loom/knowledge/` or `doc/loom/summaries/` are impossible: an `Edit` deny also becomes
  an OS write deny and would block the two CLIs. No `Read(...)` deny is written anywhere,
  for the reason stage settings never write one
  (`concerns/sandbox-and-confinement-gaps.md`, No `Read(...)` deny rule).

**3. The planning hook (file tools).** `settings.json` registers
`loom plan hook PreToolUse` for `Read|Edit|Write|MultiEdit|NotebookEdit|Grep|Glob`, with a
10-second timeout. It reads the session's kind from its record and decides each call
against the current tree:

- a write is allowed only inside the write table's file-tool rows for the session's kind
  and, for authoring, its live Open, Kickoff or Team scope;
- the locked-plan test is by name (prefix, a `REVIEW-` run review, `archive/`, a brief of a
  locked plan per section 03's brief-to-plan rule), so a plan locked while the session runs is locked to it at once,
  before the next session refreshes layers 1 and 2;
- a write whose path has a symlink in any component below `doc/` or the runtime directory is
  refused, as is a target that exists and is not a regular file;
- a read, grep, or glob that names a read-denied path is refused (the same lists as
  `denyRead`, from the same constants). The test applies to both the path as named and its
  resolved form, and a path under `/proc` or `/dev/fd` is refused, so a symlink or
  `/proc/self/cwd` cannot reach a read-denied file.

The hook fails closed: any error, an unreadable record, or an unparsable payload answers
deny with the reason. The same path predicate backs the server's save route (below), so
the editor and the sessions cannot disagree on what a locked plan is.

Layer 3 is the precise one; layers 1 and 2 hold if the hook times out, which Claude Code
treats as a non-blocking error. The Kickoff and Team file-tool scope restrictions depend on
layer 3; the other layers retain the broader authoring write set.

### Commands

Sandboxed Bash is not auto-allowed: `autoAllowBashIfSandboxed` is `false` (stages run with it
`true`). A command runs without a prompt when it is in Claude Code's built-in read-only set
(`ls`, `cat`, `grep`, read-only `git`, and the rest) or matches an allow rule, and it asks the
holder when it matches an ask rule. Authoring and pressure sessions run in auto mode, so the
classifier decides any command or tool that no rule lists: a routine call goes through, a risky
one is blocked, and the block shows in the chat as a notice (section 07). Summary sessions run
in `default` mode, where an unlisted command is denied at once. Open authoring sessions have
these allow rules:

```text
Bash(loom knowledge *)
Bash(loom plan summary *)
Bash(loom plan verify *)
Bash(loom plan check*)
Bash(loom map *)
```

Every authoring scope keeps `loom plan verify`, `loom plan check` and `loom map` pre-allowed.
Team also pre-allows `Bash(loom plan summary *)` for turn-end summaries of the documents it
drafted. Kickoff pre-allows only the read-only `Bash(loom plan summary status*)` among these
`doc/loom/` commands. Those commands have no allow rule, and under auto mode the classifier
would decide them, so each gets an explicit ask rule. The rules sit in the `ask` list beside
`WebFetch` and reach the holder through section 07's permission flow (the allow lists are
`AUTHORING_READ_ONLY_BASH` and `AUTHORING_DOC_WRITER_BASH` in `sandbox/permissions.rs`):

| Scope   | Ask rules                                                                                                                       |
| ------- | ------------------------------------------------------------------------------------------------------------------------------- |
| Open    | none added                                                                                                                      |
| Team    | `Bash(loom knowledge *)`                                                                                                        |
| Kickoff | `Bash(loom knowledge *)`, `Bash(loom plan summary seal*)`, `Bash(loom plan summary brief*)`, `Bash(loom plan summary release*)` |

Kickoff does not use `Bash(loom plan summary *)` as an ask: an ask rule outranks an allow rule,
so it would stop the pre-allowed `status*`. The subcommands are those of `loom plan summary
--help` (`seal`, `brief`, `status`, `release`). An approval does not make Bash subject to the
file-tool scope above.

In authoring and pressure sessions every other command goes to the classifier, and the holder
sees a block as a notice. A planning session needs very few commands beyond reading and loom's
own document tools; anything else, `mv` of a plan, a script, a build, is unusual, and the layers
above bound what a classifier-approved call can do: the OS sandbox, the `.git` deny and the
`PreToolUse` guard, which denies or stays silent and never allows, and which still runs in auto
mode. Commands that start sessions or runs (`loom run`, `loom init`, `loom pressure`) are not on
the list and would fail in this sandbox anyway; section 11 runs them from the server.

`loom map` and `loom knowledge context` refresh the source graph cache under
`.loom/cache/context-v1` when they can. In a planning session `.loom` is write-denied,
as it is for checkout-rooted knowledge stages, so a layer either one cannot write is
kept in memory for the rest of the command and answered from there. Acceptance needs a
test that runs each inside a planning sandbox.

### Network and web tools

Sandboxed commands get no network: `strictAllowlist` with no allowed domains, no
`allowLocalBinding`, no `allowUnixSockets`, no `allowAllUnixSockets`. The built-in web tools
run outside the sandbox and follow permission rules:

| Tool        | Authoring                         | Summary |
| ----------- | --------------------------------- | ------- |
| `WebSearch` | allowed                           | denied  |
| `WebFetch`  | asks the holder, per section 07   | denied  |

(My call: vision brainstorming benefits from searching; fetching a page is how injected text
reaches a session and how data leaves it, so a person approves each fetch.)

### No way for a session to approve itself

A session must never answer its own permission request. The channels and what closes them:

- **Answer files** (`sessions/<id>/answers/`): outside the write set in every layer.
- **Its own tmux pane** (typing "yes" into a dialog, or a key that changes the permission
  mode): the tmux socket is a Unix socket, and the sandbox grants none.
- **The server's HTTP port**: loopback TCP goes through the sandbox's network filter, which
  allows nothing; the token is unreadable anyway.
- **Mode changes**: `permissions.disableBypassPermissionsMode` is `"disable"`, and the chat
  refuses slash commands other than `/compact` (section 07). The settings assertion pins the
  mode of each kind (below) and refuses `bypassPermissions` and `acceptEdits` for every kind.
- **Text the server pastes into the pane**: see "Input delivered to the pane" below.

### Modes per kind

| Setting                  | Authoring                                                         | Summary                                | Pressure                         |
| ------------------------ | ----------------------------------------------------------------- | -------------------------------------- | -------------------------------- |
| `--permission-mode`      | `auto`                                                            | `default` (Manual)                     | `auto`                           |
| Unlisted command or tool | decided by the classifier; a block shows as a notice (section 07) | denied at once and logged (section 07) | decided by the classifier        |
| Ask rules                | `WebFetch`; per scope above                                       | none                                   | `WebFetch`                       |
| File-tool writes         | `doc/` per the table and scope, drafts                            | drafts only                            | per Pressure sessions below      |
| Allowed commands         | 5 Open; 4 Team; 4 Kickoff (summary status)                        | `loom plan summary *` only             | five above and completion marker |
| `WebSearch`, `WebFetch`  | see above                                                         | denied                                 | see above                        |
| `AskUserQuestion`        | denied (section 07)                                               | denied                                 | denied                           |
| `SendMessage`            | allowed for draft-all Team only                                   | denied                                 | allowed                          |
| `ListAgents`             | allowed for draft-all Team only                                   | denied                                 | allowed                          |

`SendMessage` and `ListAgents` are allowed for draft-all Team and pressure sessions only;
every other session denies them (`PEER_TOOLS`, `base_denies` in `sandbox/permissions.rs`).

Summary sessions run in Manual mode so that anything outside their rules reaches the
`PermissionRequest` hook, which denies it and logs the request. A logged request means the
rules here are wrong, and the log is how that gets found. Authoring and pressure sessions run
in auto mode: the classifier decides whatever no rule lists, and only an ask rule reaches the
holder. A block arrives through the `PermissionDenied` hook, which the server shows as a notice
and answers with nothing, so no retry is requested (section 07).

The authoring session's refinement subagents (sections 04 and 07: the `loom-plan:summary`
wave, then the `loom-plan:overview` agent) run inside the authoring session with its settings
and hooks; they write drafts and run `seal` like a summary session.

### The settings file

An Open authoring session's `settings.json`, with `<root>` the project root and `<rt>` its runtime
directory, both absolute (hook registrations omitted). An entry holding `…` or `<each other
dir>` stands for the list the server enumerates at spawn: one `denyWrite` entry per top-level
entry but `doc`, and one `Edit` allow per directory under `doc/` other than `loom`, `product`
and `plans`. `<home>` stands for each plan home above; its locked entries and bundle rules
are enumerated too.

```json
{
  "sandbox": {
    "enabled": true,
    "failIfUnavailable": true,
    "autoAllowBashIfSandboxed": false,
    "allowUnsandboxedCommands": false,
    "network": { "strictAllowlist": true },
    "filesystem": {
      "denyWrite": ["<root>/.git", "<root>/loom", "<root>/README.md", "…every top-level entry but doc",
                    "<root>/<home>/IN_PROGRESS-PLAN-x.md", "<root>/<home>/REVIEW-PLAN-x.md",
                    "<root>/<home>/archive", "<root>/<home>/briefs/x"],
      "allowWrite": ["<rt>/drafts", "<rt>/claims", "<rt>/summaries.lock"],
      "denyRead": ["~/.ssh", "…mandatory_deny_read", "~/.loom/plan", "…token_deny_paths"],
      "allowRead": ["<rt>/drafts", "<rt>/claims", "<rt>/summaries.lock"]
    }
  },
  "permissions": {
    "defaultMode": "auto",
    "disableBypassPermissionsMode": "disable",
    "allow": ["Edit(//<root>/doc/product/**)", "Edit(//<root>/doc/plans/**)",
              "Edit(//<root>/doc/<each other dir>/**)", "Edit(//<root>/doc/<each file>)",
              "Edit(//<rt>/drafts/**)", "Read(//<rt>/drafts/**)", "Read(//<rt>/claims/**)",
              "Bash(loom knowledge *)", "Bash(loom plan summary *)", "Bash(loom plan verify *)",
              "Bash(loom plan check*)", "Bash(loom map *)", "WebSearch"],
    "ask": ["WebFetch"],
    "deny": ["…base denies: stage tool denies and Workflow", "AskUserQuestion", "SendMessage", "ListAgents",
             "Edit(//<root>/<home>/IN_PROGRESS-PLAN-x.md)",
             "Edit(//<root>/<home>/REVIEW-*.md)", "Edit(//<root>/<home>/archive/**)",
             "Edit(//<root>/<home>/briefs/x/**)"]
  },
  "worktree": { "bgIsolation": "none" }
}
```

Kickoff and Team omit the two `doc/loom/` writer command grants and carry the ask rules of
"Commands"; Team also omits the
`SendMessage` and `ListAgents` denies. Their document scope lives in `scope.json` and the
guard, not in narrower `Edit` rules.

`worktree.bgIsolation: "none"` is set for the reason stage settings set it
(`sandbox/settings.rs:298-307`): Claude Code would otherwise push the refinement subagents into
worktrees of their own.

### Reused from stages, and what differs

Planning settings come from their own builder beside `sandbox::settings`. Calling
`generate_settings_json` (`sandbox/settings.rs:205`) would bring in grants a planning session
must not have: the package-manager cache writes every stage gets
(`sandbox/settings/policy.rs:165-167`, a write path into the user's later builds), the
orchestration-state reads, and plan-authored `sandbox:` config, which planning has none of.

| Reused as is                                                                 | Where                                     |
| ---------------------------------------------------------------------------- | ----------------------------------------- |
| `failIfUnavailable`, `allowUnsandboxedCommands: false`, `strictAllowlist`    | `sandbox/settings/policy.rs:64-120`       |
| The refusals of bypass mode, a disabled sandbox, and the unsandboxed escape, asserted on every planning settings file | `sandbox/config.rs:159` |
| Tool denies `LSP`, `RemoteTrigger`, `CronCreate`, `PushNotification`, `Workflow` | `SESSION_TOOL_DENIES`, `native/session_settings/contents.rs:35` |
| Credential read denies                                                       | `policy::mandatory_deny_read`             |
| Daemon token read denies                                                     | `state_root::token_deny_paths`            |
| `Edit`, never `Write`; no `Read` deny; literal paths; no nested denies       | `session_denies.rs`, knowledge cited above |
| Settings file written 0600 through `fs::safe_fs`, refusing planted symlinks  | `native/session_settings.rs:218-257`      |
| Linux and WSL sandbox prerequisite check                                     | `sandbox_prerequisite_problem`, `commands/run/sandbox_preflight.rs:56` |
| `bgIsolation: none`, `--setting-sources user,project`, `--strict-mcp-config` | `sandbox/settings.rs:307`, `capsule.rs`   |

| Differs               | Stages                                            | Planning sessions                                                                                 |
| --------------------- | ------------------------------------------------- | ------------------------------------------------------------------------------------------------- |
| Where                 | a worktree (knowledge stages: the checkout)       | the checkout, on the user's branch                                                                |
| Write set             | the worktree, minus control surfaces              | `doc/` and three runtime grants; the rest of the root denied                                      |
| Sandboxed Bash        | runs unprompted                                   | read-only and allowed commands; ask rules ask; the classifier decides the rest                    |
| Network               | the plan's allowed domains                        | none                                                                                              |
| File-tool path guards | `worktree-file-guard.sh`, `credential-guard.sh`   | `loom plan hook PreToolUse`, in Rust, sharing the server's predicate                              |
| Package caches        | writable                                          | not granted                                                                                       |
| `.git`                | writable except hooks, config, sibling admin dirs | denied whole                                                                                      |
| Home control surfaces | denied (`~/.loom/**` among them)                  | outside the write set; `~/.loom/**` is not denied, since a deny would override the runtime grants |

Authoring and pressure sessions share the stages' `auto` permission mode; summary sessions run
in `default` (see "Modes per kind").

The guard scripts key on `LOOM_STAGE_ID` and friends, which a planning session never sets
(section 07), so they do not apply; the planning hook takes their place.

### Pressure sessions

A pressure session (section 11) reviews and edits one written plan. It gets its own settings
file from the same builder, with the authoring column's rules except the write set:

| Path                                                                                 | Pressure session                 |
| ------------------------------------------------------------------------------------ | -------------------------------- |
| the plan under test, `<home>/PLAN-<slug>.md`, and its brief bundle `<home>/briefs/<slug>/` | file tools and commands          |
| `planning-omissions.md` at the repository root                                         | file tools and commands          |
| `<session dir>/done/`, holding the completion marker                                   | commands only (`touch <marker>`) |
| every other path under `doc/`, locked plans and their brief bundles included            | no                               |
| everything else in the repository and the runtime directory                           | no, as for authoring             |

In layer 1 the server leaves `planning-omissions.md` out of the top-level `denyWrite` list,
denies every `doc/` entry except `doc/plans`, and follows the path to the plan's home:
for a feature it keeps only that feature directory under `doc/plans`; inside the home it
keeps only the plan under test and `briefs/`, and inside `briefs/` only that plan's bundle.
Every other existing entry on that path is denied by its literal path. It creates
`<session dir>/done/` before launch and grants `allowWrite` on that directory only, so the
marker is the one write into the runtime directory; `sessions/` stays denied otherwise. In layer
2 the `allow` rules are `Edit` on the plan file, `Edit` on the bundle's `**`, `Edit` on
`planning-omissions.md`, and `Bash(touch <marker path>)`, the exact command
`completion_instruction` gives (`loom/src/commands/pressure/spawn.rs:21-30`). In layer 3 the
planning hook reads the plan under test from the session record and allows file-tool writes to
those three paths only. A plan under test that becomes locked during the run (renamed by hand)
is refused by the hook at once, and section 11's record writer then finds the plan moved and
writes nothing.

The permission mode is `auto`, as for authoring and as for the terminal driver
(`--permission-mode auto`): `WebFetch` asks the holder, who watches the run in the chat and can
answer (section 11, "What the chat shows"), and the classifier decides any other call that no rule
lists, with a block shown as a notice. `/pressure`'s teammates run in the session's own process
(section 11), so they inherit these settings and hooks. Pressure sessions keep `SendMessage` and
`ListAgents`, which `/pressure`'s teammates use.

The Codex step is no Claude session and none of this binds it. The server runs it as the
terminal driver does, `codex exec --sandbox workspace-write` in the project root
(`loom/src/commands/pressure/spawn.rs:58-61`), under Codex's own sandbox, which can write the
whole working tree (below, "Accepted limits").

### Sandbox prerequisites

The settings set `failIfUnavailable`, so on a host whose sandbox cannot start every session
would exit at startup. At start the server runs `sandbox_prerequisite_problem` on Linux and
WSL (bubblewrap, socat, not WSL1); a problem puts the server in section 02's read-only mode,
and the app and terminal name the missing piece, as for missing `tmux` or `claude`. macOS's
sandbox is built in.

## Server-side checks

### Paths in requests

Every path a request names is relative to `doc/`. The server refuses, before touching the
filesystem: an absolute path, a `..` or `.` component, an empty component, a backslash, a NUL
or other control character, more than 1024 bytes, and a path section 03's discovery does not
list. Section 03's discovery is the allowlist; a path that passes the syntax checks but is not
a discovered document is 404.

`doc` itself must be a real directory. A symlinked `doc` is treated as missing, and the app
says why. (My call.)

The routes of section 11 name no path: the run dialog's commit takes the changed paths from the
server's own `git status` under `doc/`, and "Abandon run" acts on the workspace's active plan as
the daemon reads it.

### Symlinks under `doc/`

Documents are read with no-follow opens at every component (`fs::safe_read`,
`read_bounded` and `open_regular_no_follow`, `fs/safe_read.rs:48,152`). A symlink under
`doc/` is shown as a document only when its target, resolved, is a regular file inside the
canonical `doc/` (outside `doc/loom/summaries/`); the server then opens the resolved path
from the `doc/` root with the same no-follow opens, so a swap between the check and the open
fails instead of escaping. Any other symlink, including every symlinked directory, is skipped
by discovery, hashing, search, and the watcher; section 03's walk follows the same rule. A
target anywhere else in the repository is refused because a link to a file such as `.env` would
otherwise publish it to every viewer. (My call.)

### Saves

The save route (section 06) is a `POST` whose body is the file's raw text
(`Content-Type: text/plain; charset=utf-8`), with the path and opened hash in the query, so
the body limit is exact: 1 MiB, the editor's file limit; a longer `Content-Length` is refused
before reading. The server applies the shared path predicate of the planning hook plus
section 06's editor-only conditions (Markdown, under 1 MiB), refuses a path with a symlink in
any component, and writes by creating the temporary file with an exclusive no-follow open in
the target's directory and renaming it over the target through the directory's descriptor.

### Paths the server reads that sessions can write

`doc/`, `drafts/`, and `claims/` are writable by sessions. Every server open of them goes
through `fs::safe_read` or `fs::safe_fs` and not `std::fs`, following the rule for
host-side opens of agent-writable paths
(`patterns/security-sandbox-and-hooks.md`). `loom plan summary seal` writes
`doc/loom/summaries/` the same way: a person can run it from a terminal outside any sandbox
(section 04), where a symlink planted in the store would otherwise redirect the write.

Server-side `git` calls (section 06's diffs) pass paths after `--`, and only paths that passed
the checks above. Sessions cannot write `.git`, so the server's git reads configuration no
session wrote.

The summary CLI inside a session reads only `drafts/`, `claims/`, and `summaries.lock` from the
runtime directory. Checking whether a claim's session is alive, and deleting the claims of
ended sessions, is the server's job (section 04, "Clearing claims"), because session records
are not readable inside the sandbox.

### Input delivered to the pane

Text the server pastes into a session (section 07's `load-buffer`/`paste-buffer`) includes
attachment text from documents, which anyone could have written. Before delivery the server
removes every C0 control character except newline and tab, DEL, and every C1 character, and
turns `\r\n` into `\n`. An escape sequence in a document could otherwise end the bracketed
paste early and type keys into Claude Code's interface. (My call.)

## Untrusted content in the browser

Documents, transcripts, tool inputs and outputs, permission requests, display names, and file
paths are all untrusted: any of them can carry text a session read from a file or a web page.

- **Rendering.** Markdown is rendered with raw HTML never rendered (section 06): comments
  dropped, other HTML shown as escaped text. Transcript text uses the same renderer. Tool
  inputs, outputs, commands, and permission request bodies render as plain text in code
  blocks. Every Unicode format (Cf) character in a permission body, a tool input or result, or a
  path (bidi controls U+202A to U+202E and U+2066 to U+2069, U+200B to U+200F, U+061C, U+FEFF)
  shows as a visible marker such as ⟨U+202E⟩, inside a left-to-right isolated block, so the text
  the holder approves is the text that runs. The transcript keeps the raw characters. Nothing is
  inserted as HTML.
- **Links.** Only `http:`, `https:`, `mailto:`, relative document links, and `#anchors`
  become links; every other scheme (`javascript:`, `data:`, `file:`, `vbscript:`) renders as
  text. External links open in a new tab with `rel="noopener noreferrer"` (section 06).
- **Images.** A Markdown image renders as its alt text and target as a link; the app loads no
  images from documents. (My call: an image URL in a document is a tracking pixel that tells
  a third party who is reading which plan.)
- **Headers.** The CSP stays the dashboard's (`CONTENT_SECURITY_POLICY`,
  `commands/status/web/http.rs:251`): `default-src 'self'`, no `script-src` exceptions,
  `img-src 'self' data:`, `frame-ancestors 'none'`. The planning server adds
  `Referrer-Policy: no-referrer`, so a followed link does not tell the target site which
  document path it came from.
- **Terminal output.** Lifecycle lines that print session names, paths, or display names pass
  them through `inline_safe`.

## Request limits

| Limit                                   | Value                         | Source                                  |
| --------------------------------------- | ----------------------------- | --------------------------------------- |
| Connections, WebSockets                 | 64, of which 48 WebSockets    | `limits.rs`, as the dashboard          |
| JSON request body                       | 8 KiB                         | `MAX_BODY_BYTES`, `http.rs:19`          |
| Save request body                       | 1 MiB of file text            | the editor's file limit (section 06)   |
| WebSocket inbound message or frame      | 256 KiB                       | new; the dashboard's 64 KiB (`limits.rs:36`) is below the 100 KB delivered-text cap, and a send command carries its attachments' provenance and JSON escaping on top of the text |
| Delivered chat text, with attachments   | 100 KB                        | section 07; the server measures it after building the delivered text (section 10) |
| One attachment's text                   | 16 KiB                        | new                                     |
| Search query                            | 256 characters                | new                                     |
| Request path parameter                  | 1024 bytes                    | new                                     |

A body over its limit is refused from `Content-Length` without being read, as the dashboard
does (`http.rs:154-161`). The new figures are my call.

## Threat model

| Who                                                        | Could try to                                                     | What stops them                                                                                     |
| ---------------------------------------------------------- | ---------------------------------------------------------------- | --------------------------------------------------------------------------------------------------- |
| A web page open in the user's browser                      | Read documents or drive a session through the user's browser (CSRF, DNS rebinding) | `Host` check; exact `Origin` on writes and WebSockets; `SameSite=Strict` `HttpOnly` cookie; no CORS headers |
| Another local user, or anyone on the network of a remote bind, without the link | Read documents, start sessions                     | The access cookie on every data route and WebSocket                                                 |
| Anyone who can see traffic to a remote bind                | Read the token in transit                                        | Not stopped: plain HTTP. Loopback default, tunnel hint, startup warning and banner                  |
| A person with the link                                     | Everything the author can do, when authoring is free             | By design; restart `loom plan` to revoke                                                            |
| A read-only browser                                        | Take authoring while held, answer the holder's permission prompts | The holder is bound to the `HttpOnly` browser id, which no other browser ever receives              |
| A read-only browser                                        | Use the dashboard link the app offers to control a run           | Its "Open dashboard" URL carries no dashboard token (section 11); it sees what the dashboard's own rules show a tokenless browser, on loopback the dashboard without its write lanes |
| A planning session, mistaken or steered by injected text   | Edit code, locked plans, the knowledge base or summaries directly | The write table, enforced by the OS sandbox, permission rules, and the planning hook                |
| The same session                                           | Read credentials or the server token                             | `denyRead` for Bash; the planning hook for file tools                                               |
| The same session                                           | Send data out                                                    | No sandbox network; `WebFetch` asks the holder; summary sessions have no web tools                  |
| The same session                                           | Approve its own permission requests                              | Answer files unwritable; no Unix sockets (tmux) or network (server port); bypass mode disabled      |
| A document with escape sequences                           | Type keys into a session through an attachment                   | Control characters stripped before delivery                                                         |
| A document or transcript with HTML, scripts, or images     | Run script in the app, track readers                             | No raw HTML, link scheme allowlist, no document images, CSP                                         |
| A request naming a crafted path, or a symlink under `doc/` | Read or write outside `doc/`                                     | Path syntax checks, discovery as allowlist, no-follow opens, symlinks followed only within `doc/`   |
| A flood of connections or large requests                   | Exhaust the server                                               | Connection caps and body limits                                                                     |
| The repository's committed `.mcp.json`                     | Start a tool server outside the sandbox                          | `--strict-mcp-config`                                                                               |

## Accepted limits

- **Bash writes inside `doc/loom/`.** The two CLIs need OS write access there, so a raw Bash
  write to the knowledge base or the summary store is held back only by the prompt: the holder
  sees the command and can refuse it. A holder who approves it bypasses `loom knowledge`'s
  checks; the summary engine still validates a hand-written summary by its hashes (section 04).
- **New top-level paths.** The root directory itself stays writable, so an approved command can
  create a new top-level file or directory. Existing ones are denied.
- **Locked plans locked mid-session.** Layers 1 and 2 are built at spawn; a plan that becomes
  locked during a session is protected from file tools by the hook at once, and from approved
  Bash commands only from the next session.
- **The repository's committed settings and `CLAUDE.md`.** They apply to planning sessions as
  to any Claude Code session in the repository, stages included; loom does not treat the
  user's own repository as hostile.
- **Readable repository files.** A session can read anything in the repository not on the
  deny lists, including an untracked `.env`, and what it reads can appear in the chat that
  every viewer sees.
- **Cookies without a port.** A different server on the same host name that the user visits
  in the same browser receives the planning cookie. A process running as the same user can
  read the lock file anyway; a hostile second user on the same machine is out of scope.
- **Same-user processes.** Anything running as the user can read the lock file, transcripts,
  and the runtime directory. Loom offers no protection between processes of one user.
- **Tokens in browser history.** The tokenized URL stays in the history of every browser that
  opened it, valid until `loom plan` restarts.
- **The Codex step of a pressure run.** `codex exec --sandbox workspace-write` can write any
  file in the working tree, the terminal `loom pressure`'s behaviour. Its edits outside the plan
  show as uncommitted changes in the next run dialog (section 11), and the step runs only when
  the holder starts a pressure run. One exception is checked: after each Codex step the server
  compares `.claude/`, `.mcp.json` and `CLAUDE.md` with their state before the step. Any change
  ends the run and names the files, and no planning session of any kind starts until the holder
  confirms them. This holds across a restart, because the block is kept in the runtime directory,
  and those files run code in the next session.

## Out of scope for this section

- The lock file, the runtime directory layout, `--host` parsing, and read-only mode: section 02.
- Which paths file mode shows and the editor's own conditions: sections 03 and 06.
- The summary CLI, claims, and the store lock: section 04.
- Session launch, the hook spool, the permission flow, the one-hour deny, and the grace
  period: section 07.
- How permission requests, presence, and the share control look: section 08.
- What the session briefs tell sessions about these rules: section 09.
- How attachments are formatted into delivered text: section 10.
- Running `loom init`, `loom run`, and `loom pressure` from the server: section 11.
