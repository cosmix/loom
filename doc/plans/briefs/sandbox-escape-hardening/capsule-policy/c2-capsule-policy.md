# C2: capsule policy (session filesystem layer, grant removal, credential list, codex grants)

Stage `capsule-policy`, wave 1, tier sonnet. Read `../common.md` first (D1 capsule half, D3's
grant removal, D4, D5 and D6 at the capsule level are yours).

## Why

The capsule (`sandbox::build_settings` plus `orchestrator/terminal/native/session_settings`)
still grants every session the operator's package caches (`PACKAGE_MANAGER_CACHE_WRITE_PATHS`),
the whole `~/.codex`, and a carve-out under `~/.claude/plugins`; it leaves the git common
directory writable to stage sessions (Claude Code grants it for a linked worktree, and loom denies
only `.git/hooks` and `.git/config`), read-denies five credential paths, and lets a stage read
every sibling worktree. You write the pure policy layer that fixes all of that, delete the
package-cache grant, and move the codex grant to the new layout. C3 calls your new function from
the capsule writer; C1 writes the cache seeding it relies on.

## Files you own

`loom/src/sandbox/mod.rs`, `loom/src/sandbox/package_caches.rs` (delete it),
`loom/src/sandbox/session_fs.rs` (new), `loom/src/sandbox/session_fs/tests.rs` (new),
`loom/src/sandbox/settings/policy.rs`, `loom/src/sandbox/settings/policy/tests.rs`,
`loom/src/sandbox/settings/tests.rs`, `loom/src/sandbox/grant_paths.rs`,
`loom/src/sandbox/control_surfaces.rs`, `loom/src/sandbox/control_surfaces/session_denies.rs`,
`loom/src/sandbox/control_surfaces/tests.rs`,
`loom/src/sandbox/control_surfaces/tests_session_denies.rs`,
`loom/src/fs/permissions/state_root.rs`, `loom/src/codex.rs`, `loom/maintainability-baseline.txt`,
and the one doc comment at `loom/src/models/stage/types.rs:258` (section 6; nothing else in that
file).

Not yours, even though they break when you remove items: every file under
`loom/src/orchestrator/terminal/native/` (C3 updates `session_settings.rs`,
`tests_capsule_contents.rs`, `tests_capsule_interpreters.rs` and the rest). Anchors below are at
`3fc28031`; locate by symbol.

## 1. `sandbox/mod.rs`

Remove `mod package_caches;` and `pub use package_caches::PACKAGE_MANAGER_CACHE_WRITE_PATHS;`
(`mod.rs:9`, `:19`). Add, in module order:

    pub mod session_cache;
    pub mod session_fs;

`session_cache` is C1's module (`loom/src/sandbox/session_cache/mod.rs`); you only declare it.
From it you use three pinned constants:

    pub const CODEX_HOME_DIR: &str = "codex-home";
    pub const CODEX_PROTECTED_FILES: [&str; 2] = ["config.toml", "hooks.json"];
    pub const SESSION_OWNER_MARKER: &str = ".loom-session";   // C/.loom-session holds the session id

Delete `loom/src/sandbox/package_caches.rs` with its five tests. The plan records their removal
as an expected integrity event; the property they pinned (no credential-bearing parent, no
executable install directory is granted) holds trivially now, because no cache is granted at
all.

## 2. The session filesystem layer: `sandbox/session_fs.rs` (new, pub)

The public surface (pinned: C3 and the contracts call it exactly like this):

    pub struct SessionFsInputs<'a> {
        pub kind: SessionType,                 // crate::models::session::SessionType
        pub repo_root: &'a Path,
        pub worktree: Option<&'a Path>,        // Some when the cwd is a stage worktree
        pub git_common_dir: &'a Path,
        pub cache_dir: &'a Path,               // the session's C
        pub home: Option<&'a Path>,
        pub codex_home: &'a Path,              // the operator's real codex home
        pub codex_licensed: bool,
        pub credential_paths: &'a [PathBuf],
        pub real_caches: &'a [PathBuf],         // RealCaches::cache_paths() of the daemon
    }

    #[derive(Debug, Default, Clone, PartialEq, Eq)]
    pub struct SessionFs {
        pub allow_write: Vec<String>,
        pub deny_write: Vec<String>,
        pub deny_read: Vec<String>,
        pub allow_read: Vec<String>,
        pub edit_deny: Vec<String>,
    }

    pub fn session_filesystem(inputs: &SessionFsInputs<'_>) -> anyhow::Result<SessionFs>;
    pub fn resolve_git_common_dir(repo_root: &Path) -> anyhow::Result<PathBuf>;
    pub fn refuse_real_cache_grants(allow_write: &[String], home: Option<&Path>, base: &Path,
        cache_dir: &Path, real_caches: &[PathBuf]) -> anyhow::Result<()>;
    pub fn env_credential_paths(lookup: &dyn Fn(&str) -> Option<std::ffi::OsString>)
        -> Vec<PathBuf>;

Spelling: sandbox-list entries are plain absolute strings (`path.to_str()`, as
`session_denies` spells its `denyWrite` paths); an `Edit` rule is the `edit_rule` shape
`session_denies` already uses: `Edit(//abs/**)` for a directory, `Edit(//abs)` for a file.
Reuse `session_denies::literal` and `edit_rule` and `control_surfaces::push_unique` (make them
`pub(crate)` and re-export them from `control_surfaces.rs`); change `literal`'s message from
"cannot deny writing {text}" to "cannot name {text} in a sandbox rule" (the existing test only
checks that the message names the path). Do not duplicate them.

`session_filesystem` builds, in this order, deduplicated:

1. **D3.** `allow_write += cache_dir` (literal; `Err` on a non-UTF-8 or glob-holding path).
   When `cache_dir/SESSION_OWNER_MARKER` is a regular file (`symlink_metadata`), also
   `deny_write +=` it and `edit_deny += Edit(//<it>)`: `C`'s path is shared by every session of
   one stage and kind, and retirement removes `C` only for the session the marker names, so the
   session must not be able to rewrite it. The daemon writes it before the session starts, so it
   exists whenever the capsule is built; an absent one is not listed.
   Then, for every kind, each `real_caches` entry whose `symlink_metadata` succeeds and that is
   neither equal to nor an ancestor of `home`, `repo_root`, `worktree` or `cache_dir`:
   `deny_write +=` it and, when `canonicalize` gives a different path (a symlinked alias), that
   path too; `edit_deny +=` the directory or file rule for each. A plan can no longer grant a
   real cache back (`refuse_real_cache_grants`), and a cache the operator relocated into a
   writable root (`CARGO_HOME` inside the worktree) is denied. The ancestor rule keeps the real
   `~/.cache` (`RealCaches::xdg_cache`) undenied: the production `C` lies under it. Never list
   an absent one (the placeholder rule of step 2).
2. **D1.** When `kind` is `Stage`, `Contract` or `Adjudication`, whatever the cwd:
   `deny_write += git_common_dir`, `edit_deny += Edit(//<git_common_dir>/**)`. A judge never
   writes git, and one running in the checkout would otherwise hold `R/.git`. For `Knowledge`,
   `Merge` and `BaseConflict` (they commit in the checkout) instead, for each of `info`,
   `objects/info`, `worktrees`, `modules`, `refs/replace`, `hooks` and `config` under
   `git_common_dir` (the last two because `session_denies` names only the literal
   `R/.git/hooks` and `R/.git/config`, which a `--separate-git-dir` store does not have) whose
   `symlink_metadata` succeeds at the call: `deny_write += <path>`, `edit_deny +=` the directory
   or file rule by its metadata. **Never list an absent one**: on Linux a `denyWrite` of an
   absent path mounts an empty placeholder that the HOST sees as an empty file while the command
   runs, and host git reading an empty `commondir`-style entry breaks
   (`architecture/execution-containment.md`, "A Denied Missing Path Shows Up in the Session's
   `git status`"). Name the kind split and the five entries as documented constants.
3. **D5.** When `worktree` is `Some(t)`: `deny_read += repo_root/.worktrees`,
   `allow_read += t`. Claude Code masks a denied directory with a tmpfs and re-mounts
   `allowRead` and `allowWrite` paths inside it. The main checkout stays readable (in-session
   loom reads config, knowledge and retrieval state through `main_project_root`).
4. **D4, env-derived.** `deny_read +=` every `credential_paths` entry that is UTF-8, absolute,
   free of glob characters, and neither equal to nor an ancestor of `home`, `repo_root`,
   `worktree` or `cache_dir` (a `GNUPGHOME` pointing at the home directory must not mask the
   whole home). Silently skip the rest. Then, for every kind, the main checkout's secrets files:
   list `repo_root` once (`read_dir`, top level only) and `deny_read +=` each entry named exactly
   `.env` or starting with `.env.` whose `symlink_metadata` is a regular file. Never a glob,
   never a directory, never an absent path (a `.env.d/` directory, a symlink and `.envrc` are not
   listed). A `repo_root` that does not exist lists nothing (the contracts pass TempDir paths
   they never create); any other listing error is an `Err`.
5. **D6.** When `codex_licensed`: `allow_write += codex_home/auth.json` (a single file; codex
   rewrites it in place with truncate+write, `codex-rs/login/src/auth/storage.rs`
   `FileAuthStorage::save`), and for each `CODEX_PROTECTED_FILES` name, when
   `cache_dir/CODEX_HOME_DIR/<name>` is a regular file (`symlink_metadata`), `deny_write +=` it
   and `edit_deny += Edit(//<it>)`: the copies C1 makes of codex's hook registrations and
   config must not be replaceable, as `~/.codex/hooks.json` and `config.toml` never were. When
   not licensed: `deny_read += codex_home/auth.json`.

`resolve_git_common_dir(repo_root)`: `crate::git::runner::run_git_checked(&["rev-parse",
"--path-format=absolute", "--git-common-dir"], repo_root)`, canonicalize the answer, require a
directory, context on every step naming `repo_root`. `git/worktree/pinned.rs:111` (`common_dir`)
does the same privately in another stage's territory; do not edit it. The duplicate is accepted
(integration-verify may consolidate). Integration-verify classifies `session_fs.rs`'s
`run_git_checked(` call in host-git-integrity's allowlist (`src/sandbox/session_fs.rs |
host-on-R`); this stage does not edit `tests/host_git_call_sites.txt`.

`refuse_real_cache_grants(allow_write, home, base, cache_dir, real_caches)`: resolve each entry
(`~/x` under `home`, skipped when `home` is `None`; `//x` and `/x` absolute; anything else under
`base`, the session's cwd), cut it at its first path component holding a glob character, and
skip it when it equals or lies inside `cache_dir`. `bail!` naming the entry and the cache when
it equals, lies inside or lies above a `real_caches` path, compared lexically and, where both
paths exist, again after canonicalizing both (so `<alias>/**` for a cache named through a
symlink is caught). `Ok(())` otherwise. Path comparison by components (`Path::starts_with`),
never by string prefix.

`env_credential_paths(lookup)`, in this order, skipping empty or relative values and
duplicates: `GH_CONFIG_DIR` + `/hosts.yml`; `DOCKER_CONFIG` + `/config.json`; each `:`-separated
entry of `KUBECONFIG`; `NPM_CONFIG_USERCONFIG`; `npm_config_userconfig`; `CARGO_HOME` +
`/credentials.toml` and + `/credentials`; `GNUPGHOME`; `PASSWORD_STORE_DIR`;
`AWS_SHARED_CREDENTIALS_FILE`; `AWS_CONFIG_FILE`; `CLOUDSDK_CONFIG`; `AZURE_CONFIG_DIR`.
`CODEX_HOME` is not here: `codex_home` (step 5) already follows it.

Keep `session_filesystem` under 50 lines by giving each step its own function. Declare the
tests with `#[cfg(test)] mod tests;` (file `session_fs/tests.rs`).

## 3. `sandbox/settings/policy.rs`

- Remove `PACKAGE_MANAGER_CACHE_WRITE_PATHS` from the import (`policy.rs:5`) and the loop that
  pushes it (`:154-156`). Rewrite the comment above the `allow_write` block (`:140-143`): plan
  entries reach `allowWrite` directly, codex's plugin-data grant is added when that lane is
  licensed, and no package cache is granted (each session's own cache directory is granted by its
  capsule, `sandbox::session_fs`).
- Nothing else in `policy.rs` changes; in particular `MANDATORY_DENY_READ` keeps pointing at
  `CREDENTIAL_DENY_READ_PATHS`, so the new entries reach every capsule and every
  `generate_settings_json`.

`policy/tests.rs` (these edits are the plan's expected integrity events; change nothing else):

- `grants_the_codex_lane_its_state_dirs`: delete the package-cache ordering block (`:55-69`, from
  the comment "Package-manager caches are granted..." through the ordering assert). The rest
  stays.
- `claude_only_allow_write_is_plan_paths_then_package_caches`: replace with
  `claude_only_allow_write_is_exactly_the_plan_paths` (`allowWrite == ["src/**"]`).
- `codex_licensed_allow_write_appends_codex_state_paths`: drop the
  `expected.extend(PACKAGE_MANAGER_CACHE_WRITE_PATHS);` line, so the expectation is
  `["src/**", "~/.codex/plugin-data"]`.
- `every_stage_gets_the_package_caches_even_with_no_plan_entries`: replace with
  `no_stage_is_granted_a_package_cache` (exact name; acceptance runs it): a claude-only
  `config()` emits no `allowWrite` key at all, and the same config with the codex lane emits
  exactly `["~/.codex/plugin-data"]` (the control: the lane still gets its one grant).

## 4. `codex.rs`

`CODEX_SANDBOX_WRITE_PATHS` becomes `[&str; 1] = ["~/.codex/plugin-data"]` and
`CODEX_PLUGIN_DATA_GRANT` (`codex.rs:121-124`) is deleted. Rewrite the doc comment on
`CODEX_SANDBOX_WRITE_PATHS` to the new truth: codex runs with `CODEX_HOME` inside the session's
own cache directory (granted by the capsule), so no capsule grants `~/.codex`; the codex
companion records each job under `~/.codex/plugin-data` (`loom-hooks/codex-forward.sh` sets
`CLAUDE_PLUGIN_DATA` there, and host-side readers such as `models/forward_receipt/locator.rs`
and `codex_lifecycle/authorization.rs` read it), which this constant grants; the single file
`<codex home>/auth.json` is granted per session by `sandbox::session_fs`. Keep the paragraph
about never using `dangerouslyDisableSandbox`.

## 5. `control_surfaces.rs` and `control_surfaces/session_denies.rs`

- `control_surfaces.rs:20` and `:98`: remove the package-cache import and the
  `grants.extend(PACKAGE_MANAGER_CACHE_WRITE_PATHS)` line. The `CODEX_SANDBOX_WRITE_PATHS` line
  stays (it now holds only the plugin-data grant). Rewrite `session_writable_roots`'s doc
  (`:91-95`): the repository, each stage's granted `allowWrite` directory, the codex plugin-data
  grant when licensed, the scratch root, `/tmp` and `$TMPDIR`; the launch adds the session cache
  root itself (C3), which keeps `WritableRootInputs`'s fields unchanged (the sandbox-canary
  stage constructs it literally).
- `HOME_SURFACES` doc (`:53-58`): the `~/.codex` hook and config files stay denied in every
  capsule although no capsule grants `~/.codex` any more, so a plan's own grant cannot reopen
  them; `~/.claude/plugins` is denied whole by `session_denies`.
- `session_denies.rs`: remove `DenyInputs::plugin_entries` (`:33-35`) and deny
  `~/.claude/plugins/**` for every session (`:77-80` becomes one `denies.home(...)` call; keep
  `PLUGINS_DIR` if you still use it, else inline). Delete `codex_plugin_entries` (`:192-222`),
  `list_dir` (`:224-232`), the `DirEntry` and `CODEX_PLUGIN_DATA_GRANT` imports, and the
  re-export in `control_surfaces.rs:25`. Rewrite the `writable_roots` field doc (`:36-42`) and
  `is_ancestor_of_writable_root`'s doc (`:132-145`): the example of `~/.bun` on `PATH` with a
  granted `~/.bun/install/cache` no longer exists; use `~/.cache` on `PATH` above the session
  cache root.
- `control_surfaces/tests.rs`: `writable_roots_cover_every_input` drops
  `/home/op/.cargo/registry`, `/home/op/.bun/install/cache`, `/home/op/.codex` and
  `/home/op/.claude/plugins/data/codex-openai-codex` and gains `/home/op/.codex/plugin-data`;
  `writable_roots_omit_the_codex_paths_unless_the_lane_is_licensed` asserts
  `/home/op/.codex/plugin-data` absent unlicensed and `/home/op/.cargo/registry` absent (it
  asserted present). Add `writable_roots_hold_no_real_package_cache`
  (`/home/op/.cargo/registry`, `/home/op/.npm`, `/home/op/.cache/uv` absent licensed or not)
  paired with the licensed case holding `/home/op/.codex/plugin-data`.
- `control_surfaces/tests_session_denies.rs`: drop the `plugin_entries` parameter from
  `denies_for` and from every call, drop `plugin_entries: None,` from every `DenyInputs`
  literal, delete `codex_plugin_entries_deny_everything_beside_the_codex_grant`,
  `a_home_without_plugins_lists_nothing` and
  `plugin_entries_replace_the_whole_plugins_deny_in_both_layers`, and add
  `every_session_denies_the_whole_plugins_directory_in_both_layers` (`~/.claude/plugins` in
  `deny_write`, `Edit(~/.claude/plugins/**)` in `edit`).

## 6. `fs/permissions/state_root.rs`: D4's literal list

`CREDENTIAL_DENY_READ_PATHS` (`state_root.rs:51`) becomes `[&str; 41]`: the five entries as they
are, then these 36 plain paths, in this order (no glob: a literal entry mounts nothing when
absent, `/dev/null` over a file, a tmpfs over a directory, and triggers no recursive
expansion; `concerns/sandbox-and-confinement-gaps.md`, "No `Read(...)` Deny Rule May Exist in
Any Settings File"). Format the constant one entry per line, each as `"<path>",` with a
trailing comma, as it is formatted today: the plan's acceptance greps `state_root.rs` for the
literal lines `"~/.netrc",`, `"~/.config/gh/hosts.yml",`, `"~/.npmrc",` and `"~/.claude.json",`.

    ~/.config/gh/hosts.yml  ~/.netrc  ~/.npmrc  ~/.yarnrc.yml  ~/.git-credentials
    ~/.config/git/credentials  ~/.docker/config.json  ~/.kube  ~/.pypirc
    ~/.cargo/credentials.toml  ~/.cargo/credentials  ~/.config/hub  ~/.config/glab-cli
    ~/.terraform.d/credentials.tfrc.json  ~/.azure  ~/.local/share/keyrings
    ~/.password-store  ~/.config/op  ~/.vault-token  ~/.gem/credentials  ~/.mozilla
    ~/.config/google-chrome  ~/.config/chromium
    ~/.bash_history  ~/.zsh_history  ~/.config/rclone  ~/.boto  ~/.s3cfg
    ~/.m2/settings.xml  ~/.gradle/gradle.properties  ~/.config/pypoetry/auth.toml
    ~/.config/doctl  ~/.wrangler  ~/.config/netlify  ~/.config/stripe  ~/.claude.json

`~/.claude.json` is denied to the sandboxed Bash only: Claude Code itself and every hook run
outside the sandbox, and nothing inside a session reads it (only the host-side
`fs/permissions/trust.rs` and `control_surfaces.rs`'s `HOME_SURFACES` name it). Say so in the
const's doc comment.

Consumers pick the list up unchanged: `policy.rs:13` (every capsule),
`models/stage/types.rs::default_deny_read` (a plan's default), `is_loom_written_read_deny`
(`state_root.rs:125`, which `loom init` and `loom repair` use to strip `Read(...)` mirrors).
Extend the const's doc: an operator-authored `Read(...)` deny naming one of the new paths is now
recognized as loom's mirror and stripped on `loom init` or `loom repair`; the OS deny carries the
boundary instead (no `Read(...)` rule may exist anyway). Existing tests iterate the const and
stay green. Add `every_credential_path_is_a_literal_home_path` (each entry starts with `~/`,
holds no `..`, and holds a glob only as the trailing `/**` of the original five).

`models/stage/types.rs:258` documents the `deny_read` default as "~/.ssh/**, ~/.aws/**,
~/.config/gcloud/**, ~/.gnupg/**"; the default is `default_deny_read` (`:340`): every
`CREDENTIAL_DENY_READ_PATHS` entry, the state-root secret files in both layouts, and the two
`../` escape patterns. Rewrite that doc comment to say so by name, without listing paths. Change
nothing else in the file, and keep the file's line count: `types.rs` is ledgered at exactly 1015
lines (`maintainability-baseline.txt`, `file src/models/stage/types.rs 1015`), so the two doc lines
at `:257-258` become exactly two lines.

## 7. `grant_paths.rs` and `settings/tests.rs`

- `grant_paths.rs:59-63`: `warn_missing_grants`'s doc names the removed constant; say it warns
  about plan grants only (every other grant loom adds is created at spawn).
- `settings/tests.rs`: line 8's import drops `PACKAGE_MANAGER_CACHE_WRITE_PATHS`; rename the
  helper `allow_write_with_caches` (`:46-51`) to `plan_allow_write`, body
  `if prefix.is_empty() { Value::Null } else { json!(prefix) }`, doc
  "`allowWrite` as the builder emits it for a claude-only stage: exactly the plan entries (no
  package cache is granted; each session's cache is granted by its capsule)"; update its four
  call sites (`:268`, `:596`, `:649`, `:825`) and the comment at `:267`. The empty case is
  `Value::Null` because the call at `:596`, `plan_allow_write(&[])`, runs on a hand-built config
  with `allow_write: vec![]`, and `policy.rs` emits `allowWrite` only when it is non-empty, so
  the key is absent and indexing `fs_block["allowWrite"]` gives `Null`; `json!(prefix)` would be
  `[]` and fail that assert. Nothing else in the file changes.

## 8. `maintainability-baseline.txt`

After your edits, count and set exactly: remove `function src/sandbox/settings/policy.rs
filesystem_settings 52` if `filesystem_settings` is now 50 lines or fewer (it should be about
49), otherwise set its new count; set `file src/sandbox/settings/tests.rs` to the file's new
`wc -l`. Touch no other line (`file src/models/stage/types.rs 1015` stays: section 6 keeps that
file's line count). `session_fs.rs` and its tests stay under 400 lines, every
function under 50.

## Tests to add in `session_fs/tests.rs` (allowed case beside the denied case)

TempDir paths only. Git fixtures run `git` with `GIT_CONFIG_GLOBAL` and `GIT_CONFIG_SYSTEM`
pointed at missing files, `GIT_CONFIG_NOSYSTEM=1`, and a local `user.name`/`user.email` (mirror
`verify/impact_tests_tests.rs`'s git helper).

- `no_git_kinds_deny_the_git_common_dir_in_both_layers` (exact name; acceptance runs it): Stage,
  Contract and Adjudication, each with `worktree` `Some` and `None`, list the common directory in
  `deny_write` and `Edit(//<it>/**)` in `edit_deny`.
- `checkout_kinds_deny_only_the_existing_git_metadata_entries` (exact name; acceptance runs it):
  a common directory holding `info/`, `objects/info/` and `worktrees/` but no `modules`,
  `refs/replace`, `hooks` or `config`; Knowledge, Merge and BaseConflict list exactly those
  three in both layers, never the common directory itself and never an absent entry.
- `checkout_kinds_deny_the_hooks_and_config_of_a_separate_store` (exact name; acceptance runs
  it): `git init --separate-git-dir <t>/store.git <t>/repo`; a Knowledge layer with
  `git_common_dir` the canonical store lists `<store>/hooks` (directory rule) and
  `<store>/config` (file rule) in both layers.
- `existing_real_caches_are_write_denied_and_ancestors_of_the_session_are_not`: a `real_caches`
  list holding an existing directory, an absent path, a symlink to a directory, the parent of
  `cache_dir` and `home` itself; `deny_write` holds the directory, the symlink and its target,
  and none of the other three.
- `real_cache_grants_are_refused_by_equality_containment_and_ancestry` (`~/.npm`,
  `~/.npm/_cacache/**`, `~` and a canonical alias each `Err`; an entry under `cache_dir` and
  `src/**` `Ok`).
- `sibling_worktrees_are_read_denied_and_the_own_worktree_re_allowed`, paired with
  `a_checkout_session_reads_every_worktree` (no `.worktrees` entry, empty `allow_read`).
- `credential_paths_are_read_denied_unless_they_would_cover_the_session` (an ordinary path kept;
  the home itself, an ancestor of the repository, a relative path and a globbed path dropped).
- `codex_auth_is_read_denied_without_the_lane_and_writable_with_it`.
- `codex_protected_copies_are_write_denied_only_when_present` (a regular `config.toml` copy is
  denied in both layers; with `hooks.json` absent, it is not listed).
- `every_kind_is_granted_its_session_cache` (`allow_write` holds `cache_dir` and never its
  parent, the session-caches root that holds every other stage's cache) and
  `a_glob_in_the_cache_dir_is_refused`.
- `top_level_env_files_are_read_denied_for_every_kind` (exact name; acceptance runs it): a
  `repo_root` holding regular files `.env` and `.env.production`, a directory `.env.d`, a file
  `.envrc` and `sub/.env`; every kind lists exactly `repo_root/.env` and
  `repo_root/.env.production` in `deny_read`, paired with the control that a `repo_root` holding
  none of them adds no `.env` entry.
- `the_owner_marker_is_write_denied_when_present` (a regular `C/.loom-session` is listed in both
  layers) paired with `an_absent_owner_marker_is_not_listed`.
- `env_credential_paths_follow_each_variable` (every variable above, `KUBECONFIG` split in
  three, an empty and a relative value skipped).
- `resolve_git_common_dir_finds_a_separate_git_dir` (`git init --separate-git-dir`: the answer
  is the store, not `<repo>/.git`, which is a gitfile) paired with
  `resolve_git_common_dir_refuses_a_plain_directory`.

## Traps

- `denyWrite` of an absent path is not free: it plants a placeholder on the host. Every deny you
  add under the git directory or `C` checks existence first; the common directory itself exists
  by construction (`resolve_git_common_dir` requires it).
- Never emit a `Read(...)` permission rule and never a `denyRead` glob; D4 entries are literal.
- `allowRead` is a new capsule key; C3 writes it into the capsule from your `allow_read`. You
  only return it.
- Keep `WritableRootInputs` and `session_writable_roots`'s signature exactly as they are: the
  sandbox-canary stage's `tests_confinement_e2e.rs` constructs `WritableRootInputs` literally.
- The crate will not compile after your wave (native files still name `plugin_entries` and
  `codex_plugin_entries`); that is expected, C3 fixes them.

## Check

None: the crate compiles again only after C3 (wave 2). Report every deviation from the pinned
surface (there should be none), the exact ledger lines you changed, and the list of existing
test functions you changed or deleted.
