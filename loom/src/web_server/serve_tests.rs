use super::*;

struct EmptyRouter;

impl Router for EmptyRouter {
    fn upgrade(&self, stream: TcpStream, _: &RequestHead, _: &ConnInfo<'_>) -> Option<TcpStream> {
        Some(stream)
    }

    fn handle(
        &self,
        _: &mut TcpStream,
        _: &RequestHead,
        _: Vec<u8>,
        _: &ConnInfo<'_>,
    ) -> io::Result<()> {
        Ok(())
    }
}

#[test]
fn zero_slots_still_waits_for_the_worker_resource_owner() {
    use crate::process::sandbox_probe::{loopback_bindable, skip_unless};

    if skip_unless(
        loopback_bindable(),
        "zero_slots_still_waits_for_the_worker_resource_owner",
        "loopback unavailable",
    ) {
        return;
    }
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let local = listener.local_addr().unwrap();
    let shared = Arc::new(Shared {
        listener,
        router: Arc::new(EmptyRouter),
        policy: AccessPolicy::new(local, None),
        limits: Limits::new(1024),
        running: Arc::new(AtomicBool::new(false)),
        thread_name: "resource-owner-test",
        completion: Completion(Arc::new(AtomicBool::new(false))),
    });
    // The exact teardown interleaving: handle has returned and dropped its
    // Slot, but the connection thread still owns its Shared clone.
    let worker = Arc::clone(&shared);
    assert_eq!(shared.limits.connection_count(), 0);
    let error = drain_until(&shared, local, Instant::now()).unwrap_err();
    let pending = error.downcast_ref::<ShutdownPending>().unwrap();
    assert!(!pending.is_complete());
    assert!(TcpListener::bind(local).is_err());
    drop(worker);
    drain_until(&shared, local, Instant::now()).unwrap();
    drop(shared);
    assert!(pending.is_complete());
    rebind_when_released(local).expect("success releases the address");
}

/// Rebind `local`, polling to a deadline on `AddrInUse`. Dropping the last
/// owner closes the listener, but a child another test thread is spawning in
/// that instant holds a copy of the descriptor between fork and exec (std
/// opens it `SOCK_CLOEXEC`, so exec drops it), keeping the address bound for
/// up to about a millisecond under load. A listener still owned in this
/// process stays `AddrInUse` past the deadline and fails the test.
fn rebind_when_released(local: SocketAddr) -> io::Result<TcpListener> {
    let deadline = Instant::now() + Duration::from_secs(1);
    loop {
        match TcpListener::bind(local) {
            Err(error) if error.kind() == io::ErrorKind::AddrInUse && Instant::now() < deadline => {
                thread::sleep(Duration::from_millis(10));
            }
            other => return other,
        }
    }
}
