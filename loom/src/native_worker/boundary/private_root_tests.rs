//! The private invocation root: exclusive 0700 creation by a no-follow walk,
//! private copies of only the allowed Codex files, and removal that refuses a
//! foreign path and never follows a link. Fixture credentials only.
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};

use super::private_root::{create, remove, PrivateRoot, CODEX_HOME_DIR, NATIVE_DIR};
use crate::process::sandbox_probe::require_directory_owners;

const INVOCATION: &str = "inv-0123456789abcdef0123456789abcdef";
const FIXTURE_AUTH: &str = "{\"fixture\":true}\n";

struct Fixture {
    _dir: tempfile::TempDir,
    cache: PathBuf,
    host_home: PathBuf,
}

fn fixture() -> Fixture {
    let dir = tempfile::tempdir().expect("a temp dir");
    let base = dir.path().canonicalize().expect("canonical");
    let cache = base.join("cache");
    let host_home = base.join("host-codex");
    fs::create_dir_all(&cache).expect("cache");
    fs::create_dir_all(host_home.join("sessions")).expect("host home");
    fs::write(host_home.join("auth.json"), FIXTURE_AUTH).expect("auth");
    fs::write(host_home.join("AGENTS.md"), "rules\n").expect("agents");
    fs::write(host_home.join("config.toml"), "model = \"x\"\n").expect("config");
    fs::write(host_home.join("sessions/old.jsonl"), "{}\n").expect("session");
    Fixture {
        _dir: dir,
        cache,
        host_home,
    }
}

fn mode(path: &Path) -> u32 {
    fs::symlink_metadata(path)
        .expect("metadata")
        .permissions()
        .mode()
        & 0o777
}

fn created(fixture: &Fixture) -> PrivateRoot {
    create(&fixture.cache, INVOCATION, &fixture.host_home).expect("the private root")
}

#[test]
fn create_makes_a_closed_root_with_only_the_allowed_copies() {
    require_directory_owners!("create_makes_a_closed_root_with_only_the_allowed_copies");
    let fixture = fixture();
    let private = created(&fixture);
    assert_eq!(
        private.root,
        fixture.cache.join(NATIVE_DIR).join(INVOCATION)
    );
    assert_eq!(private.codex_home, private.root.join(CODEX_HOME_DIR));
    for dir in [&private.root, &private.tmp, &private.codex_home] {
        assert_eq!(mode(dir), 0o700, "{}", dir.display());
    }
    let sessions = private.codex_home.join("sessions");
    assert_eq!(mode(&sessions), 0o700);
    assert_eq!(fs::read_dir(&sessions).expect("sessions").count(), 0);
    let auth = private.codex_home.join("auth.json");
    assert_eq!(fs::read_to_string(&auth).expect("auth copy"), FIXTURE_AUTH);
    assert_eq!(mode(&auth), 0o600);
    assert_eq!(mode(&private.codex_home.join("AGENTS.md")), 0o600);
    assert!(!private.codex_home.join("config.toml").exists());
    assert!(fs::read_dir(&private.tmp).expect("tmp").next().is_none());
}

#[test]
fn create_refuses_an_existing_invocation_root() {
    require_directory_owners!("create_refuses_an_existing_invocation_root");
    let fixture = fixture();
    created(&fixture);
    let again = create(&fixture.cache, INVOCATION, &fixture.host_home);
    assert!(again.is_err(), "a second launch must not reuse the root");
}

#[test]
fn create_refuses_a_symlinked_native_directory() {
    let fixture = fixture();
    let elsewhere = fixture.cache.parent().expect("base").join("elsewhere");
    fs::create_dir(&elsewhere).expect("elsewhere");
    symlink(&elsewhere, fixture.cache.join(NATIVE_DIR)).expect("plant a link");
    assert!(create(&fixture.cache, INVOCATION, &fixture.host_home).is_err());
    assert_eq!(fs::read_dir(&elsewhere).expect("elsewhere").count(), 0);
}

#[test]
fn create_refuses_a_symlinked_auth_file() {
    require_directory_owners!("create_refuses_a_symlinked_auth_file");
    let fixture = fixture();
    let secret = fixture.host_home.join("real-auth.json");
    fs::write(&secret, FIXTURE_AUTH).expect("target");
    fs::remove_file(fixture.host_home.join("auth.json")).expect("unlink");
    symlink(&secret, fixture.host_home.join("auth.json")).expect("link");
    let error = create(&fixture.cache, INVOCATION, &fixture.host_home).expect_err("refused");
    assert!(format!("{error:#}").contains("symlink"), "{error:#}");
}

#[test]
fn create_refuses_a_bad_invocation_id() {
    let fixture = fixture();
    assert!(create(&fixture.cache, "../escape", &fixture.host_home).is_err());
}

#[test]
fn remove_deletes_the_tree_and_is_idempotent() {
    require_directory_owners!("remove_deletes_the_tree_and_is_idempotent");
    let fixture = fixture();
    let private = created(&fixture);
    let nested = private.codex_home.join("sessions/2026/11");
    fs::create_dir_all(&nested).expect("nested");
    fs::write(nested.join("rollout.jsonl"), "{}\n").expect("rollout");
    fs::set_permissions(&nested, fs::Permissions::from_mode(0o500)).expect("close");
    remove(&private.root, INVOCATION).expect("removed");
    assert!(!private.root.exists());
    remove(&private.root, INVOCATION).expect("already gone is success");
    assert!(fixture.cache.join(NATIVE_DIR).is_dir(), "the parent stays");
}

#[test]
fn remove_unlinks_a_planted_link_without_following_it() {
    require_directory_owners!("remove_unlinks_a_planted_link_without_following_it");
    let fixture = fixture();
    let private = created(&fixture);
    let outside = fixture.cache.parent().expect("base").join("outside");
    fs::create_dir(&outside).expect("outside");
    fs::write(outside.join("keep"), "keep\n").expect("keep");
    symlink(&outside, private.tmp.join("link")).expect("plant");
    remove(&private.root, INVOCATION).expect("removed");
    assert!(
        outside.join("keep").is_file(),
        "the link target is untouched"
    );
}

#[test]
fn remove_refuses_a_path_that_is_not_a_native_invocation_root() {
    require_directory_owners!("remove_refuses_a_path_that_is_not_a_native_invocation_root");
    let fixture = fixture();
    let private = created(&fixture);
    let foreign = fixture.cache.join("other").join(INVOCATION);
    fs::create_dir_all(&foreign).expect("foreign");
    assert!(remove(&foreign, INVOCATION).is_err());
    assert!(foreign.is_dir());
    let other = "inv-ffffffffffffffffffffffffffffffff";
    assert!(remove(&private.root, other).is_err());
    assert!(remove(Path::new("native").join(INVOCATION).as_path(), INVOCATION).is_err());
    assert!(private.root.is_dir());
}

#[test]
fn remove_refuses_to_walk_through_a_symlinked_ancestor() {
    require_directory_owners!("remove_refuses_to_walk_through_a_symlinked_ancestor");
    let fixture = fixture();
    let private = created(&fixture);
    let alias = fixture.cache.parent().expect("base").join("alias");
    symlink(&fixture.cache, &alias).expect("alias");
    let through_link = alias.join(NATIVE_DIR).join(INVOCATION);
    assert!(remove(&through_link, INVOCATION).is_err());
    assert!(
        private.root.is_dir(),
        "nothing was removed through the link"
    );
}

/// A stage-cache base `<temp>/ancestor/cache` whose parent `ancestor` has
/// `ancestor_mode`, with the private root already created under it.
fn under_ancestor(ancestor_mode: u32) -> (Fixture, PathBuf, PathBuf) {
    let fixture = fixture();
    let ancestor = fixture.cache.parent().expect("base").join("ancestor");
    let cache = ancestor.join("cache");
    fs::create_dir_all(&cache).expect("nested cache");
    let private = create(&cache, INVOCATION, &fixture.host_home).expect("the private root");
    fs::set_permissions(&ancestor, fs::Permissions::from_mode(ancestor_mode)).expect("chmod");
    (fixture, ancestor, private.root)
}

fn restore(ancestor: &Path) {
    fs::set_permissions(ancestor, fs::Permissions::from_mode(0o700)).expect("restore");
}

#[test]
fn an_ancestor_writable_by_others_without_sticky_is_refused_by_create_and_remove() {
    require_directory_owners!(
        "an_ancestor_writable_by_others_without_sticky_is_refused_by_create_and_remove"
    );
    let (fixture, ancestor, root) = under_ancestor(0o777);
    let cache = ancestor.join("cache");
    let refused = create(
        &cache,
        "inv-ffffffffffffffffffffffffffffffff",
        &fixture.host_home,
    );
    let removal = remove(&root, INVOCATION);
    restore(&ancestor);
    let error = refused.expect_err("create walks through the open ancestor");
    assert!(
        format!("{error:#}").contains("writable by other users"),
        "{error:#}"
    );
    let error = removal.expect_err("remove walks through the open ancestor");
    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
    assert!(root.is_dir(), "nothing was removed");
}

#[test]
fn a_sticky_world_writable_ancestor_is_accepted() {
    require_directory_owners!("a_sticky_world_writable_ancestor_is_accepted");
    let (fixture, ancestor, root) = under_ancestor(0o1777);
    let cache = ancestor.join("cache");
    let created = create(
        &cache,
        "inv-ffffffffffffffffffffffffffffffff",
        &fixture.host_home,
    );
    let removal = remove(&root, INVOCATION);
    restore(&ancestor);
    created.expect("create accepts a sticky ancestor");
    removal.expect("remove accepts a sticky ancestor");
    assert!(!root.exists());
}

#[test]
fn a_regular_file_in_place_of_an_ancestor_directory_is_refused() {
    require_directory_owners!("a_regular_file_in_place_of_an_ancestor_directory_is_refused");
    let fixture = fixture();
    let base = fixture.cache.parent().expect("base");
    let file = base.join("not-a-directory");
    fs::write(&file, "file\n").expect("file");
    let cache = file.join("cache");
    let error = create(&cache, INVOCATION, &fixture.host_home).expect_err("a file ancestor");
    assert!(
        format!("{error:#}").contains("is not a directory"),
        "{error:#}"
    );
    let root = cache.join(NATIVE_DIR).join(INVOCATION);
    let error = remove(&root, INVOCATION).expect_err("a file ancestor");
    assert!(error.to_string().contains("is not a directory"), "{error}");
}

#[path = "private_root_rollouts_tests.rs"]
mod rollouts;
