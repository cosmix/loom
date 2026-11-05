//! Sealing a draft into the summary store.
//!
//! `seal` reads the draft from the runtime directory, parses it, checks every link against the
//! project and a plan summary's two levels, and writes the canonical summary file. The link checks and the write run in one hold
//! of the store lock, so a member reseal cannot land between the check and the write. Sealing
//! deletes the draft and the claim of the summary it wrote.

mod levels;
// The frozen contract file clones `Copy` states; a frozen file cannot be edited, so the lint is
// allowed on the module. Drop the attribute if the contract is ever rewritten.
#[cfg(test)]
#[allow(clippy::clone_on_copy)]
mod levels_contract_tests;
#[cfg(test)]
mod levels_tests;
mod links;
mod restate;
#[cfg(test)]
mod restate_contract_tests;
#[cfg(test)]
mod tests;
#[cfg(test)]
mod two_member_contract_tests;

use std::cell::RefCell;
use std::fmt;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use anyhow::{anyhow, Context};

use super::claims::{current_session, delete_claim, draft_rel, list_claims};
use super::evaluate::{evaluate, members_hash};
use super::format::{
    parse_draft, render, Block, FormatError, FrontMatter, SummaryBody, SummaryFile, SummaryKind,
};
use super::store;
use super::write::{read_summary, write_summary_if, WriteOutcome};
use crate::fs::safe_fs::{safe_open_dirfd, safe_remove_in_workdir};
use crate::fs::safe_read::{is_not_found, read_to_string_bounded};
use crate::planning::project::Project;
use crate::planning::runtime::RuntimePaths;
use crate::planning::sections::normalized_hash;
use links::Rules;

/// Largest draft read.
const MAX_DRAFT_BYTES: usize = 256 * 1024;
/// Times a seal retries when the summary file changes between its read and its locked write.
const WRITE_ATTEMPTS: usize = 3;

/// A sealed summary.
#[derive(Debug)]
pub struct SealOutcome {
    /// Sentences in the sealed summary.
    pub sentences: usize,
    /// Repository-relative path of the summary file.
    pub path: PathBuf,
}

/// Why a seal failed.
#[derive(Debug)]
pub enum SealError {
    /// The draft or the request breaks a rule; one entry per problem with its draft line.
    Rejected(Vec<FormatError>),
    /// Reading, locking or writing failed.
    Io(anyhow::Error),
}

impl From<anyhow::Error> for SealError {
    fn from(error: anyhow::Error) -> Self {
        SealError::Io(error)
    }
}

impl fmt::Display for SealError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SealError::Rejected(errors) => {
                let lines: Vec<String> = errors.iter().map(ToString::to_string).collect();
                write!(f, "{}", lines.join("; "))
            }
            SealError::Io(error) => write!(f, "{error:#}"),
        }
    }
}

impl std::error::Error for SealError {}

fn rejected(message: String) -> SealError {
    SealError::Rejected(vec![FormatError { line: 1, message }])
}

/// Seal the draft of summary `id` into the store. Refuses an id claimed by another session; from
/// a terminal (no `$LOOM_PLAN_SESSION`) any claim refuses unless `force`.
pub fn seal(
    root: &Path,
    paths: &RuntimePaths,
    id: &str,
    force: bool,
) -> Result<SealOutcome, SealError> {
    store::validate_id(id)?;
    let project = Project::load(root)?;
    if !store::expected_ids(&project).iter().any(|e| e == id) {
        return Err(rejected(format!("no such summary in this project: {id}")));
    }
    let draft = draft_rel(id);
    let text = read_to_string_bounded(&paths.dir, &draft, MAX_DRAFT_BYTES)
        .with_context(|| format!("cannot read the draft of {id}"))?;
    let claimed = check_claim(paths, id, force)?;
    let body = parse_draft(id, &text).map_err(SealError::Rejected)?;
    let sentences = write_locked(root, paths, &project, id, &body)?;
    if claimed {
        delete_claim(paths, id)?;
    }
    if let Err(error) = remove_draft(paths, &draft) {
        // The summary is already written and its claim released; a leftover draft is harmless,
        // so report it and succeed.
        tracing::warn!(summary = %id, error = %format!("{error:#}"), "sealed draft could not be removed");
    }
    Ok(SealOutcome {
        sentences,
        path: store::summary_path(id),
    })
}

/// Refuse when a claim on `id` is another session's, or a terminal's without `force`;
/// otherwise say whether a claim on `id` exists (the sealer then owns it and deletes it).
/// `--force` overrides a claim only from a terminal: a planning session never does.
fn check_claim(paths: &RuntimePaths, id: &str, force: bool) -> Result<bool, SealError> {
    let claims = list_claims(paths)?;
    let Some(claim) = claims.iter().find(|c| c.summary == id) else {
        return Ok(false);
    };
    let holder = claim.session.as_deref();
    let session = current_session();
    let same_session = matches!((holder, session.as_deref()), (Some(h), Some(me)) if h == me);
    if same_session || (force && session.is_none()) {
        return Ok(true);
    }
    let hint = if session.is_some() {
        ""
    } else {
        "; pass --force"
    };
    Err(rejected(format!(
        "{id} is claimed by {}{hint}",
        holder.unwrap_or("a terminal")
    )))
}

/// Run the link checks and write the summary under one hold of the store lock; returns the
/// sentence count.
fn write_locked(
    root: &Path,
    paths: &RuntimePaths,
    project: &Project,
    id: &str,
    body: &SummaryBody,
) -> Result<usize, SealError> {
    let failure: RefCell<Option<SealError>> = RefCell::new(None);
    for _ in 0..WRITE_ATTEMPTS {
        let current = read_summary(root, id)?.map(|text| normalized_hash(&text));
        let build = || {
            build_text(project, id, body).map_err(|error| {
                *failure.borrow_mut() = Some(error);
                anyhow!("seal of {id} refused")
            })
        };
        let outcome = write_summary_if(root, paths, id, current.as_deref(), build, None);
        if let Some(error) = failure.take() {
            return Err(error);
        }
        if outcome? == WriteOutcome::Written {
            return Ok(sentence_count(body));
        }
    }
    Err(SealError::Io(anyhow!(
        "summary {id} kept changing while it was sealed"
    )))
}

fn sentence_count(body: &SummaryBody) -> usize {
    let counts = body.blocks.iter().map(|block| match block {
        Block::Paragraph(sentences) => sentences.len(),
        Block::Heading { .. } => 0,
    });
    counts.sum()
}

/// Check the links of `body` and render the summary file text; runs under the store lock.
fn build_text(project: &Project, id: &str, body: &SummaryBody) -> Result<String, SealError> {
    let overview = store::is_overview(id);
    let evaluation = overview.then(|| evaluate(project, &[]));
    let rules = Rules {
        project,
        id,
        evaluation: evaluation.as_ref(),
    };
    let (cited, mut errors) = links::check(&rules, body);
    errors.extend(levels::check(id, body));
    errors.sort_by_key(|e| e.line);
    if !errors.is_empty() {
        return Err(SealError::Rejected(errors));
    }
    let front = if overview {
        let members = rules.members();
        let hashes = members.iter().filter_map(|member| {
            let eval = evaluation.as_ref()?.summaries.get(member)?;
            Some((member.clone(), members_hash(eval.file.as_ref()?)))
        });
        FrontMatter {
            kind: SummaryKind::Overview,
            of: id.to_string(),
            file: None,
            cites: cited.cites,
            members: hashes.collect(),
        }
    } else {
        let source = store::source_of(project, id)
            .ok_or_else(|| rejected(format!("{id} has no source document")))?;
        let file = project
            .content_hash(&source)
            .ok_or_else(|| rejected(format!("{} cannot be read", source.display())))?;
        FrontMatter {
            kind: SummaryKind::Document,
            of: source.to_string_lossy().into_owned(),
            file: Some(file.to_string()),
            cites: cited.cites,
            members: Default::default(),
        }
    };
    let file = SummaryFile {
        front,
        title: body.title.clone(),
        blocks: body.blocks.clone(),
    };
    Ok(render(&file))
}

/// Delete the draft; an absent draft is fine.
fn remove_draft(paths: &RuntimePaths, draft_rel: &Path) -> anyhow::Result<()> {
    let dirfd = safe_open_dirfd(&paths.dir)?;
    match safe_remove_in_workdir(dirfd.as_raw_fd(), draft_rel) {
        Err(error) if !is_not_found(&error) => Err(error),
        _ => Ok(()),
    }
}
