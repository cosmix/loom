//! Contract: when the shared account changes and the new account has no
//! reading yet, the project's `claude.json` must not keep the previous
//! account's windows, with or without an error stamped on them.
//!
//! Seam: `poller::project_account` and `poller::ProviderState::new` are
//! visible to the `quota` module.

use tempfile::tempdir;

use super::cache::read_provider;
use super::model::{ProviderQuota, QuotaWindow, WindowKind};
use super::poller::{project_account, ProviderState};
use super::shared::AccountState;

fn first_accounts_reading() -> AccountState {
    AccountState {
        quota: Some(ProviderQuota {
            observed_at: 1_000,
            windows: vec![QuotaWindow {
                kind: WindowKind::FiveHour,
                used_percent: 42.0,
                resets_at: Some(2_000),
            }],
            plan: None,
            error: None,
        }),
        ..AccountState::default()
    }
}

#[test]
fn a_second_account_without_a_reading_clears_the_first_accounts_quota() {
    let dir = tempdir().unwrap();
    let mut state = ProviderState::new();

    project_account(dir.path(), &mut state, &first_accounts_reading(), 1_000);
    assert!(
        read_provider(dir.path(), "claude").is_some(),
        "the first account's reading was not published"
    );

    let second_account = AccountState {
        consecutive_rate_limits: 1,
        ..AccountState::default()
    };
    project_account(dir.path(), &mut state, &second_account, 2_000);

    let left = read_provider(dir.path(), "claude");
    assert!(
        left.is_none(),
        "the previous account's quota is still published: {left:?}"
    );
}
