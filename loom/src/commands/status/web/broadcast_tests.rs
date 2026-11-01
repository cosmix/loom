use super::*;
use crate::daemon::{write_message, CompletionSummary};
use std::io::Write;

#[test]
fn forwards_fragmented_status_before_eof_and_ignores_completion() {
    let temp = tempfile::tempdir().unwrap();
    let (mut sender, receiver) = UnixStream::pair().unwrap();
    receiver
        .set_read_timeout(Some(Duration::from_millis(20)))
        .unwrap();
    let broadcaster = Broadcaster::new();
    let updates = broadcaster.subscribe();
    let worker_broadcaster = broadcaster.clone();
    let work_path = temp.path().to_owned();
    let worker = thread::spawn(move || {
        forward_daemon(
            &worker_broadcaster,
            receiver,
            &work_path,
            &AtomicBool::new(true),
            false,
        )
    });

    send_fragmented_status(&mut sender);

    let frame = updates.recv_timeout(Duration::from_secs(3)).unwrap();
    let snapshot: WebSnapshot = serde_json::from_str(&frame).unwrap();
    assert_eq!(snapshot.source, SnapshotSource::Daemon);
    assert_eq!(
        snapshot.status.plan_name.as_deref(),
        Some("fragmented update")
    );

    drop(sender);
    assert!(matches!(
        worker.join().unwrap(),
        DaemonExit::Disconnected {
            received_frame: true
        }
    ));
    assert!(updates.try_recv().is_err());
}

fn send_fragmented_status(sender: &mut UnixStream) {
    write_message(
        sender,
        &Response::OrchestrationComplete {
            summary: CompletionSummary {
                total_duration_secs: 2,
                stages: Vec::new(),
                success_count: 0,
                failure_count: 0,
                plan_path: "plan.yaml".into(),
            },
        },
    )
    .unwrap();
    let mut bytes = Vec::new();
    let data = StatusData {
        plan_name: Some("fragmented update".into()),
        ..StatusData::default()
    };
    write_message(
        &mut bytes,
        &Response::StatusUpdate {
            data: Box::new(data),
        },
    )
    .unwrap();
    sender.write_all(&bytes[..2]).unwrap();
    thread::sleep(Duration::from_millis(80));
    sender.write_all(&bytes[2..6]).unwrap();
    thread::sleep(Duration::from_millis(80));
    sender.write_all(&bytes[6..]).unwrap();
}
