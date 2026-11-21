//! `reviews/<stage>/impact.json`: the impact-test failures the stage's
//! disputes accepted (DESIGN D14, D15). Only the daemon writes it, when it
//! applies an accepted verdict (`verdict_records::append_impact_acceptance`);
//! the gate reads it.
//!
//! An accepted failure covers a later run only when that run fails exactly
//! as the dispute saw it: the same kind with the same count or exit code,
//! over the same selection, which is identified by the digest of every
//! target (`selection_digest`), not by the bounded names the record lists for
//! display. Any other run, a smaller count included, is judged again; a record
//! without a digest matches nothing. The record a dispute accepts is the
//! unauthenticated observed report, persisted by the host completion job or
//! operator completion. Stage-controlled test output can still forge its
//! contents; an acceptance excuses nothing beyond the failure the
//! adjudicator re-ran.
//!
//! What a dispute accepts comes from the observed record, never from the
//! filing agent's request: the daemon copies it into the dispute's evidence
//! when it files the dispute.

use anyhow::Result;
use serde::{Deserialize, Serialize};
use std::path::Path;

use super::failure::{selection_digest, FailureKind, ImpactFailure};
use crate::testrun::TestTarget;
use crate::verify::review::store::{self, RECORD_VERSION};

pub(in crate::verify) const ACCEPTED_FILE: &str = "impact.json";

/// `impact.json`; an absent file accepts nothing.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedImpact {
    pub version: u32,
    pub accepted: Vec<AcceptedFailure>,
}

impl Default for AcceptedImpact {
    fn default() -> Self {
        Self {
            version: RECORD_VERSION,
            accepted: Vec::new(),
        }
    }
}

/// One accepted failure, as the dispute's evidence showed it.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct AcceptedFailure {
    pub id: String,
    pub kind: FailureKind,
    /// The selected tests as the evidence listed them, for display only.
    pub tests: Vec<String>,
    /// `selection_digest` of the whole selection the dispute saw; a record
    /// without one matches no selection.
    #[serde(default)]
    pub selection_digest: String,
    pub dispute: u32,
}

impl AcceptedFailure {
    /// `failure` as dispute `dispute` accepted it.
    pub fn of(failure: &ImpactFailure, dispute: u32) -> Self {
        Self {
            id: failure.id.clone(),
            kind: failure.kind.clone(),
            tests: failure.tests.clone(),
            selection_digest: failure.selection_digest.clone(),
            dispute,
        }
    }
}

/// What the accepted record says about one failure.
#[derive(Debug, PartialEq, Eq)]
pub(super) enum Standing {
    /// No dispute accepted this selection.
    Unaccepted,
    /// A dispute accepted exactly this failure: the note.
    Covered(String),
    /// A dispute accepted this selection failing otherwise: how.
    Changed(String),
}

impl AcceptedImpact {
    /// Where `id`'s failure of `kind` over the selection `targets` stands.
    pub(super) fn standing(
        &self,
        id: &str,
        kind: &FailureKind,
        targets: &[TestTarget],
    ) -> Standing {
        let Some(record) = self.accepted.iter().find(|record| record.id == id) else {
            return Standing::Unaccepted;
        };
        let dispute = record.dispute;
        let accepted = record.kind.describe();
        if record.selection_digest != selection_digest(targets) {
            return Standing::Changed(format!(
                "dispute {dispute} accepted it ({accepted}) over a different selection of tests"
            ));
        }
        if record.kind != *kind {
            return Standing::Changed(format!(
                "dispute {dispute} accepted it when it {accepted}; it {} now",
                kind.describe()
            ));
        }
        Standing::Covered(format!(
            "`{id}` {}, as dispute {dispute} accepted it",
            kind.describe()
        ))
    }
}

/// `reviews/<stage>/impact.json` in `work_dir`.
pub fn load_accepted(work_dir: &Path, stage_id: &str) -> Result<AcceptedImpact> {
    let accepted: AcceptedImpact =
        store::load_optional(work_dir, stage_id, ACCEPTED_FILE)?.unwrap_or_default();
    store::check_version(
        accepted.version,
        &format!("{ACCEPTED_FILE} of stage '{stage_id}'"),
    )?;
    Ok(accepted)
}
