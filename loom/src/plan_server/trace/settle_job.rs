//! One settle commit as the settle worker runs it: check the document and a plan's briefs still
//! hold what the settle took, commit them, and say whether the commit holds that.
//!
//! A commit hook may rewrite a settled file (a formatter, say) into the commit, and a hook or the
//! reader may change it beside the commit while the hooks run. The `Settled:` line then describes
//! the text from before, so the document reads as changed since it was settled, and the reader is
//! told to settle the version now on disk. A commit that holds other text than the settle took is
//! a hook's doing; a commit that holds it beside a work tree that does not is not told apart from
//! the reader's edit. Loom never marks the line again on its own: the line is the reader's claim.
//! Nor does it amend: after a hook, HEAD may be someone else's.

use std::path::{Path, PathBuf};

use crate::context::source_graph::body_hash as sha256;
use crate::plan_server::ids::BrowserId;
use crate::plan_server::wire::trace::{SettleCommitState, SettleCommitView};
use crate::planning::discover::read_doc_bytes;
use crate::planning::sections::normalized_hash;

use super::settle_commit::{committed_bytes, record, Limits, Recorded, SettleCommit};

const EDITED_FIRST: &str = "the document changed before it could be committed; settle again";
const BRIEFS_EDITED_FIRST: &str =
    "the plan's briefs changed before they could be committed; settle again";
const HOOK_CHANGED: &str =
    "A commit hook changed the document after it was settled. Settle this version to record it.";
const CHANGED_AFTER: &str =
    "The document changed after it was settled. Settle this version to record it.";
/// What every browser but the settling one reads for a reason that carries git's or a hook's
/// words, which reach the settling browser alone.
const FAILED_FOR_OTHERS: &str = "the commit failed";
const NOTED_FOR_OTHERS: &str = "git reported a problem with the commit";

/// One queued settle commit.
pub(super) struct SettleJob {
    /// The browser that settled: the one that reads git's words when the commit goes wrong.
    pub browser: BrowserId,
    pub feature: String,
    /// The document, repository-relative (`doc/...`).
    pub doc: PathBuf,
    /// A plan's brief files.
    pub briefs: Vec<PathBuf>,
    /// What the settle took of each brief, in `briefs`' order.
    pub seen: Vec<BriefSeen>,
    /// An unsettle, which commits the document alone.
    pub clear: bool,
    /// The hash of the text the settle wrote.
    pub written: String,
}

/// A brief as a settle took it: the hash of the cached text the `Settled:` line's body hash
/// covered, or, for a brief the project holds no text for (not Markdown, or unreadable), the
/// hash of its bytes when the line was written.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) enum BriefSeen {
    Text(String),
    Bytes(Option<String>),
}

impl BriefSeen {
    /// What a settle takes of the brief at `path`; `cached` is the hash of the project's text of
    /// it, which the line's body hash covered.
    pub(super) fn of(root: &Path, path: &Path, cached: Option<String>) -> BriefSeen {
        match cached {
            Some(hash) => BriefSeen::Text(hash),
            None => BriefSeen::Bytes(bytes_hash(root, path)),
        }
    }

    /// Whether `bytes`, a brief's (`None`: no such file, or unreadable), hold what the settle took.
    fn matches(&self, bytes: Option<&[u8]>) -> bool {
        match self {
            BriefSeen::Text(hash) => text_hash(bytes).is_some_and(|text| text == *hash),
            BriefSeen::Bytes(hash) => bytes.map(sha256) == *hash,
        }
    }
}

fn bytes_hash(root: &Path, path: &Path) -> Option<String> {
    on_disk(root, path).as_deref().map(sha256)
}

/// The file at `path` on disk, `None` when it cannot be read.
fn on_disk(root: &Path, path: &Path) -> Option<Vec<u8>> {
    read_doc_bytes(root, path).ok()
}

/// The normalized hash of `bytes` as text, as the settle hashes a document; `None` for no file or
/// one that is not UTF-8.
fn text_hash(bytes: Option<&[u8]>) -> Option<String> {
    let text = std::str::from_utf8(bytes?).ok()?;
    Some(normalized_hash(text))
}

impl SettleJob {
    fn commit(&self) -> SettleCommit<'_> {
        SettleCommit {
            feature: &self.feature,
            doc: &self.doc,
            briefs: &self.briefs,
            clear: self.clear,
        }
    }

    /// Whether `bytes` (`None`: no such file) are the text the settle wrote.
    fn is_written(&self, bytes: Option<&[u8]>) -> bool {
        text_hash(bytes).is_some_and(|hash| hash == self.written)
    }

    /// The briefs a settle commits, each with what the settle took of it; an unsettle commits none.
    fn committed_briefs(&self) -> impl Iterator<Item = (&PathBuf, &BriefSeen)> {
        let briefs: &[PathBuf] = if self.clear { &[] } else { &self.briefs };
        briefs.iter().zip(&self.seen)
    }

    /// Whether the document holds the text the settle wrote.
    fn doc_unchanged(&self, root: &Path) -> bool {
        self.is_written(on_disk(root, &self.doc).as_deref())
    }

    /// Whether every brief a settle commits holds what the settle took.
    fn briefs_unchanged(&self, root: &Path) -> bool {
        self.committed_briefs()
            .all(|(path, seen)| seen.matches(on_disk(root, path).as_deref()))
    }

    /// Whether commit `sha` holds what the settle took: the document's text and each brief. Only
    /// a difference git shows counts; a git that cannot answer claims none.
    fn commit_holds_settled(&self, root: &Path, sha: &str) -> bool {
        let held = |path: &Path, holds: &dyn Fn(Option<&[u8]>) -> bool| {
            committed_bytes(root, sha, path).map_or(true, |bytes| holds(bytes.as_deref()))
        };
        held(&self.doc, &|bytes| self.is_written(bytes))
            && self
                .committed_briefs()
                .all(|(path, seen)| held(path, &|bytes| seen.matches(bytes)))
    }

    /// What changed since the settle, once its commit is made: a commit `sha` holding other text
    /// than the settle took was changed by a hook; a work tree that no longer holds it changed
    /// beside the commit, by a hook or the reader. `sha` unknown, only the work tree is asked.
    fn changed_since(&self, root: &Path, sha: Option<&str>) -> Option<&'static str> {
        if sha.is_some_and(|sha| !self.commit_holds_settled(root, sha)) {
            return Some(HOOK_CHANGED);
        }
        (!self.doc_unchanged(root) || !self.briefs_unchanged(root)).then_some(CHANGED_AFTER)
    }
}

/// Why a job ended as it did: loom's own words, which every browser may read, or words that carry
/// git's or a hook's output, which reach the settling browser alone.
enum Reason {
    Own(&'static str),
    Git(String),
}

/// How a job ended, but for its id and path.
struct End {
    state: SettleCommitState,
    commit: Option<String>,
    reason: Option<Reason>,
}

impl End {
    fn not_committed(reason: Reason) -> End {
        End {
            state: SettleCommitState::NotCommitted,
            commit: None,
            reason: Some(reason),
        }
    }
}

/// How job `id` ended, as the `settle_commit` message says it: to the settling browser, and to
/// every other one.
pub(super) struct Outcome {
    pub settler: SettleCommitView,
    pub others: SettleCommitView,
}

/// Run job `id` in the repository holding `root`, under `limits`.
pub(super) fn run(root: &Path, id: &str, job: &SettleJob, limits: Limits) -> Outcome {
    outcome(id, job, commit_job(root, job, limits))
}

/// The end of job `id` when it never ran or its run failed, `reason` saying why in loom's own
/// words: its line is written, and nothing says it was committed.
pub(super) fn not_run(id: &str, job: &SettleJob, reason: &'static str) -> Outcome {
    outcome(id, job, End::not_committed(Reason::Own(reason)))
}

fn outcome(id: &str, job: &SettleJob, end: End) -> Outcome {
    let End {
        state,
        commit,
        reason,
    } = end;
    let view = |reason: Option<String>| SettleCommitView {
        job: id.to_owned(),
        path: job.doc.to_string_lossy().into_owned(),
        state,
        commit: commit.clone(),
        reason,
    };
    let (settler, others) = match reason {
        None => (None, None),
        Some(Reason::Own(words)) => (Some(words.to_owned()), Some(words.to_owned())),
        Some(Reason::Git(words)) => {
            let shared = match state {
                SettleCommitState::Committed => NOTED_FOR_OTHERS,
                SettleCommitState::Unchanged | SettleCommitState::NotCommitted => FAILED_FOR_OTHERS,
            };
            (Some(words), Some(shared.to_owned()))
        }
    };
    Outcome {
        settler: view(settler),
        others: view(others),
    }
}

fn commit_job(root: &Path, job: &SettleJob, limits: Limits) -> End {
    // An edit that arrived since the write would be committed under the settle's message, beside
    // a line that describes the text before it.
    if !job.doc_unchanged(root) {
        return End::not_committed(Reason::Own(EDITED_FIRST));
    }
    if !job.briefs_unchanged(root) {
        return End::not_committed(Reason::Own(BRIEFS_EDITED_FIRST));
    }
    match record(root, &job.commit(), limits) {
        Recorded::Committed { commit, note } => {
            let reason = match (note, job.changed_since(root, commit.as_deref())) {
                (None, None) => None,
                (None, Some(changed)) => Some(Reason::Own(changed)),
                (Some(note), None) => Some(Reason::Git(note)),
                (Some(note), Some(changed)) => Some(Reason::Git(format!("{note} {changed}"))),
            };
            End {
                state: SettleCommitState::Committed,
                commit,
                reason,
            }
        }
        Recorded::Unchanged => End {
            state: SettleCommitState::Unchanged,
            commit: None,
            reason: None,
        },
        Recorded::NotCommitted(reason) => End::not_committed(Reason::Git(reason)),
    }
}
