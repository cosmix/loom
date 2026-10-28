//! Authoring leases: a counter of held leases that tells the bridge when an authoring chat is mid-write.

use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

/// Counts active holds that keep authoring from being released when a chat ends.
#[derive(Default)]
pub struct AuthoringLeases {
    count: Arc<AtomicUsize>,
}

impl AuthoringLeases {
    /// Hold authoring until the returned lease is dropped.
    pub fn hold(&self) -> AuthoringLease {
        self.count.fetch_add(1, Ordering::SeqCst);
        AuthoringLease {
            count: Arc::clone(&self.count),
        }
    }

    /// Whether at least one lease is held.
    pub fn held(&self) -> bool {
        self.count.load(Ordering::SeqCst) != 0
    }
}

/// A hold on authoring, released when dropped.
pub struct AuthoringLease {
    count: Arc<AtomicUsize>,
}

impl Drop for AuthoringLease {
    fn drop(&mut self) {
        self.count.fetch_sub(1, Ordering::SeqCst);
    }
}

#[cfg(test)]
mod tests {
    use super::AuthoringLeases;
    use std::sync::Arc;

    #[test]
    fn holds_last_until_every_lease_is_dropped() {
        let leases = AuthoringLeases::default();
        assert!(!leases.held());
        let first = leases.hold();
        assert!(leases.held());
        let second = leases.hold();
        drop(first);
        assert!(leases.held());
        drop(second);
        assert!(!leases.held());
    }

    #[test]
    fn observes_a_hold_created_on_another_thread() {
        let leases = Arc::new(AuthoringLeases::default());
        let other = Arc::clone(&leases);
        let lease = std::thread::spawn(move || other.hold()).join().unwrap();
        assert!(leases.held());
        drop(lease);
        assert!(!leases.held());
    }
}
