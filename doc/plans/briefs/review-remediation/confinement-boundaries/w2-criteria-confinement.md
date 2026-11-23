# confinement-boundaries / w2-criteria-confinement: state-root parent, proxy credentials, host-job socket, developer tools, GOPROXY parse

Tier: sonnet
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/verification.md`, sections CR-26, CR-34,
CR-35; `doc/plans/briefs/review-remediation/validation/runtime.md`, section CR-74 sub-claims (a)
and (e).

## Findings covered

- CR-26: a confined knowledge criterion can rename `<tree>/.loom` (not a mount point) and plant a
  writable `.loom/work` on the host.
- CR-34: `HTTP(S)_PROXY`/`ALL_PROXY` with URL userinfo reach every confined command, so a test the
  agent wrote can print the credential.
- CR-35: the host job's sccache socket is `<cache root>/h.sock`, in a root the stage session can
  write: the session can pre-bind it or connect to the host job's server.
- CR-74(a): `DeveloperTools::detect` caches a failed lookup for the life of the process.
- CR-74(e): `goproxy_has_userinfo` splits at the first `://` anywhere in an entry, so
  `user:pw@proxy.example/x?next=https://a` hides its userinfo.

## Files

Own (write):
`loom/src/verify/criteria/confine_protected.rs`; `loom/src/verify/criteria/confine_protected_tests.rs`;
`loom/src/verify/criteria/confine_host.rs`; `loom/src/verify/criteria/confine.rs`;
`loom/src/verify/criteria/confine_scratch.rs`; `loom/src/verify/criteria/confine_seatbelt.rs`;
`loom/src/verify/criteria/confine_seatbelt_network_tests.rs`;
`loom/src/verify/criteria/confine_stage_cache_tests.rs`;
`loom/src/stage_cache/environment.rs`; `loom/src/stage_cache/mod.rs`;
`loom/src/stage_cache/developer.rs`; `loom/src/stage_cache/tests.rs`;
`loom/src/stage_cache/contract_tests.rs`; `loom/src/stage_cache/host_env_tests.rs`;
`loom/src/stage_cache/toolchain_tests/mod.rs`.

Read only: `loom/src/process/environment.rs`, `loom/src/daemon/server/environment.rs`,
`loom/src/verify/criteria/confine_scratch/reclaim.rs`, `loom/src/fs/permissions/state_root.rs`.

Frozen (contract tests, never edit): `loom/src/verify/criteria/state_root_parent_contract_tests.rs`,
`loom/src/verify/criteria/proxy_credentials_contract_tests.rs`.

`confine_host.rs` is 384 lines and `confine.rs` 379: neither may grow by more than two lines.

## Changes

### 1. CR-26 (`confine_protected.rs`)

- `Protected` gains `pinned: Vec<PathBuf>`, documented: directories strictly between the tree
  and a read-only path, each bound onto itself; the kernel refuses to rename a mount point
  (`EBUSY`), and a renamed ancestor would carry the read-only path out from under its bind.
- In `protected()`, after `read_only` is built, fill `pinned`: for each read-only path, its
  ancestors strictly below `tree` (`path.ancestors().skip(1).take_while(|a| *a != tree)`),
  outermost first, kept only when `symlink_metadata` shows a directory that is not a symlink,
  without duplicates. For `<tree>/.loom/work` that is `[<tree>/.loom]`; the legacy `.work` and the
  `.git`/`.worktrees`/`.claude` entries add nothing.
- `bind()`: emit `--bind p p` for each pinned path BEFORE the `--ro-bind` loop.
- `seatbelt_rules()`: emit `(deny file-write* (literal <quoted p>))` for each pinned path before
  the read-only rules. Seatbelt is unmeasured here; say so in the doc comment.
- Visibility, so the stage contract can reach them: `confine_host.rs` declares the module as
  `pub(in crate::verify::criteria) mod protected;`; `Protected`, its four fields, `protected`,
  `bind` and `seatbelt_rules` become `pub(in crate::verify::criteria)`.

### 2. CR-34 (`stage_cache/environment.rs`, `stage_cache/mod.rs`, `confine.rs`)

- In `stage_cache/environment.rs` add
  `const PROXY_ENV_NAMES: [&str; 6] = ["HTTP_PROXY", "HTTPS_PROXY", "ALL_PROXY", "http_proxy", "https_proxy", "all_proxy"];`
  and `pub fn drop_credentialed_proxies(command: &mut std::process::Command)`: collect the names in
  `command.get_envs()` that are in `PROXY_ENV_NAMES` with `Some(value)` where
  `goproxy_has_userinfo(value)`, then `command.env_remove(name)` for each. Drop, never strip: a
  stripped URL fails with 407, and dropping matches the `GOPROXY` policy. Re-export it from
  `stage_cache/mod.rs` beside `goproxy_has_userinfo`.
- `goproxy_has_userinfo` keeps its name and module (its other caller,
  `daemon/server/environment.rs:120`, is outside this stage); extend its doc to say it also reads
  an HTTP proxy variable.
- `confine.rs::apply_confined_environment`: call `crate::stage_cache::drop_credentialed_proxies(command);`
  right after `crate::process::apply_stage_environment(command);`, and make the function
  `pub(super)` (the contract calls it). Provisioning commands go through it too: they lose a
  credentialed proxy as well (a plan needing one uses `Inherit`).
- `process/environment.rs` and the terminal allowlist do not change; leave DISPLAY, XAUTHORITY and
  the bus address alone.

### 3. CR-35 (host-job sccache socket)

Decision: the host job's socket goes in a directory only that job's own command can reach.
Linux: `/tmp`, which bubblewrap mounts as a private tmpfs per command (`bubblewrap()` in
`confine_host.rs`), so no session sees it and `/tmp/h.sock` always fits `sun_path`. macOS: the
command's own scratch directory (`CargoScratch::path()`), fresh, randomly named, created by the
host job; `scratch_socket_rule` already lets the command bind and connect there.

- `stage_cache/environment.rs::StageCache::sccache_socket`: `Consumer::HostJob` becomes
  `self.socket_dir.as_ref()?.join("h.sock")`. Rewrite its doc: every consumer's socket is in the
  directory the caller hands to `with_socket_dir`, none in the shared root.
- `stage_cache/mod.rs::with_socket_dir` doc: covers a host job too.
- `confine_scratch.rs`: add to `CargoScratch`
  `pub(super) fn sccache_socket_dir(&self) -> &Path` returning `Path::new("/tmp")` when
  `cfg!(target_os = "linux")`, else `self.path()`, with the reason above as its doc.
- `confine_host.rs::apply_environment`: the stage-cache line becomes
  `command.envs(cache.clone().with_socket_dir(scratch.sccache_socket_dir()).environment(Consumer::HostJob, tree, &host, None));`.
  Shorten the `Grants::cache_root` doc to "The canonical stage cache root, writable." to keep the
  file at or under 386 lines.
- `confine_seatbelt.rs`: delete `sccache_socket_rule` and its call in `named_rules` (the scratch
  rule covers the socket); update the docs of `named_rules` and `provision_network_rules`, which
  name it, to say the later `scratch_socket_rule` grant is what still wins over the denies.

### 4. CR-74(a) (`stage_cache/developer.rs`)

- Replace the `OnceLock` with `static CACHE: Mutex<Detection>` where
  `struct Detection { found: Option<DeveloperTools>, failed_at: Option<Instant> }` (const
  constructor) and `const RETRY_AFTER: Duration = Duration::from_secs(60);`.
- `detect()` keeps its signature: `None` off macOS; otherwise
  `detect_with(&CACHE, Instant::now(), lookup_tools)`, where `lookup_tools` is today's two
  `lookup` calls.
- `fn detect_with(cache: &Mutex<Detection>, now: Instant, lookup: impl FnOnce() -> Option<DeveloperTools>) -> Option<DeveloperTools>`:
  lock with `unwrap_or_else(PoisonError::into_inner)` and hold the lock across the lookup (one
  `xcrun` at a time); a cached `found` is returned; a failure within `RETRY_AFTER` of `failed_at`
  returns `None` without calling `lookup`; otherwise call it, store a success in `found` or the
  failure time in `failed_at`.
- Update the doc of `detect` and the comment at `confine.rs` ("looks the tools up once per
  process") within the same line count.

### 5. CR-74(e) (`stage_cache/environment.rs::goproxy_has_userinfo`)

`let authority = entry.split_once("://").filter(|(scheme, _)| !scheme.contains(['/', '@'])).map_or(entry, |(_, rest)| rest);`
and say in the doc that `://` counts as the scheme separator only when no `/` or `@` precedes it.

## Interfaces shared with other workers

None.

## Tests

- CR-26, `confine_protected_tests.rs`:
  - `a_tree_holding_its_state_root_protects_it_and_the_control_entries`: also
    `assert_eq!(found.pinned, vec![root.join(".loom")])`.
  - `canary()`: append `; mv .loom .loomx 2>/dev/null && echo STATE_PARENT_MOVED` as the LAST
    check; `assert_only_knowledge_written` also asserts `root.join(".loom/work/config.toml")` is a
    file and `!root.join(".loomx").exists()`. The expected stdout lines stay as they are, so today
    the run fails with `STATE_PARENT_MOVED` (where `confined_commands_runnable()` holds).
  - New `#[cfg(target_os = "linux")]` test
    `bubblewrap_pins_the_state_roots_parent_before_its_read_only_bind`: from `checkout()`, run
    `bind` on a `Command::new("bwrap")` and assert `--bind <root>/.loom <root>/.loom` comes before
    `--ro-bind <root>/.loom/work <root>/.loom/work`.
- CR-34, in a `#[cfg(test)] mod` at the end of `stage_cache/environment.rs` or in
  `stage_cache/host_env_tests.rs`: `credentialed_proxy_variables_are_removed_from_a_command`: a
  `Command` with `env_clear()`, `HTTPS_PROXY=http://u:p@proxy.example:8080`,
  `https_proxy=u:p@proxy.example:8080`, `ALL_PROXY=socks5://u:p@proxy.example:1080`,
  `HTTP_PROXY=http://proxy.example:8080`, `NO_PROXY=localhost`; after the call the first three are
  `None` in `get_envs()` and the last two keep their values.
- CR-35:
  - `stage_cache/tests.rs`: in `session_and_worker_sockets_live_in_their_own_directory` the host
    job on `shared` gets `None` and on `private` gets `/run/session-1/h.sock`; rewrite
    `the_sccache_socket_is_emitted_below_sun_len_and_omitted_at_it` to size the socket DIRECTORY
    (`with_socket_dir`) instead of `XDG_CACHE_HOME`, with the host job; in
    `sccache_sockets_are_distinct_and_each_is_omitted_on_its_own_long_path` the host job's
    expected path is `socket_dir.join("h.sock")`, it is `None` on `long_socket_dir` and on `short`,
    and the long-root case expects `Some` (a long root no longer matters).
  - `stage_cache/contract_tests.rs::host_job_sccache_socket_is_not_the_sessions`: the host job's
    parent is `socket_dir`, like the others; `assert!` that no consumer's socket starts with
    `private.root()`.
  - `stage_cache/toolchain_tests/mod.rs` `Rig::bare`: delete the `<root>/h.sock` length assert and
    fix the module doc (lines 14-15).
  - `confine_stage_cache_tests.rs::a_symlinked_cache_base_gives_variables_under_the_granted_root`:
    `SCCACHE_SERVER_UDS` is `/tmp/h.sock` on Linux, `_scratch.path().join("h.sock")` on macOS, and
    never under `granted`. `only_the_host_job_sccache_socket_is_granted` (macOS): bind
    `$SCCACHE_SERVER_UDS` succeeds and bind of the literal `<cache root>/h.sock` fails; assert
    `!cache.root().join("h.sock").exists()`.
  - `confine_seatbelt_network_tests.rs`: `render` also returns the scratch path;
    `provisioning_with_a_base_cache_denies_every_process_cache_base_before_its_own_socket` finds
    the scratch socket rule (`(allow network-outbound (remote unix-socket (subpath "<scratch>")))`)
    and asserts every deny precedes it; delete `socket_allow` if unused.
- CR-74(a), inline `#[cfg(test)] mod tests` in `developer.rs`:
  `a_failed_lookup_is_retried_after_the_throttle`: a counting lookup failing once then
  succeeding; calls at `t`, `t + 30 s` (lookup not called, `None`), `t + 61 s` (`Some`),
  `t + 62 s` (cached, lookup not called). Fails to compile today.
- CR-74(e), `stage_cache/host_env_tests.rs::a_goproxy_with_userinfo_is_dropped_whichever_entry_carries_it`:
  add `"user:pw@proxy.example/x?next=https://a"` to the credential list.
- Contracts to turn green: `state-root-parent-left-renameable`,
  `credentialed-proxy-reaches-a-confined-command`.

## Proof

From `loom/`: `cargo test --lib verify::criteria::`, `cargo test --lib stage_cache::`.

## Traps

- Emitting the pins after the `--ro-bind`s: the writable self-bind of `.loom` then covers the
  read-only state root.
- Pinning with `--ro-bind`: `.loom` must stay writable for everything but the state root.
- Stripping the userinfo instead of dropping the variable.
- Putting the host-job socket under the state root: a session can read that path, and on Linux a
  read-only bind does not stop `connect` to a socket.
- `Consumer::HostJob` with no socket dir must yield no `SCCACHE_SERVER_UDS`, not a root path.
- `confine_seatbelt_network_tests.rs` renders the profile on any host: run it on Linux too.
- `stage_cache/contract_tests.rs` is an earlier plan's contract file; change only the host-job
  socket expectations, nothing else.
