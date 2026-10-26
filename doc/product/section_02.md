# 02 — `loom plan` Command and Server

Part of the [Loom Plan product vision](index.md).

This section defines the command line, how the server binds and announces itself, how the
browser is opened, how long the process lives, what it shares with `loom status --web`, and how
it behaves in a project that has never run `loom init`.

## Command line

```text
loom plan [--port <PORT>] [--host <HOST>] [--no-open]
loom plan verify <PLAN> [--strict] [--json] [--no-color]
loom plan summary seal|brief|status|release
loom plan hook <event>
```

Bare `loom plan` starts the app. The existing subcommand, `loom plan verify`, keeps its
arguments and behaviour; any subcommand added later sits beside it the same way. In clap terms
the subcommand becomes optional and the app's flags belong to `loom plan` itself.

| Flag            | Meaning                                                                                      |
| --------------- | -------------------------------------------------------------------------------------------- |
| `--port <PORT>` | Bind exactly this port; `0` asks the OS for a free one. Without it, see "Port" below.        |
| `--host <HOST>` | Bind this address instead of `127.0.0.1`. Same parser and rules as `loom status --web --host` |
| `--no-open`     | Print the URL and do not open a browser                                                      |

The port is a flag, not a positional argument as in `loom status --web [PORT]`, because a
positional value after `loom plan` would collide with subcommand names. (My call.)

There are no config-file keys for any of these, matching the dashboard, which takes its bind
host from the CLI only. (My call.) Shell completion tables (`completions/dynamic/`) gain the
three flags and the `summary` subcommands; the hidden `hook` is left out.

`loom plan summary seal|brief|status|release` is how sessions and people build and claim
summaries: `brief` prints what a summary is built from, `seal` validates and writes it,
`status` lists what is stale, and `release` gives up a claim. Section 04 owns it.
`loom plan hook <event>` is the command Claude Code runs for hook events in planning sessions,
which feeds the session bridge. It is hidden from help, and section 07 owns it.

## Project root

`loom plan` works on the repository that contains the current directory: the root is
`git rev-parse --show-toplevel`, so running it from a subdirectory opens the whole project.
Outside a git repository it exits with an error naming the directory; the documents are meant
to be committed, and the summary store (section 04) records content hashes that assume a
repository.

Inside a loom stage worktree (a checkout on a `loom/<stage-id>` branch) it refuses and names
the main checkout to run it from. Planning edits `doc/` on the branch the user works on; a
stage worktree is merged and deleted by loom, and edits there would be lost or merged as stage
work. (My call.)

## Port

Bare `loom plan` tries 7272 and moves to the next port only when the address is in use, the
same rule `loom status --web` applies from 7373 (`listener.rs`, `bind_first_available_port`).
Two projects planned at once settle on different ports. An explicit `--port` binds exactly
that port and reports the bind error as is. The printed and opened URL always carries the port
actually bound.

## One server per project

A project has at most one `loom plan` server. The server is the backend; people share it by
opening its URL in their browsers.

On start, `loom plan` takes an exclusive OS file lock on a lock file in the per-user runtime
directory (below) and holds it for its lifetime; the file records the process id, the bound
URL, and the token. A second `loom plan` on the same project finds the lock held, starts
nothing, and exits non-zero with the running server's URL:

```text
loom plan is already running for this project (pid 4242).
Open http://127.0.0.1:7272/?token=…
```

The lock file is readable only by its owner (mode 0600), since it holds the token. An OS lock
is released with its process, so a crashed server leaves no stale lock to clean up.

## Several people, one author

Any number of people can open the server's URL. The first browser to start a chat holds
authoring; every other browser is read-only: it reads, expands, and browses, but cannot chat
or save. Authoring passes on when the holder's session ends (the grace period after its browser
closes, section 07), and the next browser to start a chat takes it.

Every browser sees who else is connected: a presence list in the header names each connected
browser, marks the one holding authoring, and shows the document each is reading. Read-only
browsers see the authoring chat as it happens, without an input box. There are no accounts:
a browser names itself on first open (kept in that browser), else it shows as "Viewer N".
(My call.) Sections 07 and 08 own the chat side of this; section 12 owns how a person reaches
the server from another machine.

## Runtime state

`loom plan` writes nothing into the project except documents under `doc/` (and what sessions
write there). Its runtime state lives in a per-user directory:

```text
~/.loom/plan/<project-key>/
  lock            server lock (pid, URL, token; mode 0600)
  sessions/       session registry and hook event spool (section 07)
```

`<project-key>` is the project directory's name plus a short hash of its canonical path, so two
checkouts of one repository are two projects. (My call.) Keeping this out of the project means
`loom plan` needs no `.gitignore` entry and no `loom init` before it runs; a project's `.loom/`
directory belongs to `loom run`. Section 12 must allow planning sessions to write their hook
events here.

The summary store is project content, not runtime state; it lives in `doc/loom/summaries/`
(vision, Decisions; section 04).

## Opening the browser

After the server binds, `loom plan` prints the URL and opens it with the platform's opener
(`open` on macOS, `xdg-open` on Linux). It prints the URL and skips the opener when any of
these holds:

- `--no-open` was given;
- `SSH_CONNECTION` or `SSH_TTY` is set (the browser would open on the remote machine, or
  nowhere);
- on Linux, neither `DISPLAY` nor `WAYLAND_DISPLAY` is set.

An opener that fails or is missing is not an error: the URL is already printed. Over SSH the
output adds a tunnel hint with the bound port:

```text
http://127.0.0.1:7272/?token=…
On your machine: ssh -L 7272:127.0.0.1:7272 <this host>
```

(My call: the tunnel keeps the server on loopback, where the access model is strongest, rather
than pushing users toward `--host`.)

## Access

The access model is the dashboard's (`access.rs`, `auth.rs`, `bootstrap.rs`): `Host` and
`Origin` checks on every request, a per-process token, and a port-scoped cookie obtained only
through `GET /?token=<token>`.

The difference is which routes need the cookie. The dashboard's only write lanes are config
writes and, with `--terminals`, stage terminals. In `loom plan`, starting a session, chatting,
and saving a file are write lanes into processes that edit the repository, so they need the
cookie in every posture, loopback included, as the dashboard's terminal lane does. Every start
therefore mints a token and the printed and opened URL carries it. Viewing routes follow the
dashboard's rules. Section 12 owns the details and may tighten viewing.

## Lifetime

`loom plan` runs in the foreground until stopped. Closing every browser tab does not stop it;
it ends the authoring session after the grace period (vision, "Sessions start cold"; section
07) and keeps serving, so the user can reopen the URL.

Stopping follows the vision:

1. First Ctrl-C (or SIGTERM): the server stops accepting new sessions and chat messages, asks
   every running session to write its final state into the documents, prints which sessions it
   is waiting on, and exits when they finish.
2. Second Ctrl-C: exits at once, leaving whatever the sessions had written.

While running, the terminal prints one line per lifecycle event (a session started or ended, a
summary rebuild started or finished with its count) and nothing per message, so the terminal
stays readable. (My call.)

Exit status is 0 after a normal stop and non-zero when the process could not start: not a git
repository, a stage worktree, a server already running for the project, a bind failure.

## Shared code with `loom status --web`

`loom plan` runs its own server process, built from the dashboard's parts. These move out of
`commands/status/web/` into a shared module that both commands use:

- `listener.rs`: binding and port search, parameterised by the default port;
- `access.rs`, `auth.rs`, `bootstrap.rs`: access policy, token, cookie bootstrap;
- `connection.rs`: the request gate, security headers and CSP, connection and WebSocket caps;
- `ws.rs`: WebSocket framing;
- static asset serving from the embedded build.

Each command keeps its own routes, wire model, and broadcaster. The dashboard's daemon-fed
broadcaster has no counterpart; the planning server publishes from its own file watcher and
session bridge (sections 04, 07).

The browser app lives in the same `web/` project as the dashboard and shares its components,
CSS tokens, and theme (vision, principle 6). It is a second entry page in the same Vite build,
embedded through the same `build.rs` step, so the dashboard's bundle does not carry the
planning app's code and both stay one product. (My call.)

## Without `.loom/work`

Steps 1 to 6 need nothing from `.loom/work`; `loom plan` works in a project that has never run
`loom init`, unlike `loom status --web`, which exits before binding without it.

What it reads of a run comes from the plan files: section 01 derives running and done plans
from the `IN_PROGRESS-` and `DONE-` prefixes. When `.loom/work` exists, the app also shows that
a run is set up and offers to open the dashboard; starting `loom init` and `loom run` from the
app is section 11.

## Missing prerequisites

Planning sessions need `tmux` and the `claude` CLI. When either is missing, `loom plan` still
starts, in read-only mode: file mode and concept mode work on the summaries already committed,
and the app and the terminal both say what is missing and how to install it. (My call: a
reviewer reading the plans on a machine without these should not be turned away.)

## Out of scope for this section

- Session start, ownership, and the hook event spool's format: section 07.
- The file watcher and summary rebuilds: section 04.
- Authentication details, session sandboxing, and writable paths: section 12.
- Handing off to `loom init`, `loom run`, and the dashboard: section 11.
