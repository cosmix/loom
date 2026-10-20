# W2: credential files in every capsule's read denies

Tier: haiku (`loom-software-engineer` with `model: haiku`): every edit below is given exactly. Read `doc/plans/token-optim/briefs/sandbox-route-closure/common.md` first.

## Goal

Every session capsule's `sandbox.filesystem.denyRead` carries nine more credential files as plain paths, whatever the plan's `deny_read` lists, and carries `~/.codex/auth.json` unless the stage licenses the codex lane. `CREDENTIAL_DENY_READ_PATHS` stays exactly as it is.

## Files you own

- `src/fs/permissions/state_root.rs`
- `src/sandbox/settings/policy.rs`
- `src/sandbox/settings/policy/tests.rs`
- `src/sandbox/settings/tests.rs` (two lines, in place)
- `src/models/stage/sandbox_config.rs`

Read only: `src/sandbox/settings.rs` (`build_settings`, `generate_settings_json`: the capsule document is built on `generate_settings_json`, and `build_settings` is called only from the capsule builder, so these denies reach capsules and no other settings file), `tests/integration/capsule.rs`.

Consumers of `CREDENTIAL_DENY_READ_PATHS` that stay correct without an edit, because they check containment and the five entries remain: `src/sandbox/settings/tests_token_rules.rs` (about line 95), `src/commands/repair/tests_token_denies.rs` (about line 136), the `state_root.rs` tests (about line 214). `rg -n CREDENTIAL_DENY_READ_PATHS src tests` must show no other consumer once you are done; a new one is yours to check.

## Why a second list

`state_root::is_loom_written_read_deny` (about line 121) strips `Read(...)` rules naming a `CREDENTIAL_DENY_READ_PATHS` entry from settings files, because older loom versions wrote those mirrors. No loom version wrote a mirror of the new paths, so an operator's own `Read(~/.netrc)` deny must stay the operator's. Putting the new paths into `CREDENTIAL_DENY_READ_PATHS` would make loom delete operator-authored rules.

## Steps

1. **`state_root.rs`**, right after `CREDENTIAL_DENY_READ_PATHS`:

   ```rust
   pub(crate) const CREDENTIAL_FILE_DENY_READ_PATHS: [&str; 9] = [
       "~/.config/gh/hosts.yml",
       "~/.netrc",
       "~/.npmrc",
       "~/.git-credentials",
       "~/.docker/config.json",
       "~/.kube/config",
       "~/.pypirc",
       "~/.cargo/credentials.toml",
       "~/.cargo/credentials",
   ];
   pub(crate) const CODEX_AUTH_DENY_READ_PATH: &str = "~/.codex/auth.json";
   ```

   Doc comments, current truth only: the first names what each file holds (the gh token, netrc logins, npm and PyPI tokens, git credential-store logins, docker registry auths, the kube client config, cargo registry tokens in both spellings), says the entries are plain paths because a `denyRead` glob expands to one mount per match while an absent plain path mounts nothing, and says why the list is separate from `CREDENTIAL_DENY_READ_PATHS` (the paragraph above). The second says a stage that licenses the codex lane keeps reading and refreshing the file in place, and every other session is denied it. In `CREDENTIAL_DENY_READ_PATHS`'s doc, the reference `models::stage::types::default_deny_read` is wrong: the function is `models::stage::sandbox_config::default_deny_read`; correct it.

   Add to the `tests` module: `the_credential_files_are_never_claimed_as_loom_written`: for every entry of `CREDENTIAL_FILE_DENY_READ_PATHS` and for `CODEX_AUTH_DENY_READ_PATH`, `is_loom_written_read_deny(&format!("Read({path})"))` is false.
2. **`policy.rs`.** Replace the const `MANDATORY_DENY_READ` with:

   ```rust
   pub(super) fn mandatory_deny_read(codex_licensed: bool) -> Vec<&'static str> {
       let codex_auth = (!codex_licensed).then_some(CODEX_AUTH_DENY_READ_PATH);
       CREDENTIAL_DENY_READ_PATHS
           .into_iter()
           .chain(CREDENTIAL_FILE_DENY_READ_PATHS)
           .chain(codex_auth)
           .collect()
   }
   ```

   Return the `Vec`, never `impl Iterator<Item = &'static str>`: `deny_read_patterns` chains this onto items that borrow the config, and an opaque iterator fails there with E0521 (common.md). Its doc comment: the read denies every capsule carries whatever its plan lists (the credential directories, the credential files, and the codex credential unless the stage licenses the codex lane); `models::stage::sandbox_config::default_deny_read` supplies the first two lists too, and `deny_read_patterns` dedupes. In `deny_read_patterns`, replace `.chain(MANDATORY_DENY_READ.iter().copied())` with `.chain(mandatory_deny_read(config.implementers.includes_codex()))`. Update the imports. Do not edit `filesystem_settings` (ledgered at exactly 52 lines).
3. **`policy/tests.rs`.** `adds_mandatory_credentials_and_filters_parent_traversal` asserts the list equals `CREDENTIAL_DENY_READ_PATHS.to_vec()`; change its expectation to this literal list, in this order (the fixture plan lists `~/.ssh/**` and `../../**`, the second is filtered, then the mandatory entries follow, deduped):

   ```text
   ~/.ssh/**  ~/.aws/**  ~/.config/gcloud/**  ~/.gnupg/**  ~/.claude/.credentials.json
   ~/.config/gh/hosts.yml  ~/.netrc  ~/.npmrc  ~/.git-credentials  ~/.docker/config.json
   ~/.kube/config  ~/.pypirc  ~/.cargo/credentials.toml  ~/.cargo/credentials  ~/.codex/auth.json
   ```

   Write the expected list as string literals, never built from the consts: a wrong const must fail it. Add two tests:
   - `a_codex_licensed_stage_keeps_reading_its_auth_file`: `config()` with `implementers = Implementers::new(vec![Implementer::Claude, Implementer::Codex])`; the patterns hold every credential file and not `~/.codex/auth.json`.
   - `every_credential_file_entry_is_a_plain_home_path`: every `CREDENTIAL_FILE_DENY_READ_PATHS` entry starts with `~/` and contains none of `* ? [ ] {`.
4. **`settings/tests.rs`** is ledgered at exactly 1565 lines; change two lines in place and nothing else. Line 2, `use crate::fs::permissions::state_root::CREDENTIAL_DENY_READ_PATHS;`, becomes `use super::policy::mandatory_deny_read;`. In `test_generate_settings_keeps_mandatory_credential_deny_when_empty`, the line `json!(CREDENTIAL_DENY_READ_PATHS)` becomes `json!(mandatory_deny_read(false))`. First run `rg -n CREDENTIAL_DENY_READ_PATHS src/sandbox/settings/tests.rs`: if any other line uses the const, keep the import and add `mandatory_deny_read` to a `use` line that already exists instead. Run `wc -l src/sandbox/settings/tests.rs` after `cargo fmt`: it must print 1565.
5. **`sandbox_config.rs`.** In `default_deny_read`, chain `CREDENTIAL_FILE_DENY_READ_PATHS` right after `CREDENTIAL_DENY_READ_PATHS` and update the comment. The `deny_read` field doc says "Default: ~/.ssh/**, ~/.aws/**, ~/.config/gcloud/**, ~/.gnupg/**"; rewrite it to name both lists and the state-root secrets. The codex credential is not a default: it depends on the stage's lanes, so only `mandatory_deny_read` adds it.

## Traps

- Plain paths only, never `/**`: `~/.docker` and `~/.kube` hold more than credentials, and the mandatory list must stay cheap in mounts.
- `CREDENTIAL_DENY_READ_PATHS` keeps its five entries: `is_loom_written_read_deny`, `loom repair` check 14 and `prune_loom_read_denies` read it.
- No `Read(` permission rule anywhere: these paths reach the OS `denyRead` list only.
- A separate contract file, `tests/sandbox_route_contracts.rs`, is frozen before you start; never edit it. It calls `loom::sandbox::generate_settings_json` on configs built with `loom::sandbox::merge_config`.

## Your one check

`cargo test --lib -- sandbox::settings:: fs::permissions::state_root::` from `loom/`, after `cargo fmt` (libtest takes both filters). Report its summary line and `wc -l src/sandbox/settings/tests.rs`.
