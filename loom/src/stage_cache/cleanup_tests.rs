use super::*;
use std::os::unix::fs::{symlink, PermissionsExt};
use tempfile::TempDir;

fn cache_in(home: &Path) -> StageCache {
    StageCache::resolve(home, "alpha", home, None).unwrap()
}

#[test]
fn removes_a_nested_tree_with_a_closed_directory() {
    let home = TempDir::new().unwrap();
    let cache = cache_in(home.path());
    let closed = cache.root().join("go/mod/closed");
    std::fs::create_dir_all(&closed).unwrap();
    std::fs::write(closed.join("file"), "x").unwrap();
    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o500)).unwrap();
    assert!(remove_stage_cache(&cache).unwrap());
    assert!(cache.root().symlink_metadata().is_err());
}

#[test]
fn a_symlink_inside_is_unlinked_not_followed() {
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    std::fs::write(outside.path().join("kept"), "keep").unwrap();
    let cache = cache_in(home.path());
    std::fs::create_dir_all(cache.root().join("a")).unwrap();
    symlink(outside.path(), cache.root().join("a/link")).unwrap();
    assert!(remove_stage_cache(&cache).unwrap());
    assert!(cache.root().symlink_metadata().is_err());
    assert_eq!(
        std::fs::read_to_string(outside.path().join("kept")).unwrap(),
        "keep"
    );
}

#[test]
fn a_symlinked_root_is_refused() {
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    std::fs::write(outside.path().join("kept"), "keep").unwrap();
    let cache = cache_in(home.path());
    std::fs::create_dir_all(cache.root().parent().unwrap()).unwrap();
    symlink(outside.path(), cache.root()).unwrap();
    assert!(remove_stage_cache(&cache).is_err());
    assert!(outside.path().join("kept").exists());
}

#[test]
fn a_refused_root_still_removes_the_base_checkouts() {
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let cache = cache_in(home.path());
    let dirs = super::super::base_checkout::ensure_base_dirs(&cache, "abc").unwrap();
    std::fs::create_dir(&dirs.checkout).unwrap();
    let stage_tree = dirs.checkout.parent().unwrap().to_path_buf();
    std::fs::create_dir_all(cache.root().parent().unwrap()).unwrap();
    symlink(outside.path(), cache.root()).unwrap();
    let error = remove_stage_cache(&cache).unwrap_err();
    assert!(format!("{error:#}").contains("stage cache"), "{error:#}");
    assert!(
        stage_tree.symlink_metadata().is_err(),
        "the base checkouts go even though the root removal failed"
    );
    assert!(outside.path().exists());
}

#[test]
fn an_absent_root_is_false() {
    let home = TempDir::new().unwrap();
    assert!(!remove_stage_cache(&cache_in(home.path())).unwrap());
}

#[test]
fn a_symlinked_parent_is_refused_and_nothing_outside_is_removed() {
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let cache = cache_in(home.path());
    let target = outside.path().join(cache.root().file_name().unwrap());
    std::fs::create_dir_all(&target).unwrap();
    std::fs::write(target.join("kept"), "keep").unwrap();
    let parent = cache.root().parent().unwrap();
    std::fs::create_dir_all(parent.parent().unwrap()).unwrap();
    symlink(outside.path(), parent).unwrap();
    assert!(remove_stage_cache(&cache).is_err());
    assert_eq!(
        std::fs::read_to_string(target.join("kept")).unwrap(),
        "keep"
    );
}

#[test]
fn a_nested_directory_closed_to_its_owner_is_removed() {
    let home = TempDir::new().unwrap();
    let cache = cache_in(home.path());
    let closed = cache.root().join("pkg/closed");
    std::fs::create_dir_all(closed.join("inner")).unwrap();
    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o000)).unwrap();
    assert!(remove_stage_cache(&cache).unwrap());
    assert!(cache.root().symlink_metadata().is_err());
}

#[test]
fn a_read_only_directory_holding_a_file_is_removed() {
    let home = TempDir::new().unwrap();
    let cache = cache_in(home.path());
    let closed = cache.root().join("mod");
    std::fs::create_dir_all(&closed).unwrap();
    std::fs::write(closed.join("file"), "x").unwrap();
    std::fs::set_permissions(&closed, std::fs::Permissions::from_mode(0o500)).unwrap();
    assert!(remove_stage_cache(&cache).unwrap());
    assert!(cache.root().symlink_metadata().is_err());
}

mod resolver_agreement {
    use crate::stage_cache::cleanup::remove_stage_cache_of;
    use crate::test_cache_env::{repo_with_work_dir, CacheEnv};
    use crate::verify::criteria::stage_cache_for;
    use tempfile::TempDir;

    fn ensure_then_remove() {
        let (repo, work_dir) = repo_with_work_dir();
        let repo_root = repo.path().canonicalize().unwrap();
        let cache = stage_cache_for(&work_dir, "alpha").unwrap();
        let sibling = stage_cache_for(&work_dir, "beta").unwrap();
        let root = cache.ensure().unwrap();
        let kept = sibling.ensure().unwrap();
        assert!(remove_stage_cache_of(&repo_root, "alpha").unwrap());
        assert!(!root.exists());
        assert!(kept.is_dir());
    }

    #[test]
    #[serial_test::serial]
    fn removal_finds_the_root_without_xdg_cache_home() {
        let _env = CacheEnv::pin();
        ensure_then_remove();
    }

    #[test]
    #[serial_test::serial]
    fn removal_finds_the_root_with_xdg_cache_home() {
        let _env = CacheEnv::pin();
        let xdg = TempDir::new().unwrap();
        std::env::set_var("XDG_CACHE_HOME", xdg.path());
        ensure_then_remove();
    }
}
