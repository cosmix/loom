//! The findings an address session takes, and the documents it may change.
//!
//! The run is the draft team's machinery with a different scope: the documents the feature's
//! actionable findings are about (open, verified by the trace's checks), each written by one
//! teammate, and the findings ledger, written by the lead alone. A locked document (a running or
//! done plan) is not in the scope; a finding about locked documents only is still listed, for the
//! lead to mark it `unaddressed` with the reason.

use std::path::{Path, PathBuf};

use super::{StartError, MAX_TEAMMATES};
use crate::planning::checklist::Offer;
use crate::planning::findings::ledger_path;
use crate::planning::project::Project;

/// The most documents a run edits besides the ledger: a team scope holds at most 64 paths.
const MAX_DOCUMENTS: usize = 63;

/// One finding of the run and the documents it may change.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressItem {
    /// `F-<n>`.
    pub id: String,
    /// The editable documents its `About:` names, in token order.
    pub documents: Vec<PathBuf>,
}

/// A feature's address run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AddressRun {
    /// The owning feature.
    pub feature: String,
    /// The actionable findings in ledger order.
    pub findings: Vec<AddressItem>,
    /// The write scope: each editable document in the order the findings first name it, then
    /// the ledger.
    pub documents: Vec<PathBuf>,
    /// One teammate per editable document, at most [`MAX_TEAMMATES`]; none when the findings
    /// name no editable document.
    pub teammates: usize,
}

impl AddressRun {
    /// The ledger, the one file only the lead writes.
    pub fn ledger(&self) -> PathBuf {
        ledger_path(&self.feature)
    }
}

/// The address run of `feature`, refused when it has no actionable finding. Findings that would
/// take the scope past `MAX_DOCUMENTS` wait for the next run.
pub fn address_run(project: &Project, feature: &str) -> Result<AddressRun, StartError> {
    let refused = || StartError::NotOffered(Offer::AddressFindings.id());
    let state = project.feature(feature).ok_or_else(refused)?;
    let mut findings = Vec::new();
    let mut documents: Vec<PathBuf> = Vec::new();
    for traced in state
        .trace
        .findings
        .iter()
        .filter(|traced| traced.actionable())
    {
        let mut mine: Vec<PathBuf> = Vec::new();
        for node in &traced.about {
            if editable(project, &node.path) && !mine.contains(&node.path) {
                mine.push(node.path.clone());
            }
        }
        let fresh = mine.iter().filter(|path| !documents.contains(path)).count();
        if documents.len() + fresh > MAX_DOCUMENTS {
            continue;
        }
        for path in &mine {
            if !documents.contains(path) {
                documents.push(path.clone());
            }
        }
        findings.push(AddressItem {
            id: traced.finding.id.clone(),
            documents: mine,
        });
    }
    if findings.is_empty() {
        return Err(refused());
    }
    let teammates = documents.len().min(MAX_TEAMMATES);
    documents.push(ledger_path(feature));
    Ok(AddressRun {
        feature: feature.to_owned(),
        findings,
        documents,
        teammates,
    })
}

/// A loaded, unlocked planning document outside `doc/loom/`.
fn editable(project: &Project, path: &Path) -> bool {
    !path.starts_with("doc/loom") && project.docs.get(path).is_some_and(|doc| doc.lock.is_none())
}

#[cfg(test)]
#[path = "address_tests.rs"]
mod tests;
