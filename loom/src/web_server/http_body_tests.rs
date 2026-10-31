use super::*;
use std::io::Write;
use std::net::TcpListener;
use std::sync::Arc;
use std::thread;

fn pair() -> Option<(TcpStream, TcpStream)> {
    let listener = match TcpListener::bind("127.0.0.1:0") {
        Ok(listener) => listener,
        Err(error) if error.kind() == ErrorKind::PermissionDenied => {
            eprintln!("SKIP body read test: loopback bind denied: {error}");
            return None;
        }
        Err(error) => panic!("loopback bind failed: {error}"),
    };
    let client = TcpStream::connect(listener.local_addr().unwrap()).unwrap();
    let (server, _) = listener.accept().unwrap();
    Some((client, server))
}

#[test]
fn complete_body_and_shutdown_cancellation() {
    let Some((mut client, mut server)) = pair() else {
        return;
    };
    let running = Arc::new(AtomicBool::new(true));
    client.write_all(b"abc").unwrap();
    assert_eq!(read_body(&mut server, vec![], 3, &running).unwrap(), b"abc");

    let stop = Arc::clone(&running);
    let worker = thread::spawn(move || read_body(&mut server, vec![b'x'], 4, &stop));
    running.store(false, Ordering::SeqCst);
    assert!(worker.join().unwrap().is_err());
    let _ = client.write_all(b"yzq");
}

#[test]
fn trickled_bytes_do_not_extend_total_deadline() {
    let Some((mut client, mut server)) = pair() else {
        return;
    };
    let worker = thread::spawn(move || {
        for _ in 0..8 {
            if client.write_all(b"x").is_err() {
                break;
            }
            thread::sleep(Duration::from_millis(25));
        }
    });
    let result = read_body_until(
        &mut server,
        vec![],
        100,
        &AtomicBool::new(true),
        Instant::now() + Duration::from_millis(120),
    );
    assert!(result.unwrap_err().to_string().contains("timed out"));
    drop(server);
    worker.join().unwrap();
}
