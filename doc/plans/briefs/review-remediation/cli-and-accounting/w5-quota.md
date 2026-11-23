# cli-and-accounting / w5-quota: account switch clears published quota, backoff, freshness, dead credential export

Tier: codex sol
Read first: `doc/plans/briefs/review-remediation/common.md`.
Evidence: `doc/plans/briefs/review-remediation/validation/cli-tui-webserver.md`, sections E5, CR-55 (sub-claims a, b, c) and CR-77 (the `quota::credentials::access_token` line only).

You run no `git` command, touch no path under `.loom/`, and run no tests; your proof is `cargo check --all-targets` from `loom/`. You answer from the base tree and cannot see other workers' edits; no other worker edits a file you own. Use `loom map --outline <file>` and `rg` to find the symbols named below.

## Findings covered
- E5: after an account switch the project cache `quota/claude.json` keeps the previous account's windows (or gets an error stamped on them) while the new account has no reading yet.
- CR-55(a): `project_account` backs off after a failed cache write, then overwrites the interval and next-due time with the account's schedule in the same call, so the failing write is retried at the account cadence.
- CR-55(b), (c): `age_secs` overflows on an extreme `observed_at` (`i64::MIN`) and counts a future `observed_at` as age 0, so a skewed or planted reading looks fresh.
- CR-77: `quota::credentials::access_token` has no production caller.

## Files
Own (write):
- `loom/src/quota/poller.rs`
- `loom/src/quota/cache.rs`
- `loom/src/quota/model.rs`
- `loom/src/quota/credentials.rs`
- `loom/src/quota/poller_tests.rs`
- `loom/src/quota/cache_tests.rs`
- `loom/src/quota/model_tests.rs`
- `loom/src/quota/credentials_tests.rs`

Read only: `loom/src/quota/shared.rs` (`AccountState`, `display_error`), `loom/src/fs/cache_io.rs` (`reject_symlink`), `loom/src/orchestrator/signals/format/budget.rs` and `loom/src/commands/status/ui/tui/ledger/quota.rs` (callers of `age_secs`; do not edit).
Frozen (never edit): `loom/src/quota/account_switch_contract_tests.rs`, `loom/src/quota/freshness_contract_tests.rs` (new, written before your work).

## Changes

### 1. `cache::clear_provider` (`cache.rs`)
`pub fn clear_provider(work_root: &Path, provider: &str) -> Result<()>`: `reject_symlink(&quota_dir(work_root))?;` then `let path = provider_path(work_root, provider); reject_symlink(&path)?;` then `std::fs::remove_file(&path)`, treating `ErrorKind::NotFound` as success and any other error as `Err` with context `format!("removing {}", path.display())`. Doc: removes a provider's cached reading; clearing an absent file is a no-op, so a machine without that provider stays silent.

### 2. E5 and CR-55(a): `project_account` (`poller.rs`)
Rewrite the body (keep the doc comment, and add that an account with no reading clears the project file, because whatever the file holds belongs to an account the user has replaced):
```text
let error = shared::display_error(account);
let written = match &account.quota {
    Some(quota) => project_quota(work_root, quota, error.clone()),
    None => cache::clear_provider(work_root, "claude").is_ok(),
};
match error { _ if !written => record_error("claude", state, "failed to write quota cache".to_string(), next_interval(state.interval, true)),
              Some(message) => record_error("claude", state, message, POLL_INTERVAL),
              None => record_success("claude", state) }
if written { state.interval = Duration::from_secs(account.interval_secs); state.next_due = Instant::now() + Duration::from_secs(wait) }   // wait as today
```
The schedule lines run only when `written`, so the backoff `record_error` sets for a failed write stands. The `None` arm no longer calls `cache::record_failure` (it re-stamped the previous account's windows with an error). `cache::record_failure` keeps its other callers (`poll_claude`'s `Err(e)` branch).
- `poll_claude`: in the two silent early returns (no home directory; `credentials::account(&home)` returns `Err`) call `let _ = cache::clear_provider(work_root, "claude");` before setting `state.next_due`. Leave the `Err(e)` branch of `shared::poll_shared` and `poll_codex` as they are.
- Visibility for the frozen contract in `quota/`: make `ProviderState` `pub(super) struct`, its `new` `pub(super) fn new() -> Self`, and `project_account` `pub(super) fn project_account(work_root: &Path, state: &mut ProviderState, account: &AccountState, now: i64)`. No other signature changes.

### 3. CR-55(b), (c): `age_secs` (`model.rs`)
Add `pub const FUTURE_SKEW_SECS: i64 = 300;` (doc: how far ahead of the reader a reading may be stamped before it stops counting as current) and change `age_secs`:
```text
if observed_at > now.saturating_add(FUTURE_SKEW_SECS) { return STALE_AFTER_SECS; }
now.saturating_sub(observed_at).max(0)
```
`i64::MIN` then gives `i64::MAX` (stale), a reading within the skew gives 0, a reading beyond it counts as exactly stale. Update the doc comment. Callers (`budget.rs`, `ledger/quota.rs`) keep their signatures; their `age >= STALE_AFTER_SECS` and `< STALE_AFTER_SECS` tests now treat a future reading as stale.

### 4. Dead `access_token` (`credentials.rs`)
`access_token` and its doc link are removed. That leaves `resolve_token`, `keychain_token` and `file_token` used only by tests, which `-D warnings` rejects, and `account` already re-implements the Keychain-then-file order. So fold them into one testable function:
- Delete `access_token`, `resolve_token`, `keychain_token`, `file_token`.
- Add `fn resolve_account(home: &Path, keychain: impl FnOnce() -> Option<String>) -> Result<(String, Option<String>)>` holding today's body of `account` with `keychain_raw()` replaced by `keychain()` (same `cfg!(target_os = "macos")` branch, same `filter(|raw| extract_access_token(raw).is_some())`, same `file_raw(home)` fallback, same `"no claude.ai login"` errors). `account(home)` becomes `resolve_account(home, keychain_raw)`.
- Keep `keychain_argv`, `keychain_raw`, `file_raw`, `extract_access_token`, `extract_plan` unchanged.

## Interfaces shared with other workers
None. The visibility changes in section 2 exist for the frozen contract file `quota/account_switch_contract_tests.rs`, which calls `super::poller::project_account` and `super::poller::ProviderState::new()`, and builds `super::shared::AccountState { quota, next_poll_at, interval_secs, consecutive_rate_limits, last_error }` by field.

## Tests (write them; the orchestrator runs them)
- `cache_tests.rs`: `clear_provider_removes_the_file_and_is_a_no_op_when_absent`; `clear_provider_refuses_a_symlinked_target_and_leaves_it_in_place` (pattern: `write_provider_refuses_a_symlinked_target_and_leaves_it_untouched` in the same file).
- `poller_tests.rs` (helpers `account_after(limits)`, `successful_quota`, `tempdir`, `ProviderState::new()`):
  - `a_second_account_without_a_reading_clears_the_first_accounts_quota`: `project_account(dir, &mut state, &account_after(0), 1_000)`, then `project_account(dir, &mut state, &AccountState { consecutive_rate_limits: 1, ..AccountState::default() }, 2_000)`; `cache::read_provider(dir, "claude")` is `None`. Fails today.
  - `a_failed_cache_write_keeps_its_backoff`: put a symlink at `cache::provider_path(dir, "claude")` as `finish_poll_does_not_append_history_when_the_snapshot_write_fails` does, call `project_account` with `&account_after(0)`; assert `state.interval == next_interval(initial_interval, true)` (initial is `ProviderState::new().interval`), not `account.interval_secs`. Fails today.
  - Existing tests `an_unconfirmed_429_keeps_the_last_reading_without_an_error`, `the_third_consecutive_429_writes_rate_limited_and_keeps_the_windows`, `projection_schedules_the_next_poll_from_the_account_state` stay unchanged and must still hold (their accounts have a reading, so they take the `Some` arm).
- `model_tests.rs`: `age_secs_treats_a_future_or_extreme_reading_as_stale`: `age_secs(i64::MIN, 1_000_000)` does not panic and is at least `STALE_AFTER_SECS`; `age_secs(1_010_000, 1_000_000) >= STALE_AFTER_SECS`; `age_secs(1_000_100, 1_000_000) == 0`; `age_secs(999_000, 1_000_000) == 1_000`.
- `credentials_tests.rs`: the shadowing helper becomes `fn access_token(home: &Path) -> Result<String> { super::resolve_account(home, || None).map(|(token, _)| token) }` and its doc comment is updated; the four tests that call `resolve_token` (`a_keychain_miss_falls_back_to_the_credentials_file`, `a_keychain_token_wins_over_the_credentials_file`, `the_keychain_is_not_consulted_off_macos`, `the_error_never_carries_a_token`) call `resolve_account(..).map(|(token, _)| token)` with the keychain closure returning raw credentials JSON, for example `Some(r#"{"claudeAiOauth":{"accessToken":"sk-keychain-token"}}"#.to_owned())`. Assertions are unchanged. Add `a_keychain_miss_still_reads_the_plan_from_the_file`: file with `"subscriptionType":"max"`, closure `|| None`, the tuple's second element is `Some("max")`.
- Contracts to turn green: `quota::account_switch_contract_tests::a_second_account_without_a_reading_clears_the_first_accounts_quota`, `quota::freshness_contract_tests::a_future_or_extreme_observed_at_is_not_fresh`.

## Proof
From `loom/`: `cargo check --all-targets`. Nothing else.

## Traps
- The validation section says to keep `resolve_token`; it would be dead code once `access_token` is gone. Follow section 4 above.
- `record_error` already sets the backoff interval and next-due time; the bug is only the unconditional lines after the `match`. Do not move the `match`.
- Do not call `clear_provider` from the `Some(quota)` arm or from `poll_claude`'s `Err(e)` branch: an account with a reading keeps its file, and a lock or I/O failure carries no account identity.
- `age_secs` returns `STALE_AFTER_SECS` (not `i64::MAX`) for a future reading so the TUI shows a plausible "10m old", not a number of days.
- Existing tests whose call shape changes: the `credentials_tests.rs` tests listed above (assertions stay).
