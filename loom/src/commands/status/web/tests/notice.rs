//! A failed status collection reaches the browser as a notice on the last frame.

use std::time::Duration;

use super::workspace;
use crate::commands::status::web::broadcast;
use crate::commands::status::web::model::WebSnapshot;
use crate::fs::work_dir::WorkDir;

#[test]
fn republish_with_notice_reuses_the_latest_frame() {
    let (_temp, base) = workspace();
    let work_dir = WorkDir::new(&base).expect("build work dir");
    let broadcaster = broadcast::Broadcaster::new();
    assert!(
        !broadcast::republish_with_notice(&broadcaster, "no frame yet"),
        "nothing to carry the notice before the first frame"
    );
    broadcast::poll_files_once(&broadcaster, &work_dir, work_dir.root(), None, false)
        .expect("first file poll");
    let receiver = broadcaster.subscribe();
    receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("replayed latest frame");

    assert!(broadcast::republish_with_notice(
        &broadcaster,
        "Status collection failed"
    ));

    let frame = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("frame carrying the failure");
    let snapshot: WebSnapshot = serde_json::from_str(&frame).expect("snapshot JSON");
    assert_eq!(snapshot.notice.as_deref(), Some("Status collection failed"));
}
