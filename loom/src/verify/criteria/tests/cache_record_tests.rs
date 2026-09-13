//! Certified cache-record integrity tests.

use std::path::Path;
use std::time::Duration;

use serial_test::serial;
use tempfile::TempDir;

use crate::models::stage::{AcceptanceCriterion, CommandConfinement};
use crate::verify::criteria::cache::{
    cache_file_path, compute_cache_key, lookup_pass, store_pass, CachePolicy,
};
use crate::verify::criteria::cache_contract::{CachedCriterionPass, CriterionContract};
use crate::verify::criteria::cache_fingerprint::InputFingerprint;
use crate::verify::criteria::confine::CommandSpec;
use crate::verify::criteria::result::CriterionResult;

fn simple_contract() -> CriterionContract {
    CriterionContract::new(
        &CommandSpec::shell("printf ok"),
        &AcceptanceCriterion::Simple("printf ok".to_string()),
        Duration::from_secs(1),
        CommandConfinement::Confined,
    )
}

fn passing_result() -> CriterionResult {
    CriterionResult::new(
        "printf ok".to_string(),
        true,
        "ok".to_string(),
        String::new(),
        Some(0),
        Duration::from_millis(5),
        false,
    )
}

fn input_fingerprint(digest: &str) -> InputFingerprint {
    InputFingerprint {
        digest: digest.to_string(),
        tree_head: "head".to_string(),
    }
}

#[test]
fn certified_pass_round_trips_and_mismatched_context_misses() {
    let work_dir = TempDir::new().unwrap();
    let contract = simple_contract();
    let input = input_fingerprint("input-a");
    let result = passing_result();
    let verdict = contract.verdict(&result);
    assert!(store_pass(
        work_dir.path(),
        &contract,
        &input,
        &result,
        result.duration,
        verdict,
    )
    .unwrap());

    let found = lookup_pass(work_dir.path(), &contract, &input).unwrap();
    assert_eq!(found.actual_exit, 0);
    assert!(lookup_pass(work_dir.path(), &contract, &input_fingerprint("input-b")).is_none());
}

#[test]
#[serial]
fn legacy_and_truncated_records_are_misses() {
    let contract = simple_contract();
    let input = input_fingerprint("input");
    let digest = compute_cache_key(&contract, &input).unwrap();
    assert_record_miss(&contract, &input, &digest, "{\"command\":\"legacy\"}");
    assert_record_miss(&contract, &input, &digest, "{\"version\":2");
}

fn assert_record_miss(
    contract: &CriterionContract,
    input: &InputFingerprint,
    digest: &str,
    content: &str,
) {
    let work_dir = TempDir::new().unwrap();
    let path = cache_file_path(work_dir.path(), digest);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, content).unwrap();
    assert!(lookup_pass(work_dir.path(), contract, input).is_none());
}

#[test]
#[serial]
fn invalid_verdict_digest_and_oversized_record_are_misses() {
    let work_dir = TempDir::new().unwrap();
    let contract = simple_contract();
    let input = input_fingerprint("input");
    let result = passing_result();
    let mut record = CachedCriterionPass::from_result(
        &contract,
        &input,
        &result,
        result.duration,
        contract.verdict(&result),
    )
    .unwrap();
    record.verdict.passed = false;
    write_record(work_dir.path(), &contract, &input, &record);
    assert!(lookup_pass(work_dir.path(), &contract, &input).is_none());

    record.verdict.passed = true;
    record.input_context_digest = "different-input".to_string();
    write_record(work_dir.path(), &contract, &input, &record);
    assert!(lookup_pass(work_dir.path(), &contract, &input).is_none());

    let digest = compute_cache_key(&contract, &input).unwrap();
    std::fs::write(
        cache_file_path(work_dir.path(), &digest),
        "x".repeat(40 * 1024),
    )
    .unwrap();
    assert!(lookup_pass(work_dir.path(), &contract, &input).is_none());
}

fn write_record(
    work_dir: &Path,
    contract: &CriterionContract,
    input: &InputFingerprint,
    record: &CachedCriterionPass,
) {
    let digest = compute_cache_key(contract, input).unwrap();
    let path = cache_file_path(work_dir, &digest);
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, serde_json::to_string(record).unwrap()).unwrap();
}

struct EnvVarGuard {
    original: Option<String>,
}

impl EnvVarGuard {
    fn set(value: &str) -> Self {
        let original = std::env::var("LOOM_ACCEPTANCE_CACHE").ok();
        std::env::set_var("LOOM_ACCEPTANCE_CACHE", value);
        Self { original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var("LOOM_ACCEPTANCE_CACHE", value),
            None => std::env::remove_var("LOOM_ACCEPTANCE_CACHE"),
        }
    }
}

#[test]
#[serial]
fn cache_policy_respects_emergency_bypass() {
    let _guard = EnvVarGuard::set("0");
    assert_eq!(CachePolicy::from_env(), CachePolicy::Bypass);
}

#[test]
#[serial]
fn cache_policy_uses_cache_for_other_values() {
    let _guard = EnvVarGuard::set("1");
    assert_eq!(CachePolicy::from_env(), CachePolicy::Use);
}
