//! The host child's acceptance configuration shares the stage's build cache
//! with a confined stage and gives an inherited one none.

use std::path::Path;
use std::sync::Arc;

use serial_test::serial;

use super::child::{acceptance_config, Progress};
use crate::models::stage::Stage;
use crate::plan::schema::CommandConfinement;
use crate::test_cache_env::{repo_with_work_dir, CacheEnv};
use crate::verify::criteria::{CachePolicy, CriteriaConfig};

fn config_for(work_dir: &Path, confinement: CommandConfinement) -> CriteriaConfig {
    let mut stage = Stage::new("cache-stage".to_string(), None);
    stage.id = "cache-stage".to_string();
    stage.sandbox.command_confinement = Some(confinement);
    let progress = Arc::new(Progress::new(work_dir, &stage.id, "job-1"));
    acceptance_config(&stage, None, work_dir, CachePolicy::Use, &progress).unwrap()
}

#[test]
#[serial]
fn a_confined_job_runs_with_the_stage_cache() {
    let env = CacheEnv::pin();
    let (repo, work_dir) = repo_with_work_dir();

    let config = config_for(&work_dir, CommandConfinement::Confined);

    let cache = config.stage_cache.expect("a confined job shares the cache");
    assert_eq!(cache.root(), env.root_of(repo.path(), "cache-stage"));
}

#[test]
#[serial]
fn an_inherited_job_resolves_no_cache_even_without_home() {
    let _env = CacheEnv::pin();
    let (_repo, work_dir) = repo_with_work_dir();
    std::env::remove_var("HOME");

    assert!(config_for(&work_dir, CommandConfinement::Inherit)
        .stage_cache
        .is_none());
}
