//! Which commit a settle made, and what is left to do about it. Git's exit status does not say: a
//! `post-commit` hook runs after the commit exists, and though git ignores its exit status, one
//! that runs past the deadline, prints past the output limit or leaves a descendant holding git's
//! pipes ends the run badly with the commit made; a hook may also commit on top of it. HEAD is
//! read before the commit and after it: HEAD that did not move means nothing was committed, and
//! the settle's own commit is the one git added on top of the old HEAD with the settle's message.

use std::path::Path;

use crate::git::runner::run_git;
use crate::git::user_git::{head, run_literal, IndexBefore};

use super::settle_commit::{refusal, Recorded};

/// How many commits a `post-commit` hook may add on top of the settle's own before the settle's is
/// no longer looked for. Revisit if projects' hooks routinely commit more than this.
const OWN_COMMIT_DEPTH: usize = 8;

/// The settle's own commit, `before` being HEAD before it: the oldest commit HEAD's first parents
/// added since `before` (among the newest [`OWN_COMMIT_DEPTH`]), when its only parent is `before`
/// (none on an unborn branch) and its subject is `message`. `None` when HEAD holds no such
/// commit: git did not commit, a hook moved HEAD elsewhere or rewrote the message.
fn own_commit(root: &Path, before: Option<&str>, message: &str) -> Option<String> {
    let range = before.map_or_else(|| "HEAD".to_owned(), |head| format!("{head}..HEAD"));
    let depth = format!("--max-count={OWN_COMMIT_DEPTH}");
    // `--encoding`: the subject as written, whatever `i18n.logOutputEncoding` says.
    let args = [
        "log",
        "--first-parent",
        "--no-show-signature",
        "--encoding=UTF-8",
        depth.as_str(),
        "-z",
        "--format=%H %P%n%s",
        range.as_str(),
        "--",
    ];
    let output = run_git(&args, root).ok().filter(|o| o.status.success())?;
    let listing = String::from_utf8(output.stdout).ok()?;
    let oldest = listing.split('\0').rfind(|record| !record.is_empty())?;
    let (ids, subject) = oldest.split_once('\n')?;
    let mut ids = ids.split(' ').filter(|id| !id.is_empty());
    let commit = ids.next()?;
    let parents: Vec<&str> = ids.collect();
    let on_before = parents == before.into_iter().collect::<Vec<_>>();
    (on_before && subject == message).then(|| commit.to_owned())
}

/// One commit as git was asked for it: the index before it, its message and paths, and the
/// untracked paths added for it.
pub(super) struct Made<'a> {
    pub index: &'a IndexBefore,
    pub message: &'a str,
    pub paths: &'a [&'a str],
    pub added: &'a [String],
}

/// What a commit came to, `ran` being how git ended. HEAD where it was means nothing was
/// committed: the added paths are unstaged. HEAD moved means git made a commit whatever its exit
/// status said. When it is the settle's own ([`own_commit`]), or git succeeded, the index is put
/// right ([`IndexBefore::reset`]), which also unstages an added path the commit does not hold.
/// Otherwise the index is left as it was but for the added paths HEAD does not hold.
pub(super) fn after_commit(root: &Path, made: &Made<'_>, ran: Result<(), String>) -> Recorded {
    let before = made.index.head();
    // HEAD that cannot be read counts as moved: nothing is unstaged on a guess.
    let moved = head(root).map_or(true, |after| after.as_deref() != before);
    if !moved {
        let reason = ran.err().unwrap_or_else(|| {
            "git commit exited with 0, but HEAD is where it was: a hook may have undone the commit"
                .to_owned()
        });
        return Recorded::NotCommitted(unstage(root, made.added, reason));
    }
    let own = own_commit(root, before, made.message);
    if own.is_none() {
        if let Err(reason) = ran {
            let reason = format!(
                "{reason}; HEAD moved to a commit that is not the settle's, so the index was not \
                 reset to it"
            );
            return Recorded::NotCommitted(unstage(root, &unheld(root, made.added), reason));
        }
    }
    let late = ran
        .err()
        .map(|reason| format!("The commit was made, but {reason}."));
    let unfound = own.is_none().then(|| {
        "The commit was made, but HEAD does not show it with the settle's message, so its id is \
         not known."
            .to_owned()
    });
    let commit = own.as_deref().unwrap_or("HEAD");
    let reset = made.index.reset(root, made.paths, commit).notes();
    let notes: Vec<String> = late.into_iter().chain(unfound).chain(reset).collect();
    Recorded::Committed {
        commit: own,
        note: (!notes.is_empty()).then(|| notes.join(" ")),
    }
}

/// The paths among `added` HEAD does not hold: untracked before the settle added them, and in no
/// commit since. A git that cannot answer keeps the path staged.
fn unheld(root: &Path, added: &[String]) -> Vec<String> {
    added
        .iter()
        .filter(|path| {
            let held = format!("HEAD:./{path}");
            run_git(&["cat-file", "-e", &held], root).is_ok_and(|output| !output.status.success())
        })
        .cloned()
        .collect()
}

/// Unstage `added`, paths a settle added that no commit holds; `reason` says why it ended.
pub(super) fn unstage(root: &Path, added: &[String], reason: String) -> String {
    if added.is_empty() {
        return reason;
    }
    // `-f`: the entries are this settle's own, whatever a hook did to the files meanwhile.
    let rm = ["rm", "--cached", "-q", "-f", "--ignore-unmatch", "--"];
    match run_literal(root, &rm, added) {
        Ok(output) if output.status.success() => reason,
        Ok(output) => format!("{reason}; {}", refusal("git rm --cached", &output)),
        Err(error) => format!(
            "{reason}; git rm --cached could not run: {}",
            error.root_cause()
        ),
    }
}
