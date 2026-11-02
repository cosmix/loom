use super::*;

#[test]
#[serial]
fn a_job_that_is_not_running_is_refused_without_a_result() {
    let fx = Fixture::new(&["true"], StageType::Standard);
    let record = fx.job(JobState::Queued, false);
    assert!(run_job(&fx.work_dir, STAGE, &record.id).is_err());
    let saved = store::load_result(&fx.work_dir, STAGE, &record.id).unwrap();
    assert_eq!(saved, None);
}

#[test]
#[serial]
fn a_state_root_without_a_plan_source_is_an_error_result() {
    let fx = Fixture::new(&["true"], StageType::Standard);
    fs::remove_file(fx.work_dir.join("config.toml")).unwrap();
    let record = fx.job(JobState::Running, false);
    let result = fx.run(&record);
    let error = error_of(&result);
    assert!(error.contains("plan source"), "{error}");
    assert!(result.criteria.is_empty(), "nothing ran");
}
