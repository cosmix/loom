//! Complete acceptance-criterion contracts and certified cache records.

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::time::Duration;

use super::cache_fingerprint::InputFingerprint;
use super::confine::CommandSpec;
use super::executor::{
    OUTPUT_COLLECTION_TIMEOUT_MARKER, OUTPUT_READ_ERROR_MARKER, OUTPUT_TRUNCATED_MARKER,
};
use super::result::CriterionResult;
use crate::models::stage::CommandConfinement;
use crate::plan::schema::AcceptanceCriterion;

pub(super) const CACHE_RECORD_VERSION: u32 = 3;
pub(super) const DIAGNOSTIC_TAIL_BYTES: usize = 4 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum ContractCommand {
    Shell(String),
    Program { program: String, args: Vec<String> },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
enum CriterionKind {
    Simple,
    Extended,
}

/// Lossless, deterministic description of everything that changes a verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub(super) struct CriterionContract {
    command: ContractCommand,
    kind: CriterionKind,
    expected_exit: i32,
    stdout_contains: Vec<String>,
    stdout_not_contains: Vec<String>,
    stderr_must_be_empty: bool,
    timeout_secs: u64,
    timeout_nanos: u32,
    confinement: CommandConfinement,
    zero_test_guard: bool,
}

impl CriterionContract {
    pub(super) fn new(
        spec: &CommandSpec,
        criterion: &AcceptanceCriterion,
        timeout: Duration,
        confinement: CommandConfinement,
    ) -> Self {
        let command = match spec {
            CommandSpec::Shell(line) => ContractCommand::Shell(line.clone()),
            CommandSpec::Program { program, args } => ContractCommand::Program {
                program: program.clone(),
                args: args.clone(),
            },
        };
        let (kind, expected_exit, contains, not_contains, stderr_empty) = match criterion {
            AcceptanceCriterion::Simple(_) => {
                (CriterionKind::Simple, 0, Vec::new(), Vec::new(), false)
            }
            AcceptanceCriterion::Extended(check) => (
                CriterionKind::Extended,
                check.exit_code.unwrap_or(0),
                check.stdout_contains.clone(),
                check.stdout_not_contains.clone(),
                check.stderr_empty == Some(true),
            ),
        };
        Self {
            command,
            kind,
            expected_exit,
            stdout_contains: contains,
            stdout_not_contains: not_contains,
            stderr_must_be_empty: stderr_empty,
            timeout_secs: timeout.as_secs(),
            timeout_nanos: timeout.subsec_nanos(),
            confinement,
            zero_test_guard: false,
        }
    }

    /// Fail a run whose test runner selected zero tests (DESIGN D10; plan v2).
    pub(super) fn with_zero_test_guard(mut self, enabled: bool) -> Self {
        self.zero_test_guard = enabled;
        self
    }

    pub(super) fn guards_zero_tests(&self) -> bool {
        self.zero_test_guard
    }

    /// Whether the zero-test guard fails a run that executed `tests_executed`.
    pub(super) fn rejects_zero_tests(&self, tests_executed: Option<u64>) -> bool {
        self.zero_test_guard && tests_executed == Some(0)
    }

    pub(super) fn digest(&self) -> Option<String> {
        let encoded = serde_json::to_vec(self).ok()?;
        Some(hex::encode(Sha256::digest(encoded)))
    }

    pub(super) fn expected_exit(&self) -> i32 {
        self.expected_exit
    }

    pub(super) fn stdout_contains(&self) -> &[String] {
        &self.stdout_contains
    }

    pub(super) fn stdout_not_contains(&self) -> &[String] {
        &self.stdout_not_contains
    }

    pub(super) fn timeout(&self) -> Duration {
        Duration::new(self.timeout_secs, self.timeout_nanos)
    }

    pub(super) fn verdict(&self, result: &CriterionResult) -> AssertionVerdict {
        let timed_out = result.timed_out;
        let exit_code_matched = result.exit_code == Some(self.expected_exit);
        let stdout_contains_matched = self
            .stdout_contains
            .iter()
            .map(|pattern| result.stdout.contains(pattern.as_str()))
            .collect::<Vec<_>>();
        let stdout_not_contains_matched = self
            .stdout_not_contains
            .iter()
            .map(|pattern| !result.stdout.contains(pattern.as_str()))
            .collect::<Vec<_>>();
        let stderr_empty_matched = self
            .stderr_must_be_empty
            .then_some(result.stderr.is_empty());
        let passed = !timed_out
            && exit_code_matched
            && stdout_contains_matched.iter().all(|matched| *matched)
            && stdout_not_contains_matched.iter().all(|matched| *matched)
            && stderr_empty_matched.unwrap_or(true);
        AssertionVerdict {
            passed,
            timed_out,
            output_complete: output_is_complete(result),
            exit_code_matched,
            stdout_contains_matched,
            stdout_not_contains_matched,
            stderr_empty_matched,
            tests_executed: None,
        }
    }

    /// [`Self::verdict`] for a run whose test runner reported `tests_executed`.
    pub(super) fn verdict_with_tests(
        &self,
        result: &CriterionResult,
        tests_executed: Option<u64>,
    ) -> AssertionVerdict {
        let mut verdict = self.verdict(result);
        verdict.passed &= !self.rejects_zero_tests(tests_executed);
        verdict.tests_executed = tests_executed;
        verdict
    }
}

/// Per-assertion proof retained by a certified pass.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct AssertionVerdict {
    pub(super) passed: bool,
    pub(super) timed_out: bool,
    pub(super) output_complete: bool,
    pub(super) exit_code_matched: bool,
    pub(super) stdout_contains_matched: Vec<bool>,
    pub(super) stdout_not_contains_matched: Vec<bool>,
    pub(super) stderr_empty_matched: Option<bool>,
    /// Tests the run executed, when the zero-test guard read a count.
    pub(super) tests_executed: Option<u64>,
}

impl AssertionVerdict {
    fn certifies(&self, contract: &CriterionContract) -> bool {
        self.passed
            && !self.timed_out
            && self.output_complete
            && self.exit_code_matched
            && all_true_with_len(
                &self.stdout_contains_matched,
                contract.stdout_contains.len(),
            )
            && all_true_with_len(
                &self.stdout_not_contains_matched,
                contract.stdout_not_contains.len(),
            )
            && self.stderr_empty_matched == contract.stderr_must_be_empty.then_some(true)
            && !contract.rejects_zero_tests(self.tests_executed)
    }
}

/// Versioned evidence that one fully evaluated criterion passed.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct CachedCriterionPass {
    pub(super) version: u32,
    pub(super) contract_digest: String,
    pub(super) input_context_digest: String,
    pub(super) source_head: String,
    pub(super) original_duration_ms: u64,
    pub(super) actual_exit: i32,
    pub(super) verdict: AssertionVerdict,
    pub(super) stdout_tail: String,
    pub(super) stdout_tail_truncated: bool,
    pub(super) stderr_tail: String,
    pub(super) stderr_tail_truncated: bool,
    pub(super) recorded_at: DateTime<Utc>,
}

impl CachedCriterionPass {
    pub(super) fn from_result(
        contract: &CriterionContract,
        fingerprint: &InputFingerprint,
        result: &CriterionResult,
        original_duration: Duration,
        verdict: AssertionVerdict,
    ) -> Option<Self> {
        let contract_digest = contract.digest()?;
        let actual_exit = result.exit_code?;
        if actual_exit != contract.expected_exit || !verdict.certifies(contract) {
            return None;
        }
        let (stdout_tail, stdout_tail_truncated) = diagnostic_tail(&result.stdout);
        let (stderr_tail, stderr_tail_truncated) = diagnostic_tail(&result.stderr);
        Some(Self {
            version: CACHE_RECORD_VERSION,
            contract_digest,
            input_context_digest: fingerprint.digest.clone(),
            source_head: fingerprint.tree_head.clone(),
            original_duration_ms: millis(original_duration),
            actual_exit,
            verdict,
            stdout_tail,
            stdout_tail_truncated,
            stderr_tail,
            stderr_tail_truncated,
            recorded_at: Utc::now(),
        })
    }

    pub(super) fn certifies(
        &self,
        contract: &CriterionContract,
        fingerprint: &InputFingerprint,
    ) -> bool {
        self.version == CACHE_RECORD_VERSION
            && contract.digest().as_deref() == Some(self.contract_digest.as_str())
            && self.input_context_digest == fingerprint.digest
            && self.source_head == fingerprint.tree_head
            && self.actual_exit == contract.expected_exit
            && self.stdout_tail.len() <= DIAGNOSTIC_TAIL_BYTES
            && self.stderr_tail.len() <= DIAGNOSTIC_TAIL_BYTES
            && self.verdict.certifies(contract)
    }
}

fn all_true_with_len(values: &[bool], expected_len: usize) -> bool {
    values.len() == expected_len && values.iter().all(|matched| *matched)
}

fn output_is_complete(result: &CriterionResult) -> bool {
    const INCOMPLETE_MARKERS: [&str; 3] = [
        OUTPUT_TRUNCATED_MARKER,
        OUTPUT_COLLECTION_TIMEOUT_MARKER,
        OUTPUT_READ_ERROR_MARKER,
    ];
    INCOMPLETE_MARKERS
        .iter()
        .all(|marker| !result.stdout.contains(*marker) && !result.stderr.contains(*marker))
}

fn diagnostic_tail(value: &str) -> (String, bool) {
    if value.len() <= DIAGNOSTIC_TAIL_BYTES {
        return (value.to_string(), false);
    }
    let mut start = value.len() - DIAGNOSTIC_TAIL_BYTES;
    while !value.is_char_boundary(start) {
        start += 1;
    }
    (value[start..].to_string(), true)
}

fn millis(duration: Duration) -> u64 {
    u64::try_from(duration.as_millis()).unwrap_or(u64::MAX)
}
