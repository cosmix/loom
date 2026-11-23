# Validation: permission, sandbox, relay, web authority, shell-hook and bounded-read findings

Source: `doc/AGY-CODE-REVIEW-2026-11-23.md`, validated against `62be2a3d2`.
Paths are relative to the repository root; Rust paths below `loom/src/` unless noted.

## CR-01 — Session-writable settings files feed `approved.json`

**Verdict:** CONFIRMED (worse than described: the same read path can also hang or exhaust the daemon)

**Evidence:**

- `fs/permissions/sync.rs:64-83` `settings_paths_to_check` returns `<worktree>/.claude/settings.local.json`, then `<working_dir>/.claude/settings.local.json` when `working_dir != worktree`, then `<worktree>/{loom,src,app,packages,workspace}/.claude/settings.local.json` when they `exists()` (which follows symlinks).
- `sync.rs:115-135` reads each one with `read_settings` (`sync.rs:162-172`: `path.exists()` and then `fs::read_to_string`, which follows symlinks, has no size cap and blocks on a FIFO). It takes `permissions.allow`, passes it through `portable_permissions` (`sync.rs:204-217`), which only rewrites single-slash, inert-`Write` and `../`/`.worktrees/` rules, and hands it to `record_approvals` (`sync.rs:92-106`). `record_approvals` adds the main checkout's `R/.claude/settings.local.json` and calls `approved::record_fold_back` (`fs/permissions/approved.rs:95-108`). That calls `record_approved` (`approved.rs:55-89`), which keeps every rule where `ControlSurfaces::names` is false.
- `sandbox/control_surfaces.rs:266-298` `names` only rejects control-surface paths. `Bash(*)` (an empty literal prefix on a non-path rule), bare `Bash`, `WebFetch(*)` and `Edit(//home/<u>/.bashrc)` all pass. The test `sandbox/control_surfaces/tests.rs:162-176` asserts that bare `Bash` is kept.
- Emission: `orchestrator/terminal/native/session_settings.rs:105` calls `approved_rules` (the same filter again on the way out), and `orchestrator/terminal/native/session_settings/contents.rs:135` adds the rules to `permissions.allow` in every later capsule. Sessions run with `--permission-mode auto` (`orchestrator/terminal/native/mod.rs:188`), where an allow rule skips the classifier.
- Fold-back callers, all of which run on the daemon or host side:
  - `orchestrator/core/inbox_drain/sweep.rs:143`, with `checkout` set to the record's checkout, which is the repo root for checkout-rooted sessions;
  - `orchestrator/core/crash_handler.rs:156`, which passes `working_dir`;
  - `orchestrator/core/merge_handler/resolver_exit.rs:98`;
  - `commands/stage/complete_pipeline.rs:155` and `commands/stage/complete_force.rs:173`, through `commands/stage/complete.rs:202-211`, which passes `acceptance_dir`.
- What a session can write (`sandbox/control_surfaces/session_denies.rs:101-115`). A worktree session gets an absolute deny on `<worktree>/.claude` plus a relative `Edit(.claude/**)` only.
  - `<worktree>/.claude/settings.local.json`: not writable.
  - `<worktree>/<wd>/.claude/settings.local.json` and the five subdirectory files: writable through Bash (the OS sandbox denies only `<worktree>/.claude`), and through the file tools when the cwd is the worktree root, because the relative rule is anchored to the cwd.
  - `R/.claude/settings.local.json`: not writable. It is outside the worktree grant and `worktree-file-guard.sh` confines the file tools.
  - A checkout-rooted session (Knowledge or Merge, the `None` branch) has `R/.claude` denied, but can write `R/{loom,src,app,packages,workspace}/.claude/settings.local.json`, and `sweep.rs:143` scans exactly those.
- No guard defeats the finding. Curated knowledge (`architecture/security-and-isolation.md`, "Where a Session's Write Grants Come From") confirms that Claude Code writes real "don't ask again" approvals to `<canonical git root>/.claude/settings.local.json`, which is `R`. The working-dir and subdirectory candidates are therefore not where user-consented approvals go.
- Second defect: a FIFO, or a symlink to `/dev/zero`, at a scanned subdirectory path makes `read_settings` block or allocate without bound inside the daemon's inbox sweep.
- Third, minor: `sync.rs:126` uses `?`, so one malformed planted file aborts the fold-back of the legitimate `R` approvals too.

**Fix seam:**

- `fs/permissions/sync.rs:64` `settings_paths_to_check`: drop the `working_dir` and subdirectory candidates, so only `<worktree>/.claude/settings.local.json` (session-denied) and `R`'s file are read. Then remove the `working_dir` parameter. Fold `sync_worktree_permissions_with_working_dir` (`sync.rs:115`) into `sync_worktree_permissions`, and update callers `commands/stage/complete.rs:8,202-211` (drop `acceptance_dir`), `orchestrator/core/crash_handler.rs:150-160` (drop `working_dir_path`), `commands/stage/complete_force.rs:173`, and the tests in `fs/permissions/sync/tests.rs:184,239,278`.
- `sync.rs:162` `read_settings`: replace with `crate::fs::safe_read::read_to_string_bounded(root, Path::new(".claude/settings.local.json"), 1 << 20)` (`fs/safe_read.rs:161`; no-follow at every component, `O_NONBLOCK`, regular-file check, size cap). Map `is_not_found` (`fs/safe_read.rs:24`) to `{}`. Use the same reader for `R`'s file in `record_approvals`.
- Make a per-file parse or read failure a `tracing::warn!` that contributes nothing, so it cannot abort the other sources.
- Do not widen `session_denies` to the subdirectories: Bash can create any directory name, so a deny list cannot cover it. Removing the source is the complete fix.
- Out of scope, as a policy decision for the owner: `ControlSurfaces::names` still admits blanket rules such as `Bash(*)` and `Edit(//…)` coming from `R`'s own file. These are user-consented, so leave them alone here.

**Regression test:**

- Extend `fs/permissions/tests/sync_tests.rs`, which has `create_state_root` (`:20`) and `approved_allow` (`:28`), or `fs/permissions/sync/tests.rs`.
- Test 1: write `{"permissions":{"allow":["Bash(*)","Edit(//home/u/.bashrc)"]}}` to `<worktree>/loom/.claude/settings.local.json` and run the fold-back. Assert that `approved_allow` contains neither rule. Today both are recorded.
- Test 2: `mkfifo` at `<worktree>/.claude/settings.local.json` and assert that the fold-back returns promptly. Today it blocks.

**Files a fix touches:** `loom/src/fs/permissions/sync.rs`, `loom/src/commands/stage/complete.rs`, `loom/src/commands/stage/complete_force.rs`, `loom/src/orchestrator/core/crash_handler.rs`, `loom/src/fs/permissions/sync/tests.rs`, `loom/src/fs/permissions/tests/sync_tests.rs`.

**Size / tier:** M; judgment (security).

**Overlap:** CR-33 (the same filter), CR-45 (unbounded reads), CR-17 (repo-root resolution, through `find_repo_root_from_cwd` in `complete.rs`).

## CR-03 — Shared scratch directory: ticket quota DoS and control-ticket piggyback

**Verdict:** NARROWED

**Evidence:**

- `relay/emit/stale.rs:38-87` `classify` counts every `.req` file it cannot decode as `live`, and nothing ever deletes such a file: `commands/hook/relay/sweep.rs:107-145` reports it and leaves it in place. `relay/emit/quota.rs:18-45` refuses a new ticket at `MAX_UNCONSUMED_TICKETS = 32` (`relay/mod.rs:39`). The flood is real, but it stays inside one session's scratch directory, and the lead can delete the files with `rm`. "Permanently" is overstated. The refusal message (`quota.rs:26-31`) does not name undecodable files, so the lead is not told how to recover.
- `relay/line.rs:60-74` `extract` applies `MAX_LINES_PER_CALL = 16` before `commands/hook/relay.rs:163-170` filters lines down to those with an existing ticket file. Sixteen forged lines that appear earlier in the output therefore crowd out a real line.
- Piggyback: `relay.rs:279-290` `relay_one` calls `admit` (`relay.rs:332-353`). `admit` refuses a control kind only when the payload `agent_type` is non-empty (`relay.rs:357-362`). A ticket a subagent writes at `<scratch>/<id>.req`, with its `LOOM_RELAY_V1` line, is relayed as `AgentRole::Main` if that line shows up in the output of a lead Bash call whose command text authorises the kind (`loom-hooks/loom-relay.sh:105-110`).
  - This needs the lead to run a compound command that prints attacker-controlled text, for example `cargo test && loom stage commit …` where a test prints a planted fixture. `loom-relay-guard.sh` does not forbid compounds.
  - `complete` and `subagents:control` are excluded because they are pinned.
  - A refused subagent control ticket is never deleted (`relay.rs:281-283` returns before any consume), so it stays available for a later lead call to replay.

**Fix seam:**

- Line cap: in `commands/hook/relay.rs:163-170`, filter by ticket existence first and cap afterwards. Make `RelayLine::extract` (`relay/line.rs:60`) uncapped, or give it a predicate, and apply `MAX_LINES_PER_CALL` to the filtered list.
- Refused control ticket: in `relay_one` (`relay.rs:279`), when `admit` refuses a control kind for a subagent, remove `ticket_check::ticket_file(scratch_dir, id)` (`relay.rs:298`), using the remove helper the sweep's `record` path already uses.
- Provenance, options:
  - (a) A PreToolUse marker. `loom-relay-guard.sh` writes `<scratch>/.pre-<tool_use_id>` for a lead call that names a control kind, and the PostToolUse relay accepts a control ticket only if its mtime is at or after that marker.
  - (b) Require exactly one control line per control-kind loom invocation in the command text.
  - Recommend (a). It binds control tickets to the lead's own call window with no CLI change, and the hook already holds `tool_use_id` (`relay.rs:214-217`).
- Quota: list undecodable `.req` ids in the `live` refusal (`quota.rs:26`), as the stale branch already does through `stale_ids`.

**Regression test:**

- Extend `commands/hook/relay/tests.rs` and `commands/hook/relay/tests_sweep.rs`.
- Test 1: 16 forged lines without ticket files followed by one real ticketed line. Assert that the real line is relayed. Today it is dropped.
- Test 2: a subagent-refused control ticket. Assert that its `.req` file is gone afterwards.
- Test 3: in `relay/emit/quota.rs`'s tests module, assert that the refusal names an undecodable id.

**Files a fix touches:** `loom/src/commands/hook/relay.rs`, `loom/src/relay/line.rs`, `loom/src/relay/emit/quota.rs`, `loom/src/relay/emit/stale.rs`, `loom-hooks/loom-relay-guard.sh` (if option (a)), `loom/src/commands/hook/relay/tests*.rs`.

**Size / tier:** M; judgment (security, concurrency).

**Overlap:** CR-66 (`loom-relay-guard.sh` fails open on bad input).

## CR-04 — Loopback `Host`/`Origin` parsing; dashboard `/ws` origin

**Verdict:** NARROWED (one real exposure: cross-port localhost read of dashboard `/ws`; the malformed-authority items cannot be produced by a browser)

**Evidence:**

- `web_server/http.rs:172-179` `origin_host`, and `host_allowed` (`http.rs:196-198`), accept each of the following:
  - `Host: [::1].evil.com:7373`: the `split_once(']')` at `http.rs:174` discards the text after `]`.
  - `Host: [::1` (unclosed): `map_or(rest, …)` at `http.rs:174` returns `::1`.
  - `Host: 127.0.0.1:80@evil.com`: `split_once(':')` at `http.rs:175-177`; `host_allowed` has no `@` check.
  - `Origin: http://[::1].evil.com` and `Origin: http://[::1`: `origin_allowed` at `http.rs:216-231` takes the authority by `split(['/','?','#'])` (`:226`), and the `@` check (`:227`) does not stop these.
  - The terminal check (`commands/status/web/terminal/upgrade.rs:153-165`) strips `/?#` but then requires `authority == host` byte for byte, so it is not a bypass.
- None of the malformed forms can come from a browser: URL parsing rejects `[::1].evil.com`, browsers strip userinfo from `Host`, and the serialised `Origin` never carries a path. A non-browser local client can already connect to a loopback bind directly. These items are parser hardening, not a DNS-rebinding bypass.
- The real gap is in `web_server/access.rs:84-103` `AccessPolicy::origin_allowed`. On a loopback bind it ignores port and scheme, and `commands/status/web/router.rs:55-63` passes `OriginRequirement::Optional` for `/ws`.
  - A page on `http://localhost:3000` (any local dev server) can open `ws://127.0.0.1:<dashboard>/ws`. WebSockets are not CORS-gated, so it can read the live status stream.
  - Plain HTTP GET routes are protected from cross-port reads by CORS, because there is no `Access-Control-Allow-Origin`.
  - The planner (`plan_server/gate.rs:89-95,105-110`) and the web terminal already require an exact `Origin == http://<Host>`.
  - The "missing Origin" half does not matter: browsers always send `Origin` on a WebSocket upgrade.

**Fix seam:**

- `web_server/access.rs:84` `origin_allowed`, loopback branch: when `Origin` is present, parse it with `remote_origin_authority` (`access.rs:141`) and require `authority_matches(candidate, local)` (`access.rs:156`), which enforces the exact port and accepts the `localhost` alias at a loopback endpoint.
- `access.rs:70` `host_allowed`, loopback branch: use `parse_authority` (`access.rs:115`) plus `authority_matches`, but accept a port-less `Host` as loopback-only. Existing tests send a bare `Host: localhost` (`commands/status/web/tests/socket.rs:127,286`).
- Delete `http::origin_host`, `http::host_allowed`, `http::origin_allowed` and `http::origin_allowed_strict` (`http.rs:172-231`) once nothing calls them. They are reached only through `access.rs`.
- `commands/status/web/router.rs:55-59`: use `OriginRequirement::Required` on `/ws` in both postures.
- The test helper `connect_websocket` (`commands/status/web/tests/socket.rs:176-191`) sends no `Origin`. Add `Origin: http://127.0.0.1:<port>`.

**Regression test:**

- Extend `web_server/access_tests.rs`, which has the `head` (`:12`) and `loopback_policy` (`:27`) helpers.
- Test 1: `loopback_policy().origin_allowed(local=127.0.0.1:7373, Origin "http://localhost:3000", Required)` must be false. It is true today.
- Test 2: `host_allowed` with `"[::1].evil.com:7373"` and with `"127.0.0.1:80@evil.com"` must be false. Both are true today.
- Test 3: add a `/ws` cross-port case to `commands/status/web/tests/socket.rs` next to `websocket_rejects_foreign_origin` (`:278`).

**Files a fix touches:** `loom/src/web_server/access.rs`, `loom/src/web_server/http.rs`, `loom/src/web_server/access_tests.rs`, `loom/src/commands/status/web/router.rs`, `loom/src/commands/status/web/tests/socket.rs`.

**Size / tier:** M; judgment (security).

**Overlap:** CR-59 (the same server module), CR-62 (CSP in `http.rs`).

## CR-05 — `credential-guard.sh` rule (c): uncanonicalised deny prefix

**Verdict:** NARROWED (a Write that creates a not-yet-existing denied file under a symlinked ancestor whose deny path is missing an intermediate directory; stage worktree sessions are also confined by `worktree-file-guard.sh`)

**Evidence:**

- `loom-hooks/credential-guard.sh:291` canonicalises `TARGET` with `canonical_target` (`:111-141`, which walks up to the nearest existing ancestor).
- `:574-576` canonicalises the deny prefix with `canonical_existing` (`:83-107`). That uses `realpath`, `readlink -f`, or a parent `cd`, and fails when the parent directory is missing; the code then falls back to the raw `SPLIT_PREFIX`.
- Example: `~/.aws/credentials` with `~/.aws` absent and `$HOME` behind a symlink, or a repository under `/var` on macOS.
  - The deny prefix stays `/home/u/.aws/credentials`, while `TARGET` becomes `/real/home/u/.aws/credentials`.
  - `pattern_matches` (`:513-533`) misses, and the script ends at `exit 0` (`:599`).
- Read, Edit and Glob/Grep need an existing target, and an existing deny path canonicalises correctly, so only Write that creates a new file is affected.
- In stage worktrees, `worktree-file-guard.sh` already refuses writes outside the worktree. The exposure is checkout-rooted Knowledge/Merge sessions and operator sessions.

**Fix seam:**

- `loom-hooks/credential-guard.sh:575-576`: replace `canonical_existing "$SPLIT_PREFIX"` with `canonical_target "$SPLIT_PREFIX"` (the same file, `:111`).
- Keep the raw-prefix fallback only for when `canonical_target` fails. A glob remainder never reaches `canonical_target`, because `split_pattern` (`:467-501`) has already cut it off.

**Regression test:**

- Extend `loom-hooks/tests/credential-guard-deny-read.sh`.
- Fixture: a temp directory `real/`, a symlink `link -> real`, and a capsule or settings `denyRead` entry `<link>/missing/secret.json`. Send a `Write` payload with `file_path=<link>/missing/secret.json`.
- Assert exit 2. Today the script exits 0.

**Files a fix touches:** `loom-hooks/credential-guard.sh`, `loom-hooks/tests/credential-guard-deny-read.sh`.

**Size / tier:** S; mechanical.

**Overlap:** CR-66 (the same hook's empty-stdin path).

## CR-16 — `loom usage` transcript scanners: unbounded lines, symlink and FIFO opens

**Verdict:** CONFIRMED (the FIFO hang is real on the codex path; following symlinks is a recorded design choice)

**Evidence:**

- `commands/usage/transcript.rs:49-58` `parse` uses `File::open`, and `:60-73` uses `BufReader::lines()` with no line cap.
- `commands/usage/codex_provider.rs:94-115` `parse_file` does the same.
- Discovery:
  - Claude side: `commands/usage/discovery.rs:209` uses `path.is_file()`, which follows symlinks and rejects a FIFO except through a TOCTOU swap. Sessions are also write-denied on `~/.claude/projects/**` (`sandbox/control_surfaces.rs:67`).
  - Codex side: `provider_data/codex_discovery.rs:66-79` collects every non-directory `.jsonl` entry, FIFOs included. A codex-licensed session can write under `~/.codex` (`codex.rs:128`), so a FIFO at `~/.codex/sessions/x.jsonl` hangs `loom usage` in `File::open`.
- Either side: a single-line multi-GB file grows memory without bound.
- `provider_data/codex_discovery_tests.rs:55` asserts that a file symlink is kept, so following symlinks is deliberate.
- Existing helpers: `tokens/tail.rs:12` defines `MAX_LINE_BYTES = 32 MiB`; `tokens/tail.rs:99` is `next_line(reader, &mut Vec<u8>, max) -> io::Result<Option<(u64, bool)>>`; `tokens/tail.rs:53-56` opens with `O_NOFOLLOW | O_NONBLOCK`; `commands/usage/parent_spawns.rs:109-122` shows the bounded pattern.

**Fix seam:**

- In `transcript.rs:54` and `codex_provider.rs:99`, open with `OpenOptions::new().read(true).custom_flags(libc::O_NONBLOCK)`, then `fstat` and skip anything that is not a regular file.
- Keep following symlinks. Discovery already pins that, and the data only feeds counts.
- Replace `.lines()` with a `next_line(&mut reader, &mut buf, tokens::tail::MAX_LINE_BYTES)` loop. Treat a cut line as a torn or malformed row: `diagnostics` in `transcript.rs`, `malformed_rows` in `codex_provider.rs`.
- Share one small `open_transcript(path) -> io::Result<File>` in `commands/usage/` for both call sites.

**Regression test:**

- Extend `commands/usage/codex_provider_tests.rs` (and the transcript tests beside `transcript.rs`).
- Test 1: `mkfifo` a `.jsonl` in a temp sessions tree and assert that the scan returns, counting the file as unreadable. Today it blocks.
- Test 2: a line longer than the cap followed by a valid row. Assert that the valid row is parsed.
- `commands/usage/spawn_join_tests.rs:6` has a FIFO fixture pattern to copy.

**Files a fix touches:** `loom/src/commands/usage/transcript.rs`, `loom/src/commands/usage/codex_provider.rs`, `loom/src/commands/usage/codex_provider_tests.rs`, transcript tests.

**Size / tier:** S-M; mechanical.

**Overlap:** CR-45, CR-51, CR-52 (the same bounded-read pattern), CR-58 (`usage` discovery).

## CR-18 — `loom pressure` codex log at a predictable temp path

**Verdict:** NARROWED (on Linux, `fs.protected_symlinks=1`, the default on mainstream distributions, refuses a symlink planted by another user in sticky `/tmp`; macOS `temp_dir()` is a per-user `/var/folders` directory; the whole-file reads are real)

**Evidence:**

- `commands/pressure/paths.rs:17-19` returns `temp_dir()/loom-pressure-codex-<pid>.log`.
- `pressure/codex_run.rs:375-376` opens it with `File::create` (`O_CREAT | O_TRUNC`, following symlinks).
- Whole-file reads: `pressure/codex_run.rs:191-195` `unavailable_line` uses `fs::read`, and `commands/pressure/spawn.rs:37-42` `print_log_tail` uses `fs::read` and then slices.
- The marker path already sits under `.loom/work/pressure/` (`paths.rs:29-35`).
- `spawn_codex_logged` is shared with `plan_server/summaries/codex_spawner.rs:244`.

**Fix seam:**

- `commands/pressure/paths.rs:17` `codex_log_path`: take `repo_root` and return `repo_root/.loom/work/pressure/codex-<pid>.log`, next to `claude_marker_path`. Update the caller `commands/pressure/mod.rs:209`.
- `pressure/codex_run.rs:375`: open with `OpenOptions::new().write(true).create(true).truncate(true).mode(0o600).custom_flags(libc::O_NOFOLLOW)`. Do not use `O_EXCL`: the plan-server caller may reuse its log path.
- Replace both whole reads with `crate::fs::safe_read::read_tail(root, relative, max_bytes)` (`fs/safe_read/tail.rs:14`), using the log's parent as `root`. `unavailable_line` scans a bounded tail, for example 256 KiB.

**Regression test:**

- Extend `pressure/codex_run/tests.rs`, which already builds `log_path` under a temp repo (`:90`, `:174`).
- Test 1: plant a symlink at the log path and assert that `spawn_codex_logged` fails without touching the target. Today it truncates the target.
- Test 2: in `commands/pressure/paths.rs`'s tests module, assert that `codex_log_path(repo)` lies under `repo/.loom/work/pressure`.

**Files a fix touches:** `loom/src/commands/pressure/paths.rs`, `loom/src/commands/pressure/mod.rs`, `loom/src/commands/pressure/spawn.rs`, `loom/src/pressure/codex_run.rs`, `loom/src/pressure/codex_run/tests.rs`.

**Size / tier:** S; mechanical.

**Overlap:** CR-45 (unbounded reads).

## CR-33 — `ControlSurfaces::pattern_names`: `..` bypass and missing `.codex`

**Verdict:** CONFIRMED (runtime denies still cover `HOME_SURFACES`, as the review says)

**Evidence:**

- `sandbox/control_surfaces.rs:278-298`:
  - `marked` checks only `CONTROL_COMPONENTS` (`:36`, with no `.codex`).
  - `literal_prefix` (`:178-188`) keeps `ParentDir`.
  - `ParentDir` is rejected only on the relative branch, and only for path rules (`:289-293`).
- Worked cases:
  - `Edit(~/foo/../.codex/hooks.json)` expands to `/home/op/foo/../.codex/hooks.json`. That does not `starts_with` the root `/home/op/.codex/hooks.json`, because component `foo` differs from `.codex`. The rule is kept.
  - `Bash(cp x ../.codex/config.toml)`: the word is relative and `path_rule=false`, so the function returns false and the rule is kept.
  - `Edit(.codex/config.toml)`, a project-local codex config, is kept because `.codex` is not a control component.
- The test `sandbox/control_surfaces/tests.rs:81-104` covers `~/.codex/hooks.json` and `../**`, but no `..` inside an absolute or `~/` path.

**Fix seam:**

- `sandbox/control_surfaces.rs:278` `pattern_names`: return true for any pattern containing a `Component::ParentDir`, for path rules and for Bash words alike. Do this before the absolute/relative split.
- Add `.codex` to `CONTROL_COMPONENTS` (`:36`, making it `[&str; 6]`).
- No normalisation is needed once `..` is rejected outright. Losing a convenience grant such as `Bash(cd ..)` from `approved.json` is acceptable, because the list is best-effort (`fs/permissions/approved.rs:27-31`).

**Regression test:** in `sandbox/control_surfaces/tests.rs`, add `"Edit(~/foo/../.codex/hooks.json)"`, `"Bash(cp x ../.codex/config.toml)"`, `"Edit(//home/op/x/../.claude/settings.json)"` and `"Edit(.codex/config.toml)"` to `names_every_control_surface` (`:81`), which uses the `surfaces()` helper (`:71`). All four are kept today.

**Files a fix touches:** `loom/src/sandbox/control_surfaces.rs`, `loom/src/sandbox/control_surfaces/tests.rs`.

**Size / tier:** S; judgment (security).

**Overlap:** CR-01.

## CR-45 — Lossy non-UTF-8 parent paths in `safe_fs/write.rs`; unbounded, symlink-following locked and session reads

**Verdict:** CONFIRMED (the read facet is hardening: the state root is write-denied to every capsule at `sandbox/control_surfaces/session_denies.rs:81-82`)

**Evidence:**

- `fs/safe_fs/write.rs:125-131` (parent of the leaf) and `:182-186` (`safe_remove_in_workdir`) rebuild the parent path with `String::from_utf8_lossy`. A non-UTF-8 component becomes U+FFFD, so `open_dir_safely` opens a different, usually missing, path.
- `fs/locking.rs:192-204` `locked_read` and `:229-243` `locked_update` use `File::open` with `read_to_string`: no `O_NOFOLLOW`, no `O_NONBLOCK`, no cap. `lock_parent_dir` (`:119-147`) also `File::open`s the directory and follows symlinks.
- These have 22 call sites outside `fs/locking.rs`. The largest users are `orchestrator/core/persistence.rs`, `fs/session_files/exact.rs`, `verify/transitions/persistence.rs`, `fs/permissions/approved.rs` and `fs/knowledge/dir.rs`. The knowledge files are session-writable repository files.
- `fs/session_files.rs:182` uses `fs::read_to_string` for every `sessions/*.md`.

**Fix seam:**

- `write.rs:125-131,182-186`: build the parent as a `PathBuf` from `OsStr::from_bytes` components (`std::os::unix::ffi::OsStrExt`) instead of `from_utf8_lossy`.
- `locking.rs:192,229`: open with `OpenOptions::new().read(true).custom_flags(libc::O_NOFOLLOW | libc::O_NONBLOCK)`, then call `crate::fs::safe_read::read_open_bounded(file, path, MAX_LOCKED_FILE_BYTES)` (`fs/safe_read.rs:80`, which does the regular-file check and the cap). Use a new module constant of 64 MiB. Keep `ENOENT` mapped to an empty string in `locked_update`.
- `session_files.rs:182`: use `read_to_string_bounded(&sessions_dir, Path::new(name), 1 << 20)` (`fs/safe_read.rs:161`).

**Regression test:**

- Extend the `fs/locking.rs` tests: a symlinked target makes `locked_read` fail. Today it reads through the link.
- Extend the `fs/safe_fs` tests: create a directory with a non-UTF-8 name, write beneath it through the anchored writer, and assert success. Today the call fails with `ENOENT`. Mark the test `#[cfg(not(target_os = "macos"))]`, because APFS rejects such names.

**Files a fix touches:** `loom/src/fs/safe_fs/write.rs`, `loom/src/fs/locking.rs`, `loom/src/fs/session_files.rs`, their tests.

**Size / tier:** M; needs-exploration (22 locked-I/O callers must tolerate the cap).

**Overlap:** CR-01, CR-16, CR-46, CR-51, CR-52.

## CR-54 — `StatusData.activity` is not sanitised

**Verdict:** CONFIRMED (dormant for `reason`; `stage_id` is rendered unsanitised)

**Evidence:**

- `status/data/collector_snapshot.rs:54-57` puts `stage_events::read_recent` into `StatusData.activity` without any sanitiser. It is the only construction site, while `:36` and `:45` sanitise summaries and the plan name.
- `fs/stage_events.rs:25-37`: `StageEvent.reason` holds the close reason a session writes with `loom stage block`.
- The web schema (`web/src/api/schema.ts:237-245,294`) ships `reason` to the browser. `web/src/lib/activity.ts:56-69` renders `event.stage_id`, not `reason`, so `reason` is dormant but `stage_id` reaches the view unneutralised. React escaping still prevents markup injection, so the exposure is bidi or visual spoofing.
- The TUI activity panel (`commands/status/ui/tui/ledger/panels.rs:52`) reads its own `TuiActivityLog`.

**Fix seam:**

- Add `pub(super) fn sanitize_stage_event(event: &mut StageEvent)` to `status/data/sanitize.rs`, reusing `flatten` (`:102`) on `stage_id` and on `session`, and `bound_close_reason` (`:135`) on `reason`.
- Drop events whose `stage_id` fails `valid_stage_id` (`sanitize.rs:150`).
- Apply it in `collector_snapshot.rs:54` with `.into_iter().filter_map(...)`.

**Regression test:** extend the `status/data/sanitize.rs` tests module, using the `summary()` helper (`:158`) as the model. An event whose `reason` contains `"\u{202E}x\x1b[2J"` must come out flattened. Today it is unchanged.

**Files a fix touches:** `loom/src/status/data/sanitize.rs`, `loom/src/status/data/collector_snapshot.rs`.

**Size / tier:** S; mechanical.

**Overlap:** CR-63 (bidi in the web views).

## CR-66 — Blocking shell hooks fail open on empty or malformed stdin

**Verdict:** CONFIRMED (seven hooks fail open on both inputs; `worktree-file-guard.sh` and `credential-guard.sh` fail open only on empty stdin)

**Evidence:**

Each hook was run from a scratch directory with `env -i` and `LOOM_SESSION_ID`, `LOOM_SCRATCH_DIR` and `LOOM_STAGE_ID` set, first on `</dev/null` and then on the truncated payload `{"tool_name":"Ba`.

| Hook | Read / parse lines | Empty | Unparseable |
| --- | --- | --- | --- |
| `codex-apply-patch.sh pre` | `:25-28`, `[[ $TOOL_NAME == apply_patch ]] \|\| exit 0` | 0 | 0 |
| `loom-relay-guard.sh` | `:37,46-55`, `jq … \|\| exit 0`, then not Bash | 0 | 0 |
| `commit-filter.sh` | `:47,54,96-98`, `TOOL_NAME != Bash` → exit 0 | 0 | 0 |
| `worktree-isolation.sh` | `:58,66`, `case *)` falls to `exit 0` (`:398`) | 0 | 0 |
| `git-add-guard.sh` | `:65,68-74` | 0 | 0 |
| `loom-control-complete.sh` | `:314` `[[ -n ]] \|\| exit 0`; `:316` not Bash → exit 0 | 0 | 0 |
| `loom-control-subagents.sh` | `:355` `[[ -n ]] \|\| exit 0`; `:356` not Bash → exit 0 | 0 | 0 |
| `worktree-file-guard.sh` | `:17-35`, blocks only when the input is non-empty and has no tool name | 0 | 2 |
| `credential-guard.sh` | `:46-64`, the same rule | 0 | 2 |
| `codex-forward-guard.sh` (the reference) | `:94` | 2 | 2 |

- The same pattern also appears in `commit-guard.sh`, `spawn-guard.sh`, `poll-guard.sh`, `read-guard.sh`, `plans-path-guard.sh`, `stage-terminal-guard.sh` and `subagent-verify-guard.sh`, which all returned 0/0. Those runs were outside a worktree, so the result shows only that a parse failure does not block.
- Empty or truncated input reaches these hooks only when `loom_read_input` (`loom-hooks/_bounded.sh:78-80`, `cat` under a 1 s or 5 s wall clock) times out. An agent cannot induce that directly; it becomes likely on a heavily loaded host.

**Fix seam:**

- Add `loom_read_payload_or_block <seconds> <hook-name>` to `loom-hooks/_bounded.sh`, next to `loom_read_input`.
  - It reads the input. When the input is empty, or when `jq -e 'type=="object" and (.tool_name|type=="string")'` fails, it prints one refusal line naming the hook and exits 2.
  - It exits 2 without jq, the same as `loom_require_jq`.
- Call it in place of `INPUT_JSON=$(loom_read_input …)` in every PreToolUse-blocking guard listed above, in their `pre` mode only. `codex-apply-patch.sh post` and PostToolUse hooks keep failing open.
- Keep `loom-relay-guard.sh`'s early `LOOM_SESSION_ID` and `LOOM_SCRATCH_DIR` gate (`:31`) before the read, so non-loom sessions are unaffected.
- Remove the per-hook `[[ -n "$INPUT_JSON" && -z "$TOOL_NAME" ]]` blocks in `worktree-file-guard.sh:21-35` and `credential-guard.sh:50-64`, which the helper replaces.

**Regression test:**

- Add `loom-hooks/tests/blocking-guards-bad-payload.sh` and register it in `loom-hooks/tests/run-all.sh`.
- Loop over the hook list and assert exit 2 for `</dev/null` and for `{"tool_name":"Ba`. Every hook except `codex-forward-guard.sh` exits 0 today.

**Files a fix touches:** `loom-hooks/_bounded.sh`, the nine listed hooks plus the seven siblings, `loom-hooks/tests/blocking-guards-bad-payload.sh`, `loom-hooks/tests/run-all.sh`.

**Size / tier:** M; mechanical (security-relevant; codex-suitable with an exact hook list).

**Overlap:** CR-03, CR-05, CR-67, CR-68 (the same hook files).

## CR-67 — `loom-control-complete.sh` writes the completion heartbeat for non-main callers

**Verdict:** NARROWED

**Evidence:**

- `loom-hooks/loom-control-complete.sh:367-374`: when the command equals `PINNED_COMMAND` and `completion_caller_is_main` (`:286-293`) is false, line `:372` calls `loom_write_completion_heartbeat` and the hook exits 0.
- The heartbeat (`loom-hooks/_post-tool-heartbeat.sh:167-185`) marks `Stage completion running`. `orchestrator/monitor/stall_line.rs:1-17` then grants the longer completion stall line, and later subagent writes keep the mark (`_post-tool-heartbeat.sh:113-121`).
- For a positive subagent (`loom_is_subagent`), `commit-filter.sh:293-325` refuses the command, so the heartbeat is spurious and delays hung-session detection.
- `completion_caller_is_main` is also false for a caller whose verdict is `unknown` and who has a `transcript_path`. That caller may be the main agent, and its command runs (with no allow, so the classifier decides). There, the heartbeat is correct.
- Removing line `:372` outright, as the review recommends, would drop the stall allowance for that case. `git log -L` shows line 372 is the original main-path heartbeat from before the allow branch was added (`5c013f5b5`).

**Fix seam:** at `loom-hooks/loom-control-complete.sh:372`, write the heartbeat only when `! loom_is_subagent "$INPUT_JSON"`. That is the same predicate `commit-filter.sh:293` uses to refuse, so the heartbeat is written exactly when the command will actually run.

**Regression test:**

- Extend `loom-hooks/tests/loom-control-complete-heartbeat.sh`.
- Send a payload with `agent_type` set and the pinned command. Assert that no heartbeat file carries `Stage completion running`. Today one is written.

**Files a fix touches:** `loom-hooks/loom-control-complete.sh`, `loom-hooks/tests/loom-control-complete-heartbeat.sh`.

**Size / tier:** S; mechanical.

**Overlap:** CR-66.

## CR-72 — `ask-user-pre.sh` interpolates `LOOM_STAGE_ID` into the no-jq JSON and `osascript`

**Verdict:** CONFIRMED (defense in depth: `LOOM_STAGE_ID` comes from loom's launch environment, which an agent cannot change for hook processes)

**Evidence:**

- `loom-hooks/ask-user-pre.sh:28` checks only that the value is non-empty.
- `:100` writes `${LOOM_STAGE_ID}` raw into a JSON heredoc.
- `:116` builds `MESSAGE`, and `:121` runs `osascript -e "display notification \"$MESSAGE\" …"`. A `"` in the id would break out into AppleScript, which can `do shell script`.
- The sibling `loom-hooks/ask-user-post.sh:28,101` has the same no-jq fallback.
- `validate_id` (`loom/src/validation.rs:74-76`) allows only `[A-Za-z0-9_-]`.

**Fix seam:**

- In both `ask-user-pre.sh` and `ask-user-post.sh`, right after the `:28` check, add `case "$LOOM_STAGE_ID$LOOM_SESSION_ID" in *[!A-Za-z0-9_-]*) exit 0 ;; esac`. This is the same character class `loom-control-complete.sh` uses. Use `[A-Za-z0-9_-]`, not the review's `._-`.
- At `ask-user-pre.sh:121`, use `osascript -e 'on run argv' -e 'display notification (item 1 of argv) with title "loom"' -e 'end run' "$MESSAGE"`.

**Regression test:** extend `loom-hooks/tests/ask-user-hooks.sh`. Run with `LOOM_STAGE_ID='a"b'` and with jq hidden from `PATH`. Assert that `events.jsonl` gets no line, or that the line parses as JSON. Today a line that does not parse is written.

**Files a fix touches:** `loom-hooks/ask-user-pre.sh`, `loom-hooks/ask-user-post.sh`, `loom-hooks/tests/ask-user-hooks.sh`.

**Size / tier:** S; mechanical.

**Overlap:** none.
