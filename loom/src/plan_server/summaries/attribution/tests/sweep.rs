//! The recovery sweep and the terminal release: whose claims go, and that a claim file that is a
//! symlink is only unlinked.

use std::os::unix::fs::symlink;

use super::*;

#[test]
fn recovery_deletes_only_dead_sessions_claims_and_reports_terminal_ones() {
    let (tmp, paths) = runtime();
    claim(&paths, "plans/dead", Some("dead"));
    claim(&paths, "plans/live", Some("live"));
    claim(&paths, "plans/alone", Some("alone"));
    claim(&paths, "plans/term", None);
    // A claim file that is a symlink to a file outside the runtime directory. The target reads as
    // a dead session's claim, so a sweep that followed the link would delete it as swept.
    let outside = tmp.path().join("outside.json");
    let body = json!({"summary": "plans/link", "session": "dead", "pid": 1, "created_at": ""});
    std::fs::write(&outside, body.to_string()).expect("outside file");
    let link = paths.claims.join("plans__link.json");
    symlink(&outside, &link).expect("symlinked claim");
    let mut attribution = Attribution::new(paths.clone());

    let terminal = attribution
        .sweep_after_recovery(&["live"], &["alone"])
        .expect("sweep");

    assert_eq!(terminal, ["plans/link", "plans/term"]);
    assert_eq!(
        claimed(&paths),
        ["plans/alone", "plans/link", "plans/live", "plans/term"],
        "only the dead session's claim is deleted"
    );
    assert!(link.symlink_metadata().is_ok(), "the sweep keeps the link");

    let released = attribution
        .release_terminal(&owned(&["plans/link"]))
        .expect("release");

    assert_eq!(released, ["plans/link"]);
    assert!(link.symlink_metadata().is_err(), "the link is removed");
    let kept = std::fs::read_to_string(&outside).expect("the target stays");
    assert_eq!(kept, body.to_string(), "only the link was removed");
    assert_eq!(claimed(&paths), ["plans/alone", "plans/live", "plans/term"]);
}
