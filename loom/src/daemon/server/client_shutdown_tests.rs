use super::*;
use crate::commands::stage::admin_proof::{mint_admin_proof, AdminProofRequest};
use crate::daemon::protocol::read_message;
use crate::daemon::server::pool::WorkerPool;
use crate::daemon::server::tokens::{admin_token_path, USER_TOKEN_FILE};
use std::io::Write;
use std::sync::mpsc;
use std::thread;
use std::time::Instant;
use tempfile::TempDir;

fn submit_client(pool: &WorkerPool, shutdown: &Arc<AtomicBool>, dir: &Path) -> UnixStream {
    let (server, client) = UnixStream::pair().unwrap();
    let shutdown = Arc::clone(shutdown);
    let dir = dir.to_path_buf();
    let (started, ready) = mpsc::channel();
    assert!(pool.try_execute(move || {
        started.send(()).unwrap();
        let _ = handle_client_connection(
            server,
            shutdown,
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(Vec::new())),
            &dir,
            ByteBudget::new(2 * crate::daemon::wire::MAX_REQUEST_BYTES),
        );
    }));
    ready.recv_timeout(Duration::from_secs(1)).unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap();
    client
}

fn ping(client: &mut UnixStream) -> bool {
    write_message(
        client,
        &Request::Ping {
            auth_token: "user-secret".into(),
        },
    )
    .is_ok()
        && matches!(read_message::<Response, _>(client), Ok(Response::Pong))
}

fn stop_daemon(stop: &mut UnixStream) {
    let proof = mint_admin_proof(
        "admin-secret",
        AdminProofRequest::daemon_stop(),
        "0123456789abcdef",
    );
    write_message(stop, &Request::Stop { auth_token: proof }).unwrap();
    assert!(matches!(
        read_message::<Response, _>(stop),
        Ok(Response::Ok)
    ));
}

#[test]
fn stop_cancels_queued_clients_and_bounds_active_reads() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join(USER_TOKEN_FILE), "user-secret").unwrap();
    std::fs::write(admin_token_path(dir.path()), "admin-secret").unwrap();
    let shutdown = Arc::new(AtomicBool::new(false));
    let pool = WorkerPool::new(3, 1);
    let mut active = submit_client(&pool, &shutdown, dir.path());
    for _ in 0..3 {
        assert!(ping(&mut active));
    }
    let mut partial = submit_client(&pool, &shutdown, dir.path());
    partial.write_all(&[0]).unwrap();
    let mut stop = submit_client(&pool, &shutdown, dir.path());
    let (queued_server, mut queued) = UnixStream::pair().unwrap();
    queued
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap();
    let queued_shutdown = Arc::clone(&shutdown);
    let queued_dir = dir.path().to_path_buf();
    assert!(pool.try_execute(move || {
        handle_client_connection(
            queued_server,
            queued_shutdown,
            Arc::new(Mutex::new(Vec::new())),
            Arc::new(Mutex::new(Vec::new())),
            &queued_dir,
            ByteBudget::new(1024),
        )
        .unwrap();
    }));
    stop_daemon(&mut stop);
    let start = Instant::now();
    while !shutdown.load(Ordering::SeqCst) {
        assert!(start.elapsed() < Duration::from_secs(1));
        thread::yield_now();
    }
    let joining = thread::spawn(move || pool.shutdown());
    assert!(!ping(&mut active));
    joining.join().unwrap();
    assert!(start.elapsed() < Duration::from_secs(7));
    assert!(!ping(&mut queued));
}
