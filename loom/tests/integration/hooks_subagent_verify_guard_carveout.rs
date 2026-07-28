//! Integration-verify carve-out refusal tests for `subagent-verify-guard.sh`.
//!
//! Split from the parent file for size. The parent covers the GRANTED
//! direction; everything here covers the refusals, which is where the security
//! actually lives: the carve-out only ever relaxes the hook, so a hole in it
//! hands any subagent a full-suite exemption. Before these existed the
//! fail-safe could have been "simplified" back into a hole with every test
//! still green.
//!
//! Uses the parent's harness (process-tree construction, env scrubbing) via
//! `use super::*` - read the parent's module docs, especially GOTCHA 2 about
//! LOOM_STAGE_ID leaking in from a real stage session.

use super::*;

/// Write a stage file at an arbitrary glob position and stage type.
fn write_stage_file(work_dir: &Path, prefix: &str, stage_id: &str, stage_type: &str) {
    let stages_dir = work_dir.join("stages");
    fs::create_dir_all(&stages_dir).expect("create stages dir");
    let content = format!(
        "---\nid: {stage_id}\nname: Some Stage\nstatus: executing\nstage_type: {stage_type}\nplan_id: PLAN-test\nworking_dir: .\n---\n\n# Stage\n"
    );
    fs::write(stages_dir.join(format!("{prefix}-{stage_id}.md")), content)
        .expect("write stage file");
}

// =============================================================================
// The carve-out must be REFUSED unless exactly one stage file, declaring
// integration-verify, matches. Only the granted direction was covered before,
// so the fail-safe below could have been "simplified" back into a hole with
// every test still green.
// =============================================================================

#[test]
fn carve_out_refused_when_a_decoy_stage_file_also_matches() {
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());

    let work_dir = dir.path().join("work");
    let stage_id = "decoy-test-stage";
    // The real stage is a plain standard stage; the decoy claims
    // integration-verify AND sorts first, so a `ls ... | head -1` lookup would
    // hand a standard stage the full-suite carve-out. Preferring the file whose
    // `id:` agrees is no defence - the decoy's `id:` matches too, on purpose.
    write_stage_file(&work_dir, "02", stage_id, "standard");
    write_stage_file(&work_dir, "00", stage_id, "integration-verify");

    let (code, _stderr) = run_hook_in_tree(
        dir.path(),
        &hook_path,
        true,
        &bash_payload("cargo test"),
        &[
            ("LOOM_WORK_DIR", work_dir.to_str().unwrap()),
            ("LOOM_STAGE_ID", stage_id),
        ],
    );

    assert_eq!(
        code, 2,
        "two stage files match the glob, so the carve-out is AMBIGUOUS and must \
         not be granted - a planted 00-<id>.md declaring integration-verify \
         would otherwise buy any subagent a full-suite exemption"
    );
}

#[test]
fn carve_out_refused_for_a_non_integration_verify_stage() {
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());

    let work_dir = dir.path().join("work");
    let stage_id = "standard-test-stage";
    write_stage_file(&work_dir, "01", stage_id, "standard");

    let (code, _stderr) = run_hook_in_tree(
        dir.path(),
        &hook_path,
        true,
        &bash_payload("cargo test"),
        &[
            ("LOOM_WORK_DIR", work_dir.to_str().unwrap()),
            ("LOOM_STAGE_ID", stage_id),
        ],
    );

    assert_eq!(
        code, 2,
        "the single matching stage file declares stage_type: standard, so the \
         carve-out must not be granted"
    );
}

#[test]
fn carve_out_refused_when_no_stage_file_exists() {
    let dir = temp_dir_no_claude();
    let hook_path = install_hook(dir.path());

    // LOOM_STAGE_ID and LOOM_WORK_DIR are set but name nothing on disk - the
    // shape a leaked/stale env pair takes. Missing state must fail SAFE.
    let work_dir = dir.path().join("work");
    fs::create_dir_all(work_dir.join("stages")).expect("create stages dir");

    let (code, _stderr) = run_hook_in_tree(
        dir.path(),
        &hook_path,
        true,
        &bash_payload("cargo test"),
        &[
            ("LOOM_WORK_DIR", work_dir.to_str().unwrap()),
            ("LOOM_STAGE_ID", "stage-that-does-not-exist"),
        ],
    );

    assert_eq!(
        code, 2,
        "no stage file matches, so the carve-out must not be granted"
    );
}
