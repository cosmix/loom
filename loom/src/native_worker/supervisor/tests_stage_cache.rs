//! The supervisor's cache lookup and the host-side resolver agree on a root.

use super::stage_cache_of;
use crate::test_cache_env::{repo_with_work_dir, CacheEnv};
use crate::verify::criteria::stage_cache_for;

#[test]
#[serial_test::serial]
fn the_invocation_directory_resolves_the_stage_root_of_its_work_dir() {
    let _env = CacheEnv::pin();
    let (_repo, work_dir) = repo_with_work_dir();
    let dir = work_dir.join("native-workers").join("alpha").join("inv-1");
    std::fs::create_dir_all(&dir).unwrap();
    let from_supervisor = stage_cache_of(&dir, "alpha").unwrap();
    let from_host = stage_cache_for(&work_dir, "alpha").unwrap();
    assert_eq!(from_supervisor.root(), from_host.root());
    assert!(from_supervisor.root().is_dir());
}
