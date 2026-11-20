use super::*;
use std::os::unix::fs::PermissionsExt;

fn cache_in(home: &Path) -> StageCache {
    StageCache::resolve(home, "s1", home, None).unwrap()
}

#[test]
fn each_base_has_its_own_directory_beside_the_stage_cache() {
    let cache =
        StageCache::resolve(Path::new("/repo"), "s1", Path::new("/home/user"), None).unwrap();
    let dir = base_checkout_dir(&cache, "abc123").unwrap();
    let name = cache.root().file_name().unwrap();
    let caches = cache.root().parent().unwrap();
    let expected = caches
        .with_file_name("stage-base")
        .join(name)
        .join("abc123");
    assert_eq!(dir, expected);
    assert!(
        !dir.starts_with(cache.root()),
        "{} is writable by the stage",
        dir.display()
    );
    assert_ne!(dir, base_checkout_dir(&cache, "def456").unwrap());
}

#[test]
fn the_checkouts_are_loom_owned_and_outside_the_stage_cache() {
    let home = tempfile::tempdir().unwrap();
    let cache = cache_in(home.path());
    let root = cache.ensure().unwrap();
    // A checkout planted where the stage can write is never the one used.
    let planted = root.join("base-checkout").join("abc");
    std::fs::create_dir_all(&planted).unwrap();
    let dir = ensure_base_checkouts(&cache, "abc").unwrap();
    assert!(!dir.starts_with(&root), "{}", dir.display());
    assert_eq!(dir.parent().unwrap().file_name(), root.file_name());
    let mode = std::fs::metadata(dir.parent().unwrap())
        .unwrap()
        .permissions()
        .mode();
    assert_eq!(mode & 0o777, 0o700);
    assert!(!dir.exists(), "the checkout itself is made by `keep`");
}

#[test]
fn checkouts_of_other_bases_are_removed() {
    let home = tempfile::tempdir().unwrap();
    let cache = cache_in(home.path());
    let old = ensure_base_checkouts(&cache, "old").unwrap();
    std::fs::create_dir_all(old.join("build")).unwrap();
    let kept = ensure_base_checkouts(&cache, "new").unwrap();
    std::fs::create_dir(&kept).unwrap();
    std::fs::write(kept.join("file"), "x").unwrap();
    assert!(!old.exists(), "the old base's checkout is pruned");
    assert_eq!(ensure_base_checkouts(&cache, "new").unwrap(), kept);
    assert!(
        kept.join("file").exists(),
        "the current base's checkout stays"
    );
    assert!(ensure_base_checkouts(&cache, "..").is_err());
    assert!(ensure_base_checkouts(&cache, "a/b").is_err());
}

#[test]
fn a_symbolic_link_above_the_checkouts_is_refused() {
    let home = tempfile::tempdir().unwrap();
    let cache = cache_in(home.path());
    let caches = cache.ensure().unwrap();
    let elsewhere = home.path().join("elsewhere");
    std::fs::create_dir(&elsewhere).unwrap();
    let link = caches.parent().unwrap().with_file_name("stage-base");
    std::os::unix::fs::symlink(&elsewhere, &link).unwrap();
    assert!(ensure_base_checkouts(&cache, "abc").is_err());
    assert_eq!(std::fs::read_dir(&elsewhere).unwrap().count(), 0);
}

#[test]
fn removing_the_stage_cache_removes_its_checkouts() {
    let home = tempfile::tempdir().unwrap();
    let cache = cache_in(home.path());
    cache.ensure().unwrap();
    let dir = ensure_base_checkouts(&cache, "abc").unwrap();
    std::fs::create_dir(&dir).unwrap();
    assert!(super::super::cleanup::remove_stage_cache(&cache).unwrap());
    assert!(!dir.parent().unwrap().exists());
}

#[test]
fn a_symbolic_link_is_refused() {
    let temp = tempfile::tempdir().unwrap();
    let link = temp.path().join("link");
    std::os::unix::fs::symlink(temp.path(), &link).unwrap();
    let error = keep(&link.join("abc"), "/nonexistent", "abc").unwrap_err();
    assert!(format!("{error:#}").contains("symbolic link"), "{error:#}");
    let error = keep(&link, "/nonexistent", "abc").unwrap_err();
    assert!(format!("{error:#}").contains("symbolic link"), "{error:#}");
}

/// A repository whose one commit ignores `build/`, its common git
/// directory, the commit, and where its base checkout goes.
fn repo() -> (tempfile::TempDir, String, String, PathBuf) {
    let repo = crate::test_git::TestRepo::builder().no_seed().create();
    repo.commit_file(".gitignore", "build/\n", "ignore");
    let commit = repo.commit_file("src/a.txt", "a\n", "base");
    let common = repo.path().join(".git").to_str().unwrap().to_string();
    let dir = repo.path().join("checkouts").join("s1").join(&commit);
    std::fs::create_dir_all(dir.parent().unwrap().parent().unwrap()).unwrap();
    (repo.into_temp_dir(), common, commit, dir)
}

#[test]
fn a_clean_checkout_is_reused_with_its_ignored_build() {
    let (_temp, common, commit, dir) = repo();
    assert!(
        !keep(&dir, &common, &commit).unwrap(),
        "the first run makes it"
    );
    assert_eq!(
        std::fs::read_to_string(dir.join("src/a.txt")).unwrap(),
        "a\n"
    );
    let head = std::fs::read_to_string(dir.join(".git/HEAD")).unwrap();
    assert_eq!(head.trim(), commit);
    std::fs::create_dir_all(dir.join("build")).unwrap();
    std::fs::write(dir.join("build/out"), "warm").unwrap();
    assert!(
        keep(&dir, &common, &commit).unwrap(),
        "a clean checkout is reused"
    );
    assert!(dir.join("build/out").exists());
}

#[test]
fn a_changed_or_extra_file_makes_the_checkout_again() {
    let (_temp, common, commit, dir) = repo();
    keep(&dir, &common, &commit).unwrap();
    std::fs::write(dir.join("src/a.txt"), "changed\n").unwrap();
    assert!(!keep(&dir, &common, &commit).unwrap(), "a changed file");
    assert_eq!(
        std::fs::read_to_string(dir.join("src/a.txt")).unwrap(),
        "a\n"
    );
    std::fs::write(dir.join("src/extra.txt"), "new\n").unwrap();
    assert!(!keep(&dir, &common, &commit).unwrap(), "an untracked file");
    assert!(!dir.join("src/extra.txt").exists());
    std::fs::remove_file(dir.join("src/a.txt")).unwrap();
    assert!(!keep(&dir, &common, &commit).unwrap(), "a deleted file");
    assert!(keep(&dir, &common, &commit).unwrap());
}

#[test]
fn a_foreign_head_makes_the_checkout_again() {
    let (_temp, common, commit, dir) = repo();
    keep(&dir, &common, &commit).unwrap();
    std::fs::write(dir.join("build"), "stale").unwrap();
    std::fs::write(dir.join(".git/HEAD"), "ref: refs/heads/main\n").unwrap();
    assert!(!keep(&dir, &common, &commit).unwrap());
    assert!(!dir.join("build").exists(), "the old checkout is gone");
    let head = std::fs::read_to_string(dir.join(".git/HEAD")).unwrap();
    assert_eq!(head.trim(), commit);
}
