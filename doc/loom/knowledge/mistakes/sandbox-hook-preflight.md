# Sandbox Hook Preflight

> Check 5 hook preflight: why extending its program denylist fails

## Extending the Check 5 Program Denylist Was Proposed as a Fix

**What happened:** `loom run` refused a user-level `bunx -y ccstatusline@latest --hook` hook. The first response split package runners out of `SELF_DIRECTED` with a better message, scanned `statusLine`, then built a trust-once flow, and recommended `bun add -g` plus an absolute binary path as the safe alternative. The owner rejected it as insufficient and it was reverted.

**Why:** check 5 is a list of named programs to refuse, and the fix only extended that list. Hooks also run session-written code through subcommand runners (`pnpm dlx`, `bun x`, `dnx`, `jbang`, `nix run`), version-manager shims reading cwd files (e.g. `.tool-versions` `path:` versions, `rust-toolchain.toml` `path`, `bunfig.toml` preload), runtimes loading writable caches (deno, go build cache, bun auto-install), Linux hard links from bun/uv/pnpm caches into global installs, shebangs, and launchers check 5 never reads (MCP stdio servers, plugin hooks, managed settings, `apiKeyHelper`).

**Prevention:** treat an unsandboxed hook as running every file and cwd config its runtime loads; before extending check 5's tables, enumerate those load paths across ecosystems and both platforms. Never recommend a global install as safe without checking whether the manager hard-links it from a session-writable cache.

**Fix:** reverted; the structural option (run non-loom hooks inside the session sandbox) is open with the owner.
