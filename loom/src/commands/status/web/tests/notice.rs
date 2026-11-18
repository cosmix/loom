//! A failed status collection reaches the browser as a notice on the last frame.

use std::time::Duration;

use super::{parse_full, workspace};
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
    let primer = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("replayed latest frame");
    assert_eq!(parse_full(&primer).notice, None);

    assert!(broadcast::republish_with_notice(
        &broadcaster,
        "Status collection failed"
    ));

    let frame = receiver
        .recv_timeout(Duration::from_secs(1))
        .expect("frame carrying the failure");
    let message: serde_json::Value = serde_json::from_str(&frame).expect("frame JSON");
    assert_eq!(message["t"], "state");
    assert_eq!(message["notice"], "Status collection failed");
    // `/api/status` serves the same notice.
    let bare: WebSnapshot =
        serde_json::from_str(&broadcaster.latest().expect("latest")).expect("snapshot JSON");
    assert_eq!(bare.notice.as_deref(), Some("Status collection failed"));
}
