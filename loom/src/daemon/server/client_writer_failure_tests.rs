use super::*;
use crate::daemon::protocol::{read_message, write_message};
use crate::daemon::server::broadcast::broadcast_to_subscribers;
use crate::daemon::server::tokens::USER_TOKEN_FILE;
use std::io;
use std::thread;
use tempfile::TempDir;

type Subscribers = Arc<Mutex<Vec<Subscriber>>>;

fn subscribed_connection() -> (
    TempDir,
    UnixStream,
    Subscribers,
    Subscribers,
    thread::JoinHandle<Result<()>>,
) {
    let dir = TempDir::new().unwrap();
    std::fs::write(dir.path().join(USER_TOKEN_FILE), "user-secret").unwrap();
    let (server, mut client) = UnixStream::pair().unwrap();
    client
        .set_read_timeout(Some(Duration::from_secs(2)))
        .unwrap();
    let status = Arc::new(Mutex::new(Vec::new()));
    let logs = Arc::new(Mutex::new(Vec::new()));
    let handler = {
        let status = Arc::clone(&status);
        let logs = Arc::clone(&logs);
        let dir = dir.path().to_path_buf();
        thread::spawn(move || {
            handle_client_connection(
                server,
                Arc::new(AtomicBool::new(false)),
                status,
                logs,
                &dir,
                ByteBudget::new(2 * crate::daemon::wire::MAX_REQUEST_BYTES),
            )
        })
    };
    for request in [
        Request::SubscribeStatus {
            auth_token: "user-secret".into(),
        },
        Request::SubscribeLogs {
            auth_token: "user-secret".into(),
        },
    ] {
        write_message(&mut client, &request).unwrap();
        assert!(matches!(
            read_message::<Response, _>(&mut client),
            Ok(Response::Ok)
        ));
    }
    (dir, client, status, logs, handler)
}

#[test]
fn failed_frame_closes_shared_writer_for_both_channels_and_ping() {
    let (_dir, mut client, status, logs, handler) = subscribed_connection();
    let writer = Arc::clone(&status.lock().unwrap()[0]);
    assert!(Arc::ptr_eq(&writer, &logs.lock().unwrap()[0]));
    let failure = write_bounded_frame_with_timeout(
        &mut writer.lock().unwrap(),
        &vec![b'x'; 1024 * 1024],
        Duration::from_millis(50),
    );
    assert_eq!(failure.unwrap_err().kind(), io::ErrorKind::TimedOut);

    write_message(
        &mut client,
        &Request::Ping {
            auth_token: "user-secret".into(),
        },
    )
    .unwrap();
    assert!(handler.join().unwrap().is_err());
    broadcast_to_subscribers(&status, &Response::Pong, &mut false);
    broadcast_to_subscribers(&logs, &Response::Pong, &mut false);
    assert!(status.lock().unwrap().is_empty());
    assert!(logs.lock().unwrap().is_empty());
}
