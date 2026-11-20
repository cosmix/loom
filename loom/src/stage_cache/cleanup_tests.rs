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
fn an_absent_root_is_false() {
    let home = TempDir::new().unwrap();
    assert!(!remove_stage_cache(&cache_in(home.path())).unwrap());
}
