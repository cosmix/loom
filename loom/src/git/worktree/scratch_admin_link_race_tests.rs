//! Links planted after the private directory was built, the moments no check
//! ahead of git could cover: git writes the private file and publication follows
//! nothing.

use super::*;
use std::os::unix::fs::symlink;

/// A reflog link planted after the first look is exactly the race a check could
/// not close: git writes the private file, and the append afterwards refuses
/// the link. The command itself stands: the ref already moved.
#[test]
fn a_reflog_link_planted_after_the_look_takes_no_entry() {
    for extension in [true, false] {
        let fixture = Fixture::with_extension(extension);
        let scratch =
            ScratchAdmin::prepare(&fixture.repo, &fixture.admin, &fixture.common, extension)
                .unwrap();
        let reflog = fixture.plant("logs/HEAD");
        let args = ["commit", "-q", "--allow-empty", "-m", "racing"];
        let output = fixture.with_private_dir(scratch.path(), &args);
        assert!(output.status.success(), "{output:?}");

        scratch.publish().unwrap();

        assert!(reflog.untouched(), "the reflog entry followed the link");
    }
}

/// A message-file link planted after the copy: git writes the private file, and
/// publication sees a file it did not copy. It refuses and follows nothing; a
/// link at a name that was present is replaced by the same swap only if it is
/// not there at all, so nothing here depends on the link being spotted.
#[test]
fn a_message_link_planted_after_the_copy_is_never_written_through() {
    for extension in [true, false] {
        let fixture = Fixture::with_extension(extension);
        let scratch =
            ScratchAdmin::prepare(&fixture.repo, &fixture.admin, &fixture.common, extension)
                .unwrap();
        let message = fixture.plant("COMMIT_EDITMSG");
        let args = ["commit", "-q", "--allow-empty", "-m", "racing"];
        let output = fixture.with_private_dir(scratch.path(), &args);
        assert!(output.status.success(), "{output:?}");

        let error = scratch.publish().unwrap_err();

        assert!(
            format!("{error:#}").contains("changed while git"),
            "{error:#}"
        );
        assert!(message.untouched(), "the message followed the link");
    }
}

/// An index link planted after the copy looks like a concurrent write: the swap
/// refuses it and follows nothing.
#[test]
fn an_index_link_planted_after_the_copy_stops_publication() {
    let fixture = Fixture::with_extension(false);
    let scratch =
        ScratchAdmin::prepare(&fixture.repo, &fixture.admin, &fixture.common, false).unwrap();
    let victim = fixture.plant("index");
    std::fs::write(fixture.worktree.join("new.txt"), "new\n").unwrap();
    let output = fixture.with_private_dir(scratch.path(), &["add", "--", "new.txt"]);
    assert!(output.status.success(), "{output:?}");

    let error = scratch.publish().unwrap_err();

    assert!(
        format!("{error:#}").contains("changed while git"),
        "{error:#}"
    );
    assert!(victim.untouched());
    assert!(std::fs::symlink_metadata(&victim.link)
        .unwrap()
        .is_symlink());
}

/// A writer flipping the link for the whole of many pinned commits. The control
/// shows the same writer reaches git on the real directory.
#[test]
fn a_writer_flipping_a_link_never_reaches_a_pinned_commit() {
    let fixture = Fixture::with_extension(false);
    let victim = fixture._tmp.path().join("victim");
    std::fs::write(&victim, "orig\n").unwrap();
    let link = fixture.admin.join("COMMIT_EDITMSG");
    let stop = Arc::new(AtomicBool::new(false));
    let writer = {
        let (link, victim, stop) = (link.clone(), victim.clone(), stop.clone());
        std::thread::spawn(move || {
            let staged = link.with_extension("staged");
            while !stop.load(Ordering::Relaxed) {
                let _ = std::fs::remove_file(&staged);
                let _ = symlink(&victim, &staged);
                let _ = std::fs::rename(&staged, &link);
            }
        })
    };
    let args = ["commit", "-q", "--allow-empty", "-m", "racing-subject"];
    for _ in 0..30 {
        // Either outcome is correct; what must not happen is the write below.
        let _ = fixture.pinned().run(&args);
        assert_eq!(std::fs::read_to_string(&victim).unwrap(), "orig\n");
    }
    let reached = (0..200).any(|_| {
        fixture.on_real_dir(&args);
        std::fs::read_to_string(&victim)
            .unwrap()
            .contains("racing-subject")
    });
    stop.store(true, Ordering::Relaxed);
    writer.join().unwrap();
    assert!(reached, "the writer never reached git on the real path");
}
