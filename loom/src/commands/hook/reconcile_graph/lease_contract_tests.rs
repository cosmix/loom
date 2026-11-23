//! Contract: the reconcile lease is keyed per checkout, so two checkouts that
//! share one context store never queue behind each other's holder.

use tempfile::TempDir;

use super::{reconcile_lock_path, try_claim, Claim};
use crate::context::store::ContextStore;

#[test]
fn two_checkouts_take_independent_leases() {
    let cache = TempDir::new().unwrap();
    let store = ContextStore::with_root(cache.path().join("cache"));
    std::fs::create_dir_all(store.root()).unwrap();
    let checkout_a = TempDir::new().unwrap();
    let checkout_b = TempDir::new().unwrap();

    let lock_a = reconcile_lock_path(&store, checkout_a.path());
    let lock_b = reconcile_lock_path(&store, checkout_b.path());
    assert_ne!(lock_a, lock_b, "each checkout gets its own lease file");
    assert_eq!(lock_a.parent(), Some(store.root()));
    assert_eq!(lock_b.parent(), Some(store.root()));

    assert_eq!(
        try_claim(&lock_a, 1_000, 600, 1_800, |_| true, 4_242),
        Claim::Won
    );
    assert_eq!(
        try_claim(&lock_b, 1_001, 600, 1_800, |_| true, 4_343),
        Claim::Won,
        "checkout B must not queue behind checkout A's live holder"
    );
}
