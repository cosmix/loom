use std::os::unix::fs::PermissionsExt;
use std::path::Path;

use super::super::{outside, CargoScratch};
use super::marker::{self, NAME};
use super::owner::{prefix, sweep, Owner};
use super::removable::listed;
use super::{create, make_removable, remove, sweepable, PREFIX};

fn mode(path: &Path, mode: u32) {
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// A tree a command could leave: a 0500 directory holding a file, a 000
/// directory, and a symlink to a directory outside that must stay as it is.
fn locked_tree(root: &Path, outside: &Path) {
    let locked = root.join("pkg/mod");
    std::fs::create_dir_all(&locked).unwrap();
    std::fs::write(locked.join("file"), "x").unwrap();
    std::fs::create_dir(root.join("work")).unwrap();
    std::os::unix::fs::symlink(outside, root.join("link")).unwrap();
    mode(&locked, 0o500);
    mode(&root.join("pkg"), 0o500);
    mode(&root.join("work"), 0o000);
}

/// Dropping a scratch removes it even when a command left directories its
/// owner cannot write.
#[test]
fn scratch_with_read_only_directories_is_removed() {
    let outside = tempfile::tempdir().unwrap();
    mode(outside.path(), 0o500);
    let tree = tempfile::tempdir().unwrap();
    let mut command = std::process::Command::new("true");
    command.env_clear();
    let scratch = CargoScratch::prepare_private(&command, tree.path()).unwrap();
    locked_tree(scratch.path(), outside.path());
    let path = scratch.path().to_path_buf();
    drop(scratch);
    assert!(!path.exists(), "{} survived", path.display());
    let kept = outside.path().metadata().unwrap().permissions().mode();
    assert_eq!(kept & 0o777, 0o500, "a symlinked directory was changed");
    mode(outside.path(), 0o700);
}

#[test]
fn scratch_names_carry_their_owner() {
    let scratch = create(std::iter::once(std::env::temp_dir())).unwrap();
    let name = scratch.path().file_name().unwrap().to_str().unwrap();
    assert!(name.starts_with(prefix()), "{name}");
    assert_eq!(Owner::of(name), Owner::current(), "{name}");
    assert_eq!(Owner::current().unwrap().pid, std::process::id());
    assert_eq!(Owner::of(&format!("{PREFIX}abc123")), None);
    assert_eq!(Owner::of(&format!("{PREFIX}9-12-34-")), None);
    let parsed = Owner::of(&format!("{PREFIX}9-12-34-x"));
    let expected = Owner {
        namespace: 9,
        pid: 12,
        start: 34,
    };
    assert_eq!(parsed, Some(expected));
}

/// A live owner's scratch stays; a gone owner's (its pid reused under another
/// start time) goes, read-only directories and all; an unowned name, one from
/// another pid namespace, a symlink and a plain file under an orphan's name
/// are left alone.
#[test]
fn sweep_reclaims_only_orphans() {
    let root = tempfile::tempdir().unwrap();
    let outside = tempfile::tempdir().unwrap();
    std::fs::write(outside.path().join("keep"), "x").unwrap();
    let own = Owner::current().unwrap();
    let name = |namespace: u64, start: u64, tag: &str| {
        let pid = own.pid;
        root.path()
            .join(format!("{PREFIX}{namespace}-{pid}-{start}-{tag}"))
    };
    let live = name(own.namespace, own.start, "live");
    let orphan = name(own.namespace, own.start + 1, "orphan");
    let foreign = name(own.namespace + 1, own.start + 1, "foreign");
    let unowned = root.path().join(format!("{PREFIX}legacy"));
    let link = name(own.namespace, own.start + 1, "link");
    let file = name(own.namespace, own.start + 1, "file");
    for dir in [&live, &orphan, &foreign, &unowned] {
        std::fs::create_dir(dir).unwrap();
    }
    locked_tree(&orphan, outside.path());
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    std::fs::write(&file, "x").unwrap();
    sweep(root.path());
    assert!(!orphan.exists(), "the orphan survived");
    assert!(live.is_dir() && foreign.is_dir() && unowned.is_dir() && file.is_file());
    assert!(link.is_symlink() && outside.path().join("keep").is_file());
}

fn mode_of(path: &Path) -> u32 {
    std::fs::symlink_metadata(path)
        .unwrap()
        .permissions()
        .mode()
        & 0o7777
}

/// A link to a directory outside, at the root or below it, is never entered:
/// a locked directory reached through it keeps its mode.
#[test]
fn removal_never_follows_a_symlink() {
    let outside = tempfile::tempdir().unwrap();
    let locked = outside.path().join("b");
    std::fs::create_dir_all(locked.join("c")).unwrap();
    mode(&locked.join("c"), 0o500);
    mode(&locked, 0o500);
    let tree = tempfile::tempdir().unwrap();
    let mut command = std::process::Command::new("true");
    command.env_clear();
    let scratch = CargoScratch::prepare_private(&command, tree.path()).unwrap();
    std::fs::create_dir(scratch.path().join("cargo")).unwrap();
    std::os::unix::fs::symlink(outside.path(), scratch.path().join("a")).unwrap();
    std::os::unix::fs::symlink(outside.path(), scratch.path().join("cargo/registry")).unwrap();
    mode(&scratch.path().join("cargo"), 0o500);
    let path = scratch.path().to_path_buf();
    drop(scratch);
    assert!(!path.exists(), "{} survived", path.display());
    let link = tree.path().join("root-link");
    std::os::unix::fs::symlink(outside.path(), &link).unwrap();
    make_removable(&link);
    assert_eq!(mode_of(&locked), 0o500, "a directory behind a link changed");
    assert_eq!(mode_of(&locked.join("c")), 0o500);
    assert!(locked.join("c").is_dir(), "the outside tree was touched");
    mode(&locked, 0o700);
    mode(&locked.join("c"), 0o700);
}

/// A directory swapped for a link to an outside 0500 tree after its parent was
/// listed, as a live process could, is never entered: the walk opens each
/// child on its parent's descriptor without following a link.
#[test]
fn a_directory_swapped_for_a_link_mid_walk_is_never_entered() {
    let outside = tempfile::tempdir().unwrap();
    let locked = outside.path().join("b");
    std::fs::create_dir(&locked).unwrap();
    mode(&locked, 0o500);
    mode(outside.path(), 0o500);
    let scratch = tempfile::tempdir().unwrap();
    let swapped = scratch.path().join("a");
    std::fs::create_dir(&swapped).unwrap();
    let (root, target) = (scratch.path().to_path_buf(), outside.path().to_path_buf());
    let swap = move |listed: &Path| {
        if listed == root && swapped.is_dir() {
            std::fs::remove_dir(&swapped).unwrap();
            std::os::unix::fs::symlink(&target, &swapped).unwrap();
        }
    };
    listed::with(swap, || make_removable(scratch.path()));
    assert!(
        scratch.path().join("a").is_symlink(),
        "the swap did not run"
    );
    let (outside_mode, locked_mode) = (mode_of(outside.path()), mode_of(&locked));
    mode(outside.path(), 0o700);
    mode(&locked, 0o700);
    assert_eq!(outside_mode, 0o500, "the walk entered a swapped-in link");
    assert_eq!(locked_mode, 0o500, "the walk entered a swapped-in link");
}

/// A 000 directory below a 0500 one, as a tool's cache may nest them, is
/// restored and removed with the rest.
#[test]
fn a_locked_directory_below_a_read_only_one_is_removed() {
    let root = tempfile::tempdir().unwrap();
    let scratch = root.path().join(format!("{PREFIX}nested"));
    let (outer, inner) = (scratch.join("x"), scratch.join("x/y"));
    std::fs::create_dir_all(&inner).unwrap();
    std::fs::write(inner.join("file"), "x").unwrap();
    mode(&inner, 0o000);
    mode(&outer, 0o500);
    remove(&scratch).unwrap();
    assert!(!scratch.exists(), "{} survived", scratch.display());
}

/// A scratch already removed, by another process's sweep, is no error.
#[test]
fn removing_a_missing_scratch_succeeds() {
    let root = tempfile::tempdir().unwrap();
    remove(&root.path().join(format!("{PREFIX}gone"))).unwrap();
}

/// Whether `sweepable` keeps `/tmp`: a directory that is root-owned and sticky, or, as in
/// a host job where `/tmp` is this user's own tmpfs, this user's with no group or other
/// write bit.
fn tmp_is_swept(tmp: &Path) -> bool {
    let metadata = tmp.symlink_metadata().unwrap();
    let owner = std::os::unix::fs::MetadataExt::uid(&metadata);
    let root_sticky = owner == 0 && mode_of(tmp) & 0o1000 != 0;
    let own_private = owner == crate::models::forward_receipt::locator::effective_uid()
        && mode_of(tmp) & 0o022 == 0;
    metadata.is_dir() && (root_sticky || own_private)
}

/// Only roots outside the tree, root-owned and sticky or this user's own
/// directory no one else can write (0700 or 0755), are swept: a TMPDIR inside
/// the worktree or one a group can write never is. `/tmp` follows the same rule,
/// whoever owns it.
#[test]
fn sweep_roots_exclude_the_tree_and_shared_directories() {
    let tree = tempfile::tempdir().unwrap();
    let inside = tree.path().join("tmp");
    std::fs::create_dir(&inside).unwrap();
    mode(&inside, 0o700);
    let private = tempfile::tempdir().unwrap();
    mode(private.path(), 0o700);
    let readable = tempfile::tempdir().unwrap();
    mode(readable.path(), 0o755);
    let group = tempfile::tempdir().unwrap();
    mode(group.path(), 0o775);
    let shared = tempfile::tempdir().unwrap();
    mode(shared.path(), 0o777);
    let sticky = Path::new("/tmp").to_path_buf();
    let candidates = [
        &inside,
        private.path(),
        readable.path(),
        group.path(),
        shared.path(),
        &sticky,
    ];
    let roots = sweepable(outside(
        tree.path(),
        candidates.map(Path::to_path_buf).into_iter(),
    ));
    assert!(!roots.contains(&inside), "a root inside the tree is swept");
    assert!(!roots.contains(&shared.path().to_path_buf()), "{roots:?}");
    assert!(!roots.contains(&group.path().to_path_buf()), "{roots:?}");
    assert!(roots.contains(&private.path().to_path_buf()), "{roots:?}");
    assert!(roots.contains(&readable.path().to_path_buf()), "{roots:?}");
    assert_eq!(roots.contains(&sticky), tmp_is_swept(&sticky), "{roots:?}");
    let own = Owner::current().unwrap();
    let (namespace, pid, start) = (own.namespace, own.pid, own.start + 1);
    let orphan = readable
        .path()
        .join(format!("{PREFIX}{namespace}-{pid}-{start}-orphan"));
    std::fs::create_dir(&orphan).unwrap();
    sweep(readable.path());
    assert!(!orphan.exists(), "an orphan under a 0755 root survived");
}

/// An orphan marked for a boundary stays while that namespace may have
/// members (here this test's own namespace, or an unknown one) in this boot,
/// and goes once the namespace is empty or the host rebooted.
#[test]
fn sweep_keeps_a_scratch_while_its_boundary_may_live() {
    let root = tempfile::tempdir().unwrap();
    let own = Owner::current().unwrap();
    let orphan = |tag: &str| {
        let (namespace, pid, start) = (own.namespace, own.pid, own.start + 1);
        let path = root
            .path()
            .join(format!("{PREFIX}{namespace}-{pid}-{start}-{tag}"));
        std::fs::create_dir(&path).unwrap();
        path
    };
    let live = orphan("live");
    marker::write(&live, Some(own.namespace)).unwrap();
    let unknown = orphan("unknown");
    marker::write(&unknown, None).unwrap();
    let emptied = orphan("emptied");
    marker::write(&emptied, Some(1)).unwrap();
    let rebooted = orphan("rebooted");
    std::fs::write(
        rebooted.join(NAME),
        format!("not-this-boot {}\n", own.namespace),
    )
    .unwrap();
    let fifo = orphan("fifo");
    nix::unistd::mkfifo(&fifo.join(NAME), nix::sys::stat::Mode::S_IRWXU).unwrap();
    sweep(root.path());
    assert!(live.is_dir() && unknown.is_dir() && fifo.is_dir());
    assert!(!emptied.exists() && !rebooted.exists());
}

/// A marker that cannot be read or parsed keeps its scratch only while the
/// scratch last changed in this boot. One naming this boot, with its namespace
/// live, keeps it even when the boot time reads later than its last change (a
/// wall clock stepped forward): the boot id decides, never the age.
#[test]
fn only_an_unreadable_marker_goes_by_the_scratch_age() {
    let root = tempfile::tempdir().unwrap();
    let own = Owner::current().unwrap();
    let scratch = |tag: &str| {
        let path = root.path().join(format!("{PREFIX}{tag}"));
        std::fs::create_dir(&path).unwrap();
        path
    };
    let fifo = scratch("fifo");
    nix::unistd::mkfifo(&fifo.join(NAME), nix::sys::stat::Mode::S_IRWXU).unwrap();
    let empty = scratch("empty");
    std::fs::write(empty.join(NAME), "").unwrap();
    let live = scratch("live");
    marker::write(&live, Some(own.namespace)).unwrap();
    let after_creation = Some(i64::MAX);
    let before_creation = Some(0);
    for path in [&fifo, &empty, &live] {
        assert!(marker::may_live_since(path, before_creation), "{path:?}");
        assert!(marker::may_live_since(path, None), "{path:?}");
    }
    for path in [&fifo, &empty] {
        assert!(!marker::may_live_since(path, after_creation), "{path:?}");
    }
    assert!(
        marker::may_live_since(&live, after_creation),
        "a clock step reclaimed a scratch whose marker names this boot"
    );
    assert!(marker::may_live(&fifo) && marker::may_live(&empty) && marker::may_live(&live));
}
