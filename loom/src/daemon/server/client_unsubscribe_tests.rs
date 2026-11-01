use super::*;
use crate::daemon::protocol::{read_message, write_message};
use crate::daemon::server::broadcast::broadcast_to_subscribers;
use crate::daemon::server::tokens::USER_TOKEN_FILE;
use std::io::Read;
use std::thread::{self, JoinHandle};
use tempfile::TempDir;

type Subscribers = Arc<Mutex<Vec<Subscriber>>>;

fn spawn_client(
    dir: &Path,
    status: &Subscribers,
    logs: &Subscribers,
    shutdown: &Arc<AtomicBool>,
) -> (UnixStream, JoinHandle<Result<()>>) {
    let (server, client) = UnixStream::pair().unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(7)))
        .unwrap();
    let dir = dir.to_path_buf();
    let status = Arc::clone(status);
    let logs = Arc::clone(logs);
    let shutdown = Arc::clone(shutdown);
    let handler = thread::spawn(move || {
        handle_client_connection(
            server,
            shutdown,
            status,
            logs,
            &dir,
            ByteBudget::new(2 * crate::daemon::wire::MAX_REQUEST_BYTES),
        )
    });
    (client, handler)
}

fn send_expect_ok(client: &mut UnixStream, request: Request) {
    write_message(client, &request).unwrap();
    assert!(matches!(
        read_message::<Response, _>(client).unwrap(),
        Response::Ok
    ));
}

fn subscribe_both(client: &mut UnixStream) {
    send_expect_ok(
        client,
        Request::SubscribeStatus {
            auth_token: "user-secret".into(),
        },
    );
    send_expect_ok(
        client,
        Request::SubscribeLogs {
            auth_token: "user-secret".into(),
        },
    );
}

#[test]
fn unsubscribe_removes_only_its_connection_from_both_streams() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join(USER_TOKEN_FILE), "user-secret").unwrap();
    let status = Arc::new(Mutex::new(Vec::new()));
    let logs = Arc::new(Mutex::new(Vec::new()));
    let shutdown = Arc::new(AtomicBool::new(false));
    let (mut first, first_handler) = spawn_client(dir.path(), &status, &logs, &shutdown);
    subscribe_both(&mut first);

    let (mut second, second_handler) = spawn_client(dir.path(), &status, &logs, &shutdown);
    subscribe_both(&mut second);
    let second_writer = Arc::clone(&status.lock().unwrap()[1]);
    assert!(Arc::ptr_eq(&second_writer, &logs.lock().unwrap()[1]));

    send_expect_ok(
        &mut first,
        Request::Unsubscribe {
            auth_token: "user-secret".into(),
        },
    );
    assert_eq!(status.lock().unwrap().len(), 1);
    assert_eq!(logs.lock().unwrap().len(), 1);
    assert!(Arc::ptr_eq(&second_writer, &status.lock().unwrap()[0]));
    assert!(Arc::ptr_eq(&second_writer, &logs.lock().unwrap()[0]));
    assert_eq!(first.read(&mut [0u8; 1]).unwrap(), 0);

    broadcast_to_subscribers(&status, &Response::Pong, &mut false);
    broadcast_to_subscribers(
        &logs,
        &Response::LogLine {
            line: "ready".into(),
        },
        &mut false,
    );
    assert!(matches!(
        read_message::<Response, _>(&mut second).unwrap(),
        Response::Pong
    ));
    assert!(
        matches!(read_message::<Response, _>(&mut second).unwrap(), Response::LogLine { line } if line == "ready")
    );

    drop(first);
    drop(second);
    first_handler.join().unwrap().unwrap();
    second_handler.join().unwrap().unwrap();
}

#[test]
fn stale_broadcast_snapshots_cannot_follow_unsubscribe_acknowledgement() {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join(USER_TOKEN_FILE), "user-secret").unwrap();
    let status = Arc::new(Mutex::new(Vec::new()));
    let logs = Arc::new(Mutex::new(Vec::new()));
    let shutdown = Arc::new(AtomicBool::new(false));
    let (mut client, handler) = spawn_client(dir.path(), &status, &logs, &shutdown);
    subscribe_both(&mut client);
    let stale_status = Arc::new(Mutex::new(status.lock().unwrap().clone()));
    let stale_logs = Arc::new(Mutex::new(logs.lock().unwrap().clone()));

    send_expect_ok(
        &mut client,
        Request::Unsubscribe {
            auth_token: "user-secret".into(),
        },
    );
    broadcast_to_subscribers(&stale_status, &Response::Pong, &mut false);
    broadcast_to_subscribers(
        &stale_logs,
        &Response::LogLine {
            line: "late".into(),
        },
        &mut false,
    );

    assert!(stale_status.lock().unwrap().is_empty());
    assert!(stale_logs.lock().unwrap().is_empty());
    assert_eq!(client.read(&mut [0u8; 1]).unwrap(), 0);
    handler.join().unwrap().unwrap();
}
