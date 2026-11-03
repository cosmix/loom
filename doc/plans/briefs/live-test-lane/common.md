# live-test-lane: shared context for every worker

Read this file in full before your own brief. The plan is `doc/plans/PLAN-live-test-lane.md`
(`IN_PROGRESS-PLAN-live-test-lane.md` once it runs). Where this file and a stage description
disagree, the stage description's PUBLIC SURFACE wins; report the disagreement in your final
message.

## What the live profile is

A small set of loom's own tests drive a real Claude Code: they need outbound network and a
credential, which neither the Claude Code session sandbox nor loom's confined runner provides.
The live profile is loom's confined runner (the daemon's host job behind `loom check` and
`loom stage complete`) with three additions: a private writable `HOME`, the per-run gate as the
only network path, and a profile marker the test guards read. Live entries (`live:` on a stage)
run only through `loom check --live <stage>`; `loom stage complete` never runs them and only
reads the receipt the last live run wrote.

- Linux: the runner's empty network namespace plus one read-only bind of the directory holding
  the gate's Unix socket. Inside, `loom live exec` listens on `127.0.0.1:3128` and pipes each
  connection to that socket, then runs the entry command, so clients use `HTTPS_PROXY`.
- macOS: the runner's Seatbelt profile plus `(allow network-outbound (remote ip
  "localhost:<port>"))` for the gate's loopback port; no forwarder.
- The gate runs on the host in the job's process. It answers `CONNECT` for allowlisted hosts
  only, requires the per-run proxy secret (`Proxy-Authorization: Basic`), terminates TLS with a
  per-run CA, strips every client auth header, injects the real credential, re-originates TLS
  upstream with real verification, and enforces a per-run budget. The credential never enters
  the sandbox.

## Rules that bind every worker

- Files at most 400 lines, functions at most 50 lines (`cargo test --test maintainability`).
  `loom/maintainability-baseline.txt` is a ratchet file: never edit it unless your brief says so.
  Several seams sit at or near 400 lines (`host_verify/child.rs` is exactly 400,
  `cli/types.rs` 392, `verify/criteria/confine_host.rs` 382, `host_verify/runner.rs` 387,
  `orchestrator/signals/merge.rs` 396):
  check `wc -l` before and after.
- Never `cargo install`; never add a dependency by hand. The crates this plan uses were added by
  the first stage with `cargo add`.
- `rustls`, `tokio-rustls` and `rcgen`: build every config with an explicit provider
  (`rustls::crypto::aws_lc_rs::default_provider()`, `ServerConfig::builder_with_provider`,
  `ClientConfig::builder_with_provider`). Both `ring` and `aws-lc-rs` are in the lock file, so
  `ServerConfig::builder()` and `ClientConfig::builder()` panic at run time. Never call them.
- `src/lib.rs` carries `#![deny(unsafe_op_in_unsafe_fn)]` and
  `#![deny(clippy::undocumented_unsafe_blocks)]`: every `unsafe` block gets a `// SAFETY:` line.
- Errors: `anyhow::Result` with `.context(..)` at each boundary. A refusal the daemon returns to
  an agent is `Response::Error { message }`, never an `Err`.
- Doc comments state current behaviour: no "was", no "used to", no dated history.
- Line numbers in briefs drift. Locate every seam by symbol with `loom map --outline <file>` and
  `loom map --find-all <name>` before editing.
- Socket, loopback and nested-bubblewrap tests: a stage session's own sandbox denies
  `socket(AF_UNIX)` and user namespaces, so those tests fail inside your session and run under
  `loom check` (the host job). Your one allowed check is a compile check
  (`cargo test --no-run ...`) or a module filter that needs none of them.
- MEMORY: record mistakes, decisions and surprises with `loom memory` immediately; never
  `loom knowledge`; never Claude Code auto-memory.

## Layout

| What | Path |
| --- | --- |
| Credential | `<state root>/live-credential` (`fs::permissions::state_root::LIVE_CREDENTIAL_FILE`, one of `STATE_ROOT_SECRET_FILES`), JSON `{"kind":"api-key"\|"oauth-token","value":"...","set_at":"<RFC 3339>"}`, mode 0600, owner the euid. Every capsule denies it; the confined runner never sees the state root of a stage worktree and hides the file when a knowledge stage's tree holds it |
| Run root | `<std::env::temp_dir()>/loom-live-<euid>/<12 hex>/` with `home/`, `tmp/`, `gate/` (each 0700); the parent is refused when it is a symlink or not owned by the euid; removed when the entry ends |
| Gate socket (Linux) | `<run root>/gate/gate.sock` |
| Stage receipt | `<state root>/verification/<stage>/live-receipt.json` (written only by the host-job child or the operator's own `loom check --live`) |
| Entry logs | the live job's directory, one log per entry (`Progress::record`) |
| Smoke receipt | `<state root>/verification/live-smoke.json` |

`verification/` is in the session read grants (`sandbox/settings.rs` `STATE_READ_DIRS`), so a
session reads receipts and logs and writes neither.

## Test guards (defined by the test-escape-policy stage)

All in `loom::process::sandbox_probe` (the guard API lives in its child module `facilities`,
re-exported):

- `LOOM_TEST_FACILITIES` (`FACILITIES_ENV`): comma list of facilities the environment provides.
  Loom's runner sets it on every confined command; CI sets it per job. Names: `unix-sockets`,
  `loopback`, `process-tree`, `bubblewrap`, `tmux`, `network-gate`, `writable-home`,
  `claude-code`, `codex` (no environment provides `codex` yet: plan 11's tests need it and
  report unavailable). A probe for an advertised facility that fails is a test failure
  (`require_facility`); an unadvertised one skips. The variable only adds strictness, so
  nothing forges it to widen anything.
- The live-profile marker: `LOOM_LIVE_PROFILE=<run id>` (`PROFILE_ENV`) plus a regular file
  `$HOME/.loom-live-profile` (`PROFILE_MARKER`) holding the run id, with `HOME` different from
  the account's passwd home. All three must hold (`check_profile`). A test that needs the
  profile is `#[ignore = "loom-live-profile: <what it needs>"]` and starts with
  `if !require_live_profile("<name>", &[...]) { return; }`.
- Outside the profile the guard writes `loom-live-profile: skip <test>: <why>` straight to the
  process's stderr handle (`std::io::stderr().write_all`, which libtest's capture does not
  intercept); inside, a needed facility the profile does not provide gives
  `loom-live-profile: unavailable <test>: needs <facility>, ...`. Inside the profile every
  other skip (`skip_unless`, `require_facility`) writes the same `skip` line too, so an entry
  whose tests skipped fails. Both lines start with `PROFILE_LINE_PREFIX`
  (`loom-live-profile:`).

Advertised facilities: the standard runner on Linux gives `unix-sockets`, `loopback`,
`process-tree`, `bubblewrap` when the host can start bubblewrap (`bubblewrap_blocker`), and
`tmux` when `tmux` resolves on the command's `PATH`; on macOS it gives none (the Seatbelt
profile denies sockets and loopback). The live profile adds `network-gate`, `writable-home`, and
`claude-code` when a native Claude Code was bound in.

## The profile's environment and HOME

The command's environment is the confined allowlist (`process::apply_confined_host_environment`)
plus the runner's own entries (its scratch already sets `CARGO_HOME` explicitly), then, in
this order: `HOME=<run root>/home`; `RUSTUP_HOME` pinned to the original's value, or
`<original HOME>/.rustup` when unset (so rustup never looks under the private home); `PATH`
with `<home>/.local/bin`
first; `HTTPS_PROXY`, `https_proxy`, `HTTP_PROXY`, `http_proxy` =
`http://loom-live:<secret>@127.0.0.1:<port>`; `NO_PROXY` and `no_proxy` =
`localhost,127.0.0.1,::1` (tests talk to in-process servers on loopback);
`NODE_EXTRA_CA_CERTS=<home>/.loom-live/ca.pem`; `SSL_CERT_FILE=<home>/.loom-live/ca.pem`;
`CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC=1`; `DISABLE_AUTOUPDATER=1`;
`LOOM_LIVE_PROFILE=<run id>`; `TMPDIR` and `TMUX_TMPDIR` = `<run root>/tmp` on macOS (Linux keeps
its private `/tmp`); `LOOM_TEST_FACILITIES` = the runner's list plus the profile's.

`seed_home` writes, all mode 0600 unless noted, never the real credential:

- `.loom-live-profile`: the run id.
- `.loom-live/ca.pem`: the gate's per-run CA certificate.
- `.claude.json`: `{"hasCompletedOnboarding": true, "lastOnboardingVersion": "<claude
  version>"}` (the version key only when Claude Code was found). Loom's planning launch adds the
  project's trust entry to this same file (`fs::permissions::trust::trust_project`).
- `.claude/settings.json`: `{"env": {...}}` with the proxy, `NO_PROXY`, CA,
  `CLAUDE_CODE_DISABLE_NONESSENTIAL_TRAFFIC` and `DISABLE_AUTOUPDATER` entries above, plus
  `CLAUDE_CODE_OAUTH_TOKEN=sk-ant-oat01-loom-live-placeholder` for an `oauth-token` credential;
  for an `api-key` credential `"apiKeyHelper": "<home>/.loom-live/api-key-helper"` instead.
  The settings file carries these because loom's planning launch rebuilds the session
  environment (`env -i` plus `process::agent_session_environment_from`, which forwards no proxy
  or CA names) and passes `--setting-sources user,project`, so user settings under the profile
  `HOME` are what reaches a Claude Code that loom starts. Claude Code documents that every
  network variable can be set in `settings.json` `env`.
- `.loom-live/api-key-helper` (mode 0700, `api-key` only): `#!/bin/sh` printing
  `sk-ant-api03-loom-live-placeholder`.
- `.local/bin/claude` (when found): a symlink to the canonical native Claude Code binary, which
  the boundary binds read-only.

## The gate

- `CONNECT <host>:443` only; any other method or an absolute-form request gets 405. A missing
  or wrong `Proxy-Authorization` gets 407 with `Proxy-Authenticate: Basic realm="loom-live"`;
  a host outside the list or a port other than 443 gets 403. Neither contacts the upstream.
- After `200 Connection Established` the gate is the TLS server for that host: leaf signed by
  the per-run CA (ECDSA P-256, 24 hours, name-constrained to the hosts), SNI must equal the
  CONNECT host, ALPN `http/1.1` only.
- Each request: `Host` must equal the CONNECT host (ASCII case-insensitive, `:443` suffix
  ignored), else 421; remove `authorization`, `x-api-key`, `proxy-authorization` and the
  hop-by-hop headers; insert `x-api-key: <value>` (api-key) or `authorization: Bearer <value>`
  (oauth-token); pass everything else (`anthropic-beta`, `anthropic-version`) through.
- Upstream: a fresh TLS connection with real verification (rustls-platform-verifier for
  `Upstream::public`, the given roots for `Upstream::fixed`), ALPN `http/1.1`; a verification or
  connect failure answers 502 with a body naming the class of failure.
- Budget per run: 1000 requests (then 429, no upstream contact), 1 GiB of response bytes (the
  body is cut and `budget_exhausted` set), 32 MiB per request body, 32 concurrent connections.
  Bodies stream; nothing is buffered (Claude Code streams server-sent events).
- Log: one line per request, `<seq> <METHOD> <host><path> <status> <bytes>` with the path cut at
  `?`, bytes outside `0x21..=0x7e` escaped as `%XX`, at most 256 bytes; one line per refused
  connection; at most 2,000 lines; never a header, a body or the credential.

## Entry verdict (first match wins)

1. Timed out: `failed: timed out after <n> s`.
2. The credential value appears in stdout, stderr or the gate log: `failed: the credential value
   appeared in the entry's output` (the outputs are scrubbed to `[loom-live-credential]` either
   way).
3. The gate's budget ran out: `failed: the gate's budget ran out`.
4. Exit code not 0: `failed: exited <code>`.
5. A `loom-live-profile: skip` line: `failed: <n> tests skipped for want of the live profile
   inside it`.
6. A `loom-live-profile: unavailable` line: `unavailable (facility): <first such line>`.
7. A v2 stage and the zero-test guard reads zero executed tests: `failed: selected zero tests
   (<adapter>)`.
8. Otherwise `passed`.

A profile that cannot start (no bubblewrap, a refused nested Seatbelt) is `unavailable
(facility)` with the boundary's error. No configured credential is `unavailable (credential)`
for every entry, and nothing runs. Nothing ever falls back to the host.

## Receipts and completion

A live run writes one receipt for the stage: the binding (the content manifest digest of the
checkout, `verify::criteria::content_digest`, or the HEAD tree id when the manifest cannot be
taken and the tree is clean) and one outcome per entry, each with that entry's definition hash
(`plan::schema::live_entry_hash`). A tree that changed during the run leaves the binding empty,
so the receipt never matches.

`loom stage complete` runs a `live` check for every v2 stage with live entries. It runs
nothing: for each `required: true` entry it needs a receipt whose binding matches the stage's
current content and that holds an outcome with the entry's current definition hash, `passed`.
A missing or stale receipt fails with `run loom check --live <stage>`; a failed entry names
`[live <n>]` and its
reason; an unavailable entry names its cause. Entries with `required: false` are advisory: their
outcomes are printed and never fail the check. An agent may `loom stage block` a stage whose
only failing check is `live` when every failing required entry is `unavailable (credential)`; a
failure of any other kind is fixed, rerun or disputed with
`loom stage dispute-criteria <stage> --field live --criterion-index <n>`.

A merge resolution runs the same `live` check: `verify_resolved_completion`, behind
`--resolved` and a resolver's exit, checks the stage worktree the resolver merged into, and the
merge changed its content, so the earlier receipt no longer counts. A resolver of a stage with a
required entry runs `loom check --live <stage>` after committing the merge and before
`--resolved` (its signal's `## Live Entries` section, live-checks R2); the relay admits a Merge
session's `check` only with `live` and only for the stage it resolves (live-checks R1). A
resolver cannot dispute, so a live outcome it cannot fix ends in its give-up (`loom stage
block`, which routes the stage to human review).

## Placeholders

`sk-ant-api03-loom-live-placeholder` (api-key) and `sk-ant-oat01-loom-live-placeholder`
(oauth-token). The real value never enters any file under the run root.
