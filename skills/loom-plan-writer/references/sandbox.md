# Sandbox Detail

Read when: configuring a plan's `sandbox` block, or when an acceptance command writes files, needs a host resource, the network, or `HOME`.

**Walk the writes.** For every acceptance command in every stage, list the paths it writes and confirm each is inside `allow_write`: build outputs (`dist/**`, `.vite/**`, `target/**`), caches, and the lockfile by its REAL name — read the repo, don't assume (`bun.lock` vs `bun.lockb` bit three logged plans). A blocked write can exit 0 (`SKILL.md` Section 9) — the stage "passes" while nothing landed. **And a path you cannot get INTO `allow_write` disqualifies the command — see below.**

**Package-manager caches are pre-granted.** Loom emits the per-user cache directories of bun, npm, pnpm, yarn, deno, cargo, rustup, uv, pip and go (`sandbox/package_caches.rs`) into every stage's OS-level `allowWrite`, so a dependency install in a worktree does not need a plan `allow_write` line. Two gaps stay the plan's job: a cache relocated by an env var (`XDG_CACHE_HOME`, `CARGO_HOME`, `BUN_INSTALL_CACHE_DIR`, ...) must be listed in `allow_write` explicitly, and a cache directory that does not exist on the host at session start is skipped by the sandbox — a manager used for the very first time on that machine fails with `EROFS` until the directory exists.

## Acceptance runs INSIDE the stage's sandbox — verify it THERE

`loom stage complete` runs the acceptance list itself, from the agent's own process inside the
worktree session. Every criterion therefore inherits that session's sandbox and that worktree's
filesystem layout — NOT your main checkout, and not the host shell you tried it in. The daemon's
host-side verification does not inherit them, which is why the same list can look green from an
operator shell and be impossible from inside. **A command you confirmed by hand at the repo root
has been confirmed in the wrong environment.**

**The ungrantable-resource rule: if a command needs something the stage's sandbox cannot be
configured to grant, it is NOT an acceptance criterion — however well it would prove the
feature.** Prove the behavior another way (a test that INJECTS the root/handle instead of
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
- **Host daemons and OS resources** — tmux and `AF_UNIX` sockets, Docker, an X11 display, a
  listening port.
- **Network beyond `allowed_domains`** — including the registry fetch a "cheap" build step makes
  on a cold worktree.
- **The user's real HOME** — credentials, `~/.claude`, a global toolchain config.

Loom's OWN CLI earns its own line: **never put a `loom` subcommand that opens shared state into a
worktree stage's acceptance.** `loom map`, anything touching `.work/` or `.loom/`, and the
memory/knowledge journal all write state shared with every sibling stage. The read-only
`loom map --outline` / `--find-all` / `--impact` views are source-graph queries, but keep them
out of worktree acceptance because the derived graph is shared state.
