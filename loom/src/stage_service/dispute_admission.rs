//! What admits a plan v2 dispute: who may file the kind, and the daemon's own
//! re-derivation of the evidence it names.

use anyhow::{bail, Context, Result};
use std::collections::HashSet;
use std::path::Path;

use crate::fs::session_files::load_session_exact_unlocked;
use crate::models::dispute::{
    select_events, select_findings, select_impact, DisputeKind, FileReplacement,
    MAX_REPLACEMENT_BYTES,
};
use crate::models::stage::{Stage, StageStatus, StageType};
use crate::utils::sha256_hex;
use crate::verify::contracts::drift::{frozen_drift, Drift, DriftKind};
use crate::verify::contracts::site::stage_site;
use crate::verify::contracts::store::{contract_files, load_freeze, FreezeRecord, FrozenFile};
use crate::verify::impact_tests::{check_failure_id, load_observed};
use crate::verify::review::store::open_findings;

/// Admission decisions are durable; errors reading their evidence are not.
pub(super) enum Admission {
    Accepted(DisputeKind),
    Rejected(String),
}

/// Refuse a kind the stage's current session may not file. The session record
/// `stage.session` names is the filer: the socket admits a dispute only from
/// the stage's own Stage session, the relay never from a contract writer, and
/// loom files a freeze refusal for the writer it refused. A stage with no
/// current session, or whose session record is gone, admits every kind but a
/// freeze refusal: that is an operator's filing.
///
/// Runs under the exclusive stage lock, and `complete_stage` takes the
/// sessions-directory lock before the stage lock, so this read takes no lock.
pub(super) fn check_filer(
    work_dir: &Path,
    stage: &Stage,
    kind: &DisputeKind,
) -> Result<Option<String>> {
    let session = match stage.session.as_deref() {
        Some(session_id) => load_session_exact_unlocked(work_dir, session_id)?,
        None => None,
    };
    match session {
        Some(session) if !kind.filed_by(session.session_type) => Ok(Some(format!(
            "a {} dispute is not filed for stage '{}' while its {} session '{}' owns it",
            kind.name(),
            stage.id,
            session.session_type,
            session.id
        ))),
        Some(_) => Ok(None),
        None if matches!(kind, DisputeKind::FreezeRefusal { .. }) => Ok(Some(format!(
            "stage '{}' has no contract writer whose freeze could be refused",
            stage.id
        ))),
        None => Ok(None),
    }
}

/// A freeze refusal stands only while the contract phase is open: a standard
/// stage with contracts, executing, its contracts not frozen yet.
fn check_unfrozen(work_dir: &Path, stage: &Stage) -> Result<Option<String>> {
    if stage.stage_type != StageType::Standard || stage.contracts.is_empty() {
        return Ok(Some(format!(
            "stage '{}' has no contracts to freeze",
            stage.id
        )));
    }
    if stage.status != StageStatus::Executing {
        return Ok(Some(format!(
            "stage '{}' is {}, not executing its contract phase",
            stage.id, stage.status
        )));
    }
    if load_freeze(work_dir, &stage.id)?.is_some() {
        return Ok(Some(format!(
            "the contracts of stage '{}' are already frozen",
            stage.id
        )));
    }
    Ok(None)
}

/// The contract dispute to record. `contract_ids` must be distinct frozen
/// contracts of the stage. Each replacement must name a frozen file, at most
/// once, be bounded and differ from the frozen content, and a replaced file
/// holding a contract needs every frozen contract in it disputed too. No frozen
/// file may have drifted in the worktree: a frozen file changes only through a
/// replacement. Each replacement is bound to its frozen file's hash, which
/// overwrites any value the client sent (`apply_contract_replacement.rs`
/// refuses an accept once the file moved on).
fn confirm_contract(
    work_dir: &Path,
    stage: &Stage,
    contract_ids: Vec<String>,
    mut replacements: Vec<FileReplacement>,
) -> Result<DisputeKind> {
    let Some(record) = load_freeze(work_dir, &stage.id)? else {
        bail!("the contracts of stage '{}' are not frozen", stage.id);
    };
    check_contract_ids(stage, &record, &contract_ids)?;
    let mut replaced = HashSet::new();
    for replacement in &mut replacements {
        if !replaced.insert(replacement.file.clone()) {
            bail!(
                "the frozen file '{}' is replaced twice: file one replacement per file",
                replacement.file
            );
        }
        let frozen = frozen_file(&record, &replacement.file)?;
        check_disputes_every_contract_of(stage, &record, &contract_ids, &replacement.file)?;
        check_replacement_content(replacement, &frozen.sha256)?;
        replacement.base_sha256 = Some(frozen.sha256.clone());
    }
    check_no_drift(work_dir, stage, &record)?;
    Ok(DisputeKind::Contract {
        contract_ids,
        replacements,
    })
}

/// At least one id, each named once, each a frozen contract the stage still
/// lists (an accepted amendment may have deleted one the record keeps).
fn check_contract_ids(stage: &Stage, record: &FreezeRecord, contract_ids: &[String]) -> Result<()> {
    if contract_ids.is_empty() {
        bail!("no contract is named: name each disputed contract with `--contract <id>`");
    }
    let mut named = HashSet::new();
    for id in contract_ids {
        if !named.insert(id.as_str()) {
            bail!("'{id}' is named twice: name each contract once");
        }
        let current = stage.contracts.iter().any(|contract| contract.id == *id);
        if !current || !record.contracts.iter().any(|frozen| frozen.id == *id) {
            bail!(
                "'{id}' names no frozen contract of this stage: `loom stage contracts show` \
                 lists them"
            );
        }
    }
    Ok(())
}

/// The freeze record's entry for `file`, which a replacement must name.
fn frozen_file<'r>(record: &'r FreezeRecord, file: &str) -> Result<&'r FrozenFile> {
    record
        .files
        .iter()
        .find(|frozen| frozen.path == file)
        .with_context(|| {
            format!(
                "'{file}' is no frozen file of this stage: a replacement names a file as \
                 `loom stage contracts show` lists it"
            )
        })
}

/// A replaced file that holds contracts changes all of them, so the filing
/// must dispute every contract of the stage `file` holds, by the map verdict
/// recording and the apply share (`store::contract_files`).
fn check_disputes_every_contract_of(
    stage: &Stage,
    record: &FreezeRecord,
    contract_ids: &[String],
    file: &str,
) -> Result<()> {
    let undisputed: Vec<String> = contract_files(Some(record), &stage.contracts)
        .into_iter()
        .filter(|(id, held)| held == file && !contract_ids.contains(id))
        .map(|(id, _)| id)
        .collect();
    if !undisputed.is_empty() {
        bail!(
            "the replacement for '{file}' changes contracts this filing does not dispute: add \
             `--contract {}`",
            undisputed.join(" --contract ")
        );
    }
    Ok(())
}

/// A replacement fits the inline bound and changes its frozen file.
fn check_replacement_content(replacement: &FileReplacement, frozen_sha256: &str) -> Result<()> {
    let file = &replacement.file;
    let bytes = replacement.content.len();
    if bytes > MAX_REPLACEMENT_BYTES {
        bail!(
            "the replacement for '{file}' is {bytes} bytes, over the \
             {MAX_REPLACEMENT_BYTES}-byte limit"
        );
    }
    if sha256_hex(replacement.content.as_bytes()) == frozen_sha256 {
        bail!("the replacement is identical to the frozen '{file}': there is nothing to change");
    }
    Ok(())
}

/// Refuse while any frozen file differs from the freeze in the stage's
/// worktree, read where the freeze read it (`stage_service::contracts::read_files`).
fn check_no_drift(work_dir: &Path, stage: &Stage, record: &FreezeRecord) -> Result<()> {
    let (worktree_root, working_dir) = stage_site(work_dir, stage)?;
    let drift = frozen_drift(record, &working_dir, &worktree_root)?;
    if drift.is_empty() {
        return Ok(());
    }
    let problems: Vec<String> = drift
        .iter()
        .map(|Drift { path, kind }| {
            let what = match kind {
                DriftKind::Changed => "changed in the worktree".to_string(),
                DriftKind::Missing => "is missing from the worktree".to_string(),
                DriftKind::Unreadable(error) => format!("cannot be read as frozen ({error})"),
            };
            format!(
                "frozen file '{path}' {what}: restore it with `loom stage contracts restore {}` \
                 and file the change as `--replacement {path}=<file>`",
                stage.id
            )
        })
        .collect();
    bail!("{}", problems.join("; "))
}

/// The dispute to record: `kind` with every id checked against what exists
/// now and its evidence re-derived by the daemon.
pub(super) fn confirm(work_dir: &Path, stage: &Stage, kind: DisputeKind) -> Result<Admission> {
    if stage.plan_version != 2 {
        return Ok(Admission::Rejected(format!(
            "stage '{}' is not a plan version 2 stage; only its acceptance criteria, wiring \
             checks and wiring tests can be disputed, with `loom stage dispute-criteria --field`",
            stage.id
        )));
    }
    if let Some(reason) = check_filer(work_dir, stage, &kind)? {
        return Ok(Admission::Rejected(reason));
    }
    if let DisputeKind::FreezeRefusal { problems } = kind {
        return Ok(match check_unfrozen(work_dir, stage)? {
            Some(reason) => Admission::Rejected(reason),
            None => Admission::Accepted(DisputeKind::FreezeRefusal { problems }),
        });
    }
    Ok(match confirm_evidence(work_dir, stage, kind) {
        Ok(kind) => Admission::Accepted(kind),
        Err(error) => Admission::Rejected(format!("{error:#}")),
    })
}

fn confirm_evidence(work_dir: &Path, stage: &Stage, kind: DisputeKind) -> Result<DisputeKind> {
    match kind {
        DisputeKind::Criterion { .. } => {
            bail!("a criterion is disputed with `loom stage dispute-criteria`")
        }
        DisputeKind::FreezeRefusal { .. } => unreachable!("freeze refusals are confirmed above"),
        DisputeKind::Findings { finding_ids, .. } => {
            let open = open_findings(work_dir, &stage.id)?;
            let evidence = select_findings(&open, &finding_ids)?;
            Ok(DisputeKind::Findings {
                finding_ids,
                evidence,
            })
        }
        DisputeKind::Contract {
            contract_ids,
            replacements,
        } => confirm_contract(work_dir, stage, contract_ids, replacements),
        DisputeKind::Integrity { event_ids, .. } => {
            let events = crate::stage_service::observer::integrity_events(work_dir, stage)?;
            let evidence = select_events(&events, &event_ids)?;
            Ok(DisputeKind::Integrity {
                event_ids,
                evidence,
            })
        }
        // Whatever evidence the client sent is dropped: the observed record is
        // it, the host or operator completion's latest observed failure.
        DisputeKind::ImpactTests { test_ids, .. } => confirm_impact(work_dir, stage, test_ids),
    }
}

/// An impact-test dispute of a standard stage, its evidence each named
/// failure as the observed record holds it (an unauthenticated report the
/// adjudicator checks by re-running the selection).
fn confirm_impact(work_dir: &Path, stage: &Stage, test_ids: Vec<String>) -> Result<DisputeKind> {
    if stage.stage_type != StageType::Standard {
        bail!(
            "stage '{}' is not a standard stage; impact-selected tests run only when a \
             standard stage completes",
            stage.id
        );
    }
    for id in &test_ids {
        check_failure_id(id)?;
    }
    let observed = load_observed(work_dir, &stage.id)?;
    let evidence = select_impact(&observed.failures, &test_ids).with_context(|| {
        format!(
            "loom keeps only the failures the latest failing `loom stage complete {}` run \
             reported; dispute an id that run listed as disputable",
            stage.id
        )
    })?;
    Ok(DisputeKind::ImpactTests { test_ids, evidence })
}
