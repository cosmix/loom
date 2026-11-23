//! The per-directory probe cache survives a probe that panics.

use super::*;
use std::panic::{catch_unwind, AssertUnwindSafe};

#[test]
fn a_panicking_probe_does_not_poison_the_cache() {
    static CACHE: PathProbeCache = OnceLock::new();
    let dir = Path::new("/probe-cache-test");

    let panicked = catch_unwind(AssertUnwindSafe(|| {
        cached_path_probe(&CACHE, dir, |_| panic!("probe failed"))
    }));
    assert!(panicked.is_err());

    assert!(cached_path_probe(&CACHE, dir, |_| true));
}
