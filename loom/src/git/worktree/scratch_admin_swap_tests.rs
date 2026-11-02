//! Publication as a compare and swap under git's own locks, and what a private
//! directory leaves behind.

use super::*;

/// A `HEAD` publication takes `HEAD.lock` as git does. The old code ignored the
/// lock, so it replaced `HEAD` under a concurrent git. The second half is a real
/// git holding off the update: the lock this code takes stops it.
#[test]
fn publishing_head_is_a_compare_and_swap_under_git_s_own_lock() {
    let fixture = Fixture::with_extension(false);
    let scratch = fixture.scratch();
    let original = std::fs::read(fixture.admin.join(HEAD)).unwrap();
    std::fs::write(scratch.path().join(HEAD), "ref: refs/heads/main\n").unwrap();
    let lock = fixture.admin.join("HEAD.lock");
    std::fs::write(&lock, "").unwrap();

    let error = scratch.publish().unwrap_err();

    assert!(format!("{error:#}").contains("cannot lock"), "{error:#}");
    assert_eq!(std::fs::read(fixture.admin.join(HEAD)).unwrap(), original);
    assert!(
        lock.exists(),
        "a lock this process does not own was removed"
    );
    std::fs::remove_file(&lock).unwrap();

    let refused = fixture.scratch();
    std::fs::write(refused.path().join(HEAD), "ref: refs/heads/main\n").unwrap();
    std::fs::write(fixture.admin.join(HEAD), "ref: refs/heads/other\n").unwrap();
    assert!(refused.publish().is_err());
    assert!(!lock.exists(), "a failed swap left its lock behind");

    std::fs::write(fixture.admin.join(HEAD), &original).unwrap();
    let published = fixture.scratch();
    std::fs::write(published.path().join(HEAD), "ref: refs/heads/main\n").unwrap();
    published.publish().unwrap();
    assert_eq!(
        std::fs::read_to_string(fixture.admin.join(HEAD)).unwrap(),
        "ref: refs/heads/main\n"
    );
    assert!(!lock.exists());
}

/// A status that lost a race for the index keeps what the concurrent git wrote
/// and succeeds; a command whose index write is the point fails instead.
#[test]
fn a_lost_race_drops_a_refresh_and_fails_a_real_index_write() {
    for (optional, expect_ok) in [(true, true), (false, false)] {
        let fixture = Fixture::with_extension(false);
        let scratch = fixture.scratch().with_optional_index_write(optional);
        std::fs::write(scratch.path().join(INDEX), "private\n").unwrap();
        std::fs::write(fixture.admin.join(INDEX), "concurrent\n").unwrap();

        assert_eq!(scratch.publish().is_ok(), expect_ok, "optional {optional}");

        let kept = std::fs::read_to_string(fixture.admin.join(INDEX)).unwrap();
        assert_eq!(kept, "concurrent\n");
    }
}

/// A `git add` running while a host command's copy is out sees its own index
/// kept: the host command changed nothing, so nothing is published.
#[test]
fn a_command_that_changed_nothing_publishes_nothing() {
    let fixture = Fixture::with_extension(false);
    let scratch = fixture.scratch();
    std::fs::write(fixture.admin.join(INDEX), "concurrent\n").unwrap();
    std::fs::write(fixture.admin.join(HEAD), "ref: refs/heads/main\n").unwrap();

    scratch.publish().unwrap();

    assert_eq!(
        std::fs::read_to_string(fixture.admin.join(INDEX)).unwrap(),
        "concurrent\n"
    );
    assert_eq!(
        std::fs::read_to_string(fixture.admin.join(HEAD)).unwrap(),
        "ref: refs/heads/main\n"
    );
}

/// Git compares each entry's time with the index file's own to spot racy
/// entries, so the copy must keep the time or an edit in the same tick reads as
/// clean.
#[test]
fn the_index_copy_keeps_its_modification_time() {
    let fixture = Fixture::with_extension(false);
    let real = std::fs::metadata(fixture.admin.join(INDEX))
        .unwrap()
        .modified()
        .unwrap();

    let scratch = fixture.scratch();

    let copy = std::fs::metadata(scratch.path().join(INDEX))
        .unwrap()
        .modified()
        .unwrap();
    assert_eq!(real, copy);
}

#[test]
fn leftovers_of_a_killed_process_are_reaped_and_live_directories_kept() {
    let fixture = Fixture::with_extension(false);
    let parent = fixture
        .repo
        .join(".loom/work")
        .canonicalize()
        .unwrap()
        .join(PRIVATE_PARENT);
    std::fs::create_dir_all(&parent).unwrap();
    let (old, live) = (parent.join(".tmpold"), parent.join(".tmplive"));
    std::fs::create_dir(&old).unwrap();
    std::fs::create_dir(&live).unwrap();
    std::fs::write(old.join("index"), "left behind\n").unwrap();
    let long_ago = std::time::SystemTime::now() - Duration::from_secs(3 * 24 * 60 * 60);
    std::fs::File::open(&old)
        .unwrap()
        .set_modified(long_ago)
        .unwrap();

    fixture
        .pinned()
        .run_checked(&["rev-parse", "HEAD"])
        .unwrap();

    assert!(!old.exists(), "a leftover survived");
    assert!(live.exists(), "a directory in use was removed");
}
