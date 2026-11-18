//! `core.splitIndex` and the budget on what a private git directory copies. A
//! `sharedindex.<hash>` file is named after its content: git only freshens its
//! modification time, so a read must not rewrite it, and an `index` must never
//! reach the real directory ahead of the shared file it names.

use super::*;
use std::fs::File;
use std::os::unix::fs::MetadataExt;
use std::time::{Duration, SystemTime};

/// Force a new shared file on every index write.
const ALWAYS_SPLIT: [&str; 2] = ["-c", "splitIndex.maxPercentChange=0"];

fn shared_names(dir: &Path) -> Vec<OsString> {
    let mut names: Vec<OsString> = std::fs::read_dir(dir)
        .unwrap()
        .map(|entry| entry.unwrap().file_name())
        .filter(|name| name.to_string_lossy().starts_with("sharedindex."))
        .collect();
    names.sort();
    names
}

fn split_fixture() -> Fixture {
    let fixture = Fixture::new();
    ok(&fixture.worktree, &["update-index", "--split-index"]);
    assert_eq!(
        shared_names(&fixture.admin).len(),
        1,
        "the index is not split"
    );
    fixture
}

impl Fixture {
    /// Staged through the private directory of `scratch`, so its `index` names a
    /// shared file only that directory holds. Returns that file's name.
    fn stage_privately(&self, scratch: &ScratchAdmin) -> OsString {
        std::fs::write(self.worktree.join("new.txt"), "new\n").unwrap();
        let args = [ALWAYS_SPLIT[0], ALWAYS_SPLIT[1], "add", "--", "new.txt"];
        let output = self.with_private_dir(scratch.path(), &args);
        assert!(output.status.success(), "{output:?}");
        let real = shared_names(&self.admin);
        let created: Vec<OsString> = shared_names(scratch.path())
            .into_iter()
            .filter(|name| !real.contains(name))
            .collect();
        assert_eq!(created.len(), 1, "git made no new shared file");
        created.into_iter().next().unwrap()
    }

    /// Whether git can read the real index, the shared file it names included.
    fn real_index_reads(&self) -> bool {
        self.on_real_dir(&["status", "--porcelain"])
            .status
            .success()
    }
}

/// A file git would never write; only its name differs from what git writes.
fn hash_name(seed: usize) -> String {
    format!("sharedindex.{seed:040x}")
}

fn sparse(path: &Path, bytes: u64) {
    File::create(path).unwrap().set_len(bytes).unwrap();
}

/// Git freshens the shared file on every read. That is no change to publish.
#[test]
fn a_read_in_a_split_index_worktree_leaves_the_shared_file_alone() {
    let fixture = split_fixture();
    let name = shared_names(&fixture.admin).remove(0);
    let inode = std::fs::metadata(fixture.admin.join(&name)).unwrap().ino();

    let output = fixture
        .pinned()
        .run(&["status", "--porcelain"])
        .expect("a read-only command failed");

    assert!(output.status.success(), "{output:?}");
    let after = std::fs::metadata(fixture.admin.join(&name)).unwrap().ino();
    assert_eq!(after, inode, "the freshened copy replaced the real file");
    assert!(fixture.real_index_reads());
}

/// A session's git freshens the real file while the daemon's read runs.
#[test]
fn a_concurrent_freshen_of_the_shared_file_does_not_fail_a_read() {
    let fixture = split_fixture();
    let name = shared_names(&fixture.admin).remove(0);
    let scratch = fixture.scratch().with_optional_index_write(true);
    let output = fixture.status_with_git_dir(scratch.path());
    assert!(output.status.success(), "{output:?}");

    let real = File::options()
        .write(true)
        .open(fixture.admin.join(&name))
        .unwrap();
    real.set_modified(SystemTime::now() + Duration::from_secs(60))
        .unwrap();

    scratch.publish().expect("a read-only command failed");
}

/// The new shared file reaches the real directory with the index naming it.
#[test]
fn a_new_shared_file_is_published_with_the_index_that_names_it() {
    let fixture = split_fixture();
    let scratch = fixture.scratch();
    let name = fixture.stage_privately(&scratch);

    scratch.publish().unwrap();

    assert!(fixture.admin.join(&name).is_file());
    assert!(fixture.real_index_reads());
    let staged = ok(&fixture.worktree, &["diff", "--cached", "--name-only"]);
    assert_eq!(staged.trim(), "new.txt");
}

/// The shared file cannot be published: the index stays as it was, since an
/// index naming a missing file breaks every git in the worktree.
#[test]
fn an_index_never_reaches_the_real_directory_ahead_of_its_shared_file() {
    let fixture = split_fixture();
    let before = std::fs::read(fixture.admin.join(INDEX)).unwrap();
    let scratch = fixture.scratch();
    let name = fixture.stage_privately(&scratch);
    let lock = format!("{}.lock", name.to_string_lossy());
    std::fs::write(fixture.admin.join(&lock), "").unwrap();

    let error = scratch.publish().unwrap_err();

    assert!(format!("{error:#}").contains("cannot lock"), "{error:#}");
    std::fs::remove_file(fixture.admin.join(&lock)).unwrap();
    assert_eq!(std::fs::read(fixture.admin.join(INDEX)).unwrap(), before);
    assert!(fixture.real_index_reads(), "the index names a missing file");
}

/// The same failure in a command that only refreshes drops the refresh.
#[test]
fn a_refresh_that_cannot_publish_its_shared_file_drops_the_index_too() {
    let fixture = split_fixture();
    let before = std::fs::read(fixture.admin.join(INDEX)).unwrap();
    let scratch = fixture.scratch().with_optional_index_write(true);
    let name = fixture.stage_privately(&scratch);
    let lock = format!("{}.lock", name.to_string_lossy());
    std::fs::write(fixture.admin.join(&lock), "").unwrap();

    scratch.publish().expect("a read-only command failed");

    std::fs::remove_file(fixture.admin.join(&lock)).unwrap();
    assert_eq!(std::fs::read(fixture.admin.join(INDEX)).unwrap(), before);
    assert!(fixture.real_index_reads(), "the index names a missing file");
}

/// A concurrent git got there first. Same name, same content.
#[test]
fn a_shared_file_a_concurrent_git_already_wrote_is_no_conflict() {
    let fixture = split_fixture();
    let scratch = fixture.scratch();
    let name = fixture.stage_privately(&scratch);
    std::fs::copy(scratch.path().join(&name), fixture.admin.join(&name)).unwrap();

    scratch.publish().unwrap();

    assert!(fixture.real_index_reads());
}

#[test]
fn only_the_names_git_writes_are_copied() {
    let fixture = Fixture::with_extension(false);
    let shared = hash_name(1);
    let long_hash = format!("sharedindex.{:064x}", 2);
    for name in [
        shared.as_str(),
        long_hash.as_str(),
        "BISECT_LOG",
        "BISECT_TERMS",
    ] {
        std::fs::write(fixture.admin.join(name), "copied\n").unwrap();
    }
    let ignored = [
        "sharedindex.",
        "sharedindex.short",
        "sharedindex.0123456789ABCDEF0123456789ABCDEF01234567",
        "BISECT_",
        "BISECT_ANYTHING",
    ];
    for name in ignored {
        std::fs::write(fixture.admin.join(name), "ignored\n").unwrap();
    }

    let scratch =
        ScratchAdmin::prepare(&fixture.repo, &fixture.admin, &fixture.common, false).unwrap();

    for name in [
        shared.as_str(),
        long_hash.as_str(),
        "BISECT_LOG",
        "BISECT_TERMS",
    ] {
        assert!(scratch.path().join(name).is_file(), "{name} was not copied");
    }
    for name in ignored {
        assert!(!scratch.path().join(name).exists(), "{name} was copied");
    }
}

/// Over the count: refused before anything is copied or any git starts.
#[test]
fn too_many_generated_files_refuse_the_command() {
    let fixture = Fixture::with_extension(false);
    for seed in 0..300 {
        std::fs::write(fixture.admin.join(hash_name(seed)), "").unwrap();
    }
    let parent = fixture.repo.join(".loom/work").join(files::PRIVATE_PARENT);

    let error = fixture
        .pinned()
        .run(&["status", "--porcelain"])
        .unwrap_err();

    let message = format!("{error:#}");
    assert!(message.contains("will not run git"), "{message}");
    assert!(
        message.contains(&fixture.admin.display().to_string()),
        "{message}"
    );
    assert!(message.contains("files"), "{message}");
    assert_eq!(
        std::fs::read_dir(parent).unwrap().count(),
        0,
        "a copy stayed"
    );
}

/// Sparse files, so the test costs no disk while the old copy would write them
/// all out: over the total, refused on sizes alone, promptly and with no copy.
#[test]
fn sparse_files_over_the_total_refuse_the_command_without_a_copy() {
    let fixture = Fixture::with_extension(false);
    for seed in 0..5 {
        sparse(&fixture.admin.join(hash_name(seed)), 1 << 30);
    }
    let started = std::time::Instant::now();

    let error = match ScratchAdmin::prepare(&fixture.repo, &fixture.admin, &fixture.common, false) {
        Ok(_) => panic!("the copy went ahead"),
        Err(error) => error,
    };

    let message = format!("{error:#}");
    assert!(message.contains("will not run git"), "{message}");
    assert!(message.contains("bytes"), "{message}");
    assert!(
        started.elapsed() < Duration::from_secs(5),
        "it copied first"
    );
}

/// One file of the size a real index can reach stays allowed.
#[test]
fn a_sparse_file_within_the_total_is_copied() {
    let fixture = Fixture::with_extension(false);
    sparse(&fixture.admin.join(hash_name(1)), 1 << 20);

    let scratch =
        ScratchAdmin::prepare(&fixture.repo, &fixture.admin, &fixture.common, false).unwrap();

    let copy = std::fs::metadata(scratch.path().join(hash_name(1))).unwrap();
    assert_eq!(copy.len(), 1 << 20);
}
