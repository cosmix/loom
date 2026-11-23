use super::retry_tests::{fixture, request};
use super::*;
use crate::fs::safe_read::OverLimit;
use crate::plan::parser::stage_definition_in;
use crate::planning::discover::MAX_DOC_BYTES;
use serial_test::serial;
use std::env;

struct RestoreCwd(PathBuf);

impl Drop for RestoreCwd {
    fn drop(&mut self) {
        env::set_current_dir(&self.0).unwrap();
    }
}

#[test]
#[serial]
fn apply_amendment_accepts_a_bare_relative_plan_path() {
    let (temp, work, plan) = fixture();
    let _cwd = RestoreCwd(env::current_dir().unwrap());
    env::set_current_dir(temp.path()).unwrap();

    let request = request("a", AmendmentPatch::Delete { index: 0 });
    apply_amendment(Path::new("plan.md"), &work, request).unwrap();

    let content = fs::read_to_string(plan).unwrap();
    let definition = stage_definition_in(&content, "a").unwrap().unwrap();
    assert_eq!(
        definition.acceptance,
        vec![AcceptanceCriterion::Simple("two".into())]
    );
}

#[test]
fn apply_amendment_refuses_an_oversized_plan() {
    let (_temp, work, plan) = fixture();
    fs::OpenOptions::new()
        .write(true)
        .open(&plan)
        .unwrap()
        .set_len(MAX_DOC_BYTES as u64 + 1)
        .unwrap();

    let request = request("a", AmendmentPatch::Delete { index: 0 });
    let error = apply_amendment(&plan, &work, request).unwrap_err();
    assert!(error.downcast_ref::<OverLimit>().is_some(), "{error:#}");
}
