# Sandbox Detail

Read when: configuring a plan's `sandbox` block, or when an acceptance command writes files, needs a host resource, the network, or `HOME`.

**Walk the writes.** For every acceptance command in every stage, list the paths it writes and confirm each lands inside the worktree or in the command's private scratch: a criterion runs in the daemon's host job, where only those two are writable, and `allow_write` never applies to it. Build outputs (`dist/`, `.vite/`, `target/`), in-tree caches and the lockfile are inside the worktree, so they are covered. `allow_write` is for the session's own writes OUTSIDE the worktree. Loom drops every non-glob `allow_write` entry that lies inside the working directory, because each one would become a separate bind mount, and git cannot replace a file on a mount point (`unable to unlink ... Device or resource busy`); `loom plan verify` warns on a relative one. A blocked write can exit 0 (`SKILL.md` Section 9) — the stage "passes" while nothing landed. **And a criterion that must write outside the tree and the scratch is disqualified — see below.**

**Package-manager caches are pre-granted.** Loom emits the per-user cache directories of bun, npm, pnpm, yarn, deno, cargo, rustup, uv, pip and go (`sandbox/package_caches.rs`) into every stage's OS-level `allowWrite`, so a dependency install in a worktree does not need a plan `allow_write` line. Two gaps stay the plan's job: a cache relocated by an env var (`XDG_CACHE_HOME`, `CARGO_HOME`, `BUN_INSTALL_CACHE_DIR`, ...) must be listed in `allow_write` explicitly, and a cache directory that does not exist on the host at session start is skipped by the sandbox — a manager used for the very first time on that machine fails with `EROFS` until the directory exists.

## Criteria run in the daemon's host job

`loom stage complete` and `loom check` in a session ask the daemon to run the criteria in a host
job: the stage worktree's filesystem layout under loom's confinement (no network, private `/tmp`,
read-only git metadata), never the session's sandbox, your main checkout or a bare shell. Verify a
criterion with `loom check <stage-id>` from a session or the host, not by running it bare. **A
command you confirmed by hand at the repo root has been confirmed in the wrong environment.**

**The ungrantable-resource rule: if a command needs something the host job's confinement does
not grant a criterion (no `allow_write` or `allowed_domains` entry reaches it), it is NOT an
acceptance criterion — however well it would prove the feature.** Prove the behavior another way (a test that INJECTS the root/handle instead of
resolving it, a read-only/`--dry-run` flag, an `artifacts` check on something the code already
wrote) and state in the prose what was traded away. Proving that a write can never be granted is
the moment to DROP the command — not to write the finding down as a known limitation and keep it.
Four ungrantable classes, each logged:

- **Writes that escape the worktree.** Anything resolved through `main_project_root` or through
  the `.loom/work` symlink (or the legacy `.work` symlink) — in this repo `ContextStore::open` (so
  `loom map --outline`, `loom knowledge context`, and every command that opens the context store),
  `.loom/cache/**`, `.loom/work/context/**`, `.git/info/exclude`. Both settings emitters filter out
  every `../` entry
  (`sandbox/settings/policy.rs`, `sandbox/settings.rs`), so **no `allow_write` line can express
  those paths at all.** See `doc/loom/knowledge/mistakes/parallel-worktree-shared-state.md`.
- **Host daemons and OS resources** — Docker, the host's tmux, an X11 display, a listening port.
  Their sockets stay hidden even from a stage that may create its own (below).

**`AF_UNIX` sockets are grantable per stage, with residual risk.** By default creating any Unix
socket fails with `Operation not permitted`, at any path. `allow_unix_sockets` (a path list) does
nothing on Linux: the sandbox cannot filter a socket by its path. `allow_all_unix_sockets: true` on
one stage lifts the filter; loom then hides the host's sockets (`/run`, `/var/run`, the ssh and gpg
agents, tmux, X11, the loom daemon, and those in `/tmp` at spawn), but a socket created under
`/tmp` after the session starts, and every socket under `$HOME`, stays reachable. `loom run`
refuses such a plan until the operator passes `--accept-unix-socket-risk`, on every run. Prefer a
tool's TCP-only option (postgres `-c unix_socket_directories=''`); grant the stage only when none
exists.

- **Network beyond `allowed_domains`** — including the registry fetch a "cheap" build step makes
  on a cold worktree.
- **The user's real HOME** — credentials, `~/.claude`, a global toolchain config.

Loom's OWN CLI earns its own line: **never put a `loom` subcommand that opens shared state into a
worktree stage's acceptance.** `loom map`, anything touching `.work/` or `.loom/`, and the
memory/knowledge journal all write state shared with every sibling stage. The read-only
`loom map --outline` / `--find-all` / `--impact` views are source-graph queries, but keep them
out of worktree acceptance because the derived graph is shared state.

## `provision`: dependencies installed on the host before each session

`loom.provision` (`version: 2` plans only) is a list of `{ working_dir, command }` entries. It gives a
fresh worktree its dependencies (`node_modules/`, `.venv/`) before any stage agent runs, so no stage
spends a session task on an install the sandbox may not permit.

```yaml
loom:
  provision:
    - working_dir: "web"
      command: "test ! -e .npmrc && test ! -L .npmrc && bun install --frozen-lockfile --ignore-scripts --backend=copyfile --config=/dev/null"
```

- **When.** Each time a stage session spawns in a worktree: first spawn, retry, handoff successor and
  requeue after a verdict, for standard, integration-verify and knowledge-distill stages. The daemon
  runs the stage's `before_stage` checks on the pristine worktree first, then each provision command
  in order, in `<worktree>/<working_dir>`, 600 s each. A `before_stage` check therefore cannot need
  provisioned dependencies. The contract phase's own spawns on a worktree (the
  contract-to-implementation handoff and a replacement contract session) reuse the install.
- **Where.** On the host, outside the sandbox, with the stage-command environment: `HOME` and `PATH`
  kept, credentials dropped. The registry needs no `allowed_domains` entry for provision itself.
- **Idempotent.** Every retry, handoff successor and verdict requeue runs the commands again, so use
  the frozen forms: `bun install --frozen-lockfile`, `npm ci`,
  `uv sync --frozen --no-install-project`, `pnpm install --frozen-lockfile`, with
  `--ignore-scripts` on the JS installs (next item). `uv sync` without `--no-install-project`
  builds the local project through its build backend, which runs repository code on the host.
- **No repository-controlled code.** Provision runs on the host, outside the sandbox, in a worktree
  whose files a stage can edit, so a command must not run anything the repository or an agent
  controls. A JS install interprets five such channels: `package.json` lifecycle scripts, the bun
  cache, `bunfig.toml`, pnpm's `.pnpmfile.cjs` and `.npmrc`. Close them with `--ignore-scripts` (no
  `postinstall`), for bun `--backend=copyfile` (no hardlinks into the real bun cache) and
  `--config=/dev/null` (an agent-written `bunfig.toml` is ignored), and for pnpm
  `--ignore-pnpmfile`. An agent-written `.npmrc` still redirects the registry even then, so the
  command opens with `test ! -e .npmrc && test ! -L .npmrc &&` and joins the rest with `&&` only.
  The command may not `cd` (nor `pushd` or `popd`), since the refusal checks only the directory the
  install runs in: set `working_dir` to the package directory instead.
  The hardened form is the example above; the other managers' forms put
  `npm ci --ignore-scripts`, `pnpm install --frozen-lockfile --ignore-scripts --ignore-pnpmfile`
  or `yarn install --frozen-lockfile --ignore-scripts` behind the same refusal.
- **Enforced.** `loom plan verify` and `loom init` reject an entry whose `bun install`,
  `npm ci`/`install`, `pnpm install`, `yarn` or `uv sync` lacks a flag above, the `.npmrc` refusal,
  or (for `uv sync`) `--no-install-project`, and name the hardened form to use. The JS provision
  lint suggests the hardened form for the package's lockfile; a package with no lockfile must
  commit one first, since an unlocked install writes an untracked lockfile that blocks the stage.
- **Git-ignored writes only.** The daemon lists `git status` once before the first entry and once
  after the last, and blocks the stage on any entry that is new: it would read as a non-contract
  edit at the contract freeze and as prior stage work on a retry.
- **Failure.** A failing command blocks the stage; `loom status` shows ``provision `<cmd>` in `<dir>`
  failed: <stderr tail>``.
- **The snapshot.** `loom init` copies the entries into the work directory's `config.toml`
  (`[plan_provision]`). The daemon runs only that copy and never the plan file, because a stage can
  edit the plan file and provision runs on the host. To change the entries during a run, the operator
  edits `[plan_provision]` and runs `loom stage retry <id>`.
- **Versus `setup:`.** `setup:` only prefixes acceptance commands; it never runs as part of a
  session's own work and runs wherever the acceptance command runs, in the daemon's host job.
  `provision` runs once per spawn, on the host, before the agent starts.
- **Repository-level lints.** `loom plan verify` judges the repository, not the stage: a repository
  with a JS package (this one has `web/`) needs a `provision` entry for it in every v2 plan, and a
  JS package with a test runner and at least one `dependencies` or `devDependencies` entry that no
  provision entry covers is an error. A package that declares neither needs no install.

## The pre-commit hook needs the network too

Every stage commits, so the repository's pre-commit hook (from `core.hooksPath`, else `.git/hooks`)
runs inside every stage's sandbox. A hook that runs `bunx` fetches from the npm registry: every
sandboxed stage then needs `registry.npmjs.org` in `allowed_domains`, including stages whose own
acceptance needs no network. `loom plan verify` errors when a stage's sandbox omits a registry the
hook fetches from.

## Where criteria run

Acceptance, setup, wiring, truth, dead-code, contract and impact-test commands run in a host job
the daemon starts (bubblewrap on Linux, `sandbox-exec` on macOS), not in the session sandbox:

- No network, ever. The plan's `network.allowed_domains` reaches only the agent's session.
- Git metadata is read-only; `/tmp` and `/var/tmp` are private to the run.
- A string criterion gets 300 s. The extended form `{command: ..., timeout_secs: N}` defaults to
  30 s and takes `timeout_secs` from 1 to 3600: set it for a slow suite.
- A tool a criterion needs comes from a `provision` entry (provision has network); the criterion
  calls the installed binary by its path in the tree, such as `node_modules/.bin/<tool>` after a
  Bun or npm install or `.venv/bin/<tool>` after `uv sync`. Never `bunx`, `npx`, `uvx`,
  `pip install`, `cargo install` or `curl` at criterion time: `loom plan verify` errors on the
  registry commands for a `confined` stage whatever `allowed_domains` says.
- A lint or format check is a criterion only when the target project already runs it (its CI,
  hooks or scripts) with its own config in the repository. Never carry a check over from another
  project or from loom's own repository.

`loom check <stage-id>` runs the criteria in the same host job, so a mismatch shows before
completion (`--criterion <n>` re-runs only the named, 0-based criteria); its first line `Host job: <state>` is the result (`requested` and `pending` are not:
run it again).

**Granting the session's network.** Think hard about `network.allowed_domains`: list every fetch the
stage's own work does by reading the repo's manifests, lockfiles, scripts and hooks, and grant exactly
those. Typical hosts: cargo `crates.io`, `index.crates.io`, `static.crates.io`; bun/npm
`registry.npmjs.org`; uv/pip `pypi.org`, `files.pythonhosted.org`; Playwright browsers (its download
hosts); GitHub release downloads `github.com`, `objects.githubusercontent.com`; go `proxy.golang.org`.
Anything only a criterion needs goes to `provision`, not to the domain list.

## Registry domains `loom plan verify` checks

A command in a stage's after-stage checks, or in any scanned command of a stage whose
`command_confinement` is `inherit`, that fetches from a registry its sandbox does not allow is a
`loom plan verify` error. In a `confined` stage (the default) a registry fetch in an acceptance,
setup, wiring or dead-code command is an error whatever the domains say; the remedy is a
`provision` entry. `loom init` prints the same suggestions.

| Command | Domains the stage's `allowed_domains` needs |
| --- | --- |
| `bunx`, `npx`, `npm install`, `bun install`, `pnpm install`, `yarn install` | `registry.npmjs.org` |
| `cargo install`, `cargo fetch` | `crates.io`, `index.crates.io`, `static.crates.io` |
| `uv sync`, `uv add`, `uv pip install`, `uvx`, `pip install` | `pypi.org`, `files.pythonhosted.org` |
| `go get`, `go mod download` | `proxy.golang.org` |

## Configuration shape

```yaml
loom:
  sandbox:
    enabled: true
    auto_allow: true
    filesystem:
      deny_read: ["~/.ssh/**", "~/.aws/**", "~/.config/gcloud/**", "~/.gnupg/**"]
      deny_write: [".loom/work/stages/**"]
      allow_write: []              # paths OUTSIDE the worktree only; the worktree is writable
    network:                       # ⛔ MUST be a struct, NEVER the string "deny"
      allowed_domains: []          # empty = deny all; or list domains
      allow_local_binding: false
      allow_unix_sockets: []       # ignored on Linux: the sandbox cannot filter sockets by path
      allow_all_unix_sockets: false  # true only per stage, with the operator's `loom run --accept-unix-socket-risk`
```
