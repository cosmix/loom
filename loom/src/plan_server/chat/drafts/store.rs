//! Reading, listing and deleting the draft files under `<runtime>/kickoffs`.
//!
//! Only `<canonical uuid>.json` names are ever read, so a temp file or a stray name is
//! ignored, and a session id never becomes a path before it passed [`canonical`].

use std::fs;
use std::os::fd::AsRawFd;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};

use super::{canonical, Draft, DIR, MAX_FILE_BYTES};
use crate::fs::plan_lifecycle::{DONE_PREFIX, IN_PROGRESS_PREFIX, RETIRED_PREFIX};
use crate::fs::safe_fs::{safe_open_dirfd, safe_remove_in_workdir};
use crate::fs::safe_read::read_bounded;
use crate::plan_server::context::PlanContext;
use crate::planning::slug::plan_slug;

/// How long a draft is kept after its last change.
const KEEP_DAYS: i64 = 30;

pub(super) struct Files {
    dir: PathBuf,
    root: PathBuf,
}

impl Files {
    pub(super) fn new(ctx: &PlanContext) -> Files {
        Files {
            dir: ctx.runtime().dir.join(DIR),
            root: ctx.root().to_path_buf(),
        }
    }

    /// The file of `session`, which must be canonical.
    pub(super) fn path(&self, session: &str) -> PathBuf {
        self.dir.join(format!("{session}.json"))
    }

    /// The draft of `session`. A missing file is `None`; one that cannot be read, exceeds the
    /// read cap, does not parse or breaks a cap is skipped with a warning and left in place.
    pub(super) fn load(&self, session: &str) -> Option<Draft> {
        if !canonical(session) || fs::symlink_metadata(self.path(session)).is_err() {
            return None;
        }
        let name = format!("{session}.json");
        let bytes = match read_bounded(&self.dir, Path::new(&name), MAX_FILE_BYTES) {
            Ok(bytes) => bytes,
            Err(error) => return skipped(session, &format!("{error:#}")),
        };
        let draft: Draft = match serde_json::from_slice(&bytes) {
            Ok(draft) => draft,
            Err(error) => return skipped(session, &error),
        };
        if draft.session != session || !draft.within_caps() {
            return skipped(session, &"it names another session or breaks a limit");
        }
        Some(draft)
    }

    /// The session ids of the draft files, in no order.
    pub(super) fn sessions(&self) -> Vec<String> {
        let Ok(entries) = fs::read_dir(&self.dir) else {
            return Vec::new();
        };
        let names = entries.filter_map(|entry| entry.ok()?.file_name().into_string().ok());
        let sessions = names.filter_map(|name| name.strip_suffix(".json").map(str::to_owned));
        sessions.filter(|session| canonical(session)).collect()
    }

    /// Delete the file of `session`; false when it was not there or could not go.
    pub(super) fn remove(&self, session: &str) -> bool {
        if !canonical(session) || fs::symlink_metadata(self.path(session)).is_err() {
            return false;
        }
        let name = format!("{session}.json");
        let removed = safe_open_dirfd(&self.dir)
            .and_then(|dirfd| safe_remove_in_workdir(dirfd.as_raw_fd(), Path::new(&name)));
        if let Err(error) = &removed {
            tracing::warn!(session, "kickoff draft not removed: {error:#}");
        }
        removed.is_ok()
    }

    /// The confirmed document exists, under its own name or any lifecycle spelling of a plan
    /// (`IN_PROGRESS-`, `DONE-`, `RETIRED-`, or moved to `archive/`): the work has landed.
    pub(super) fn landed(&self, draft: &Draft) -> bool {
        let Some(target) = draft.target.as_deref() else {
            return false;
        };
        let exists = |rel: &Path| fs::symlink_metadata(self.root.join(rel)).is_ok();
        spellings(Path::new(target)).iter().any(|rel| exists(rel))
    }

    /// Delete every draft last changed more than [`KEEP_DAYS`] before `now`. It runs once at
    /// server start, when no session runs, so the stored `live` flag (the state the last server
    /// left) says nothing and is ignored.
    pub(super) fn sweep(&self, now: DateTime<Utc>) -> usize {
        let cutoff = now - chrono::Duration::days(KEEP_DAYS);
        let expired = |draft: &Draft| draft.updated() < cutoff;
        let sessions = self.sessions();
        let drafts = sessions.iter().filter_map(|session| self.load(session));
        let old: Vec<String> = drafts.filter(expired).map(|draft| draft.session).collect();
        old.iter().filter(|session| self.remove(session)).count()
    }
}

fn skipped(session: &str, why: &dyn std::fmt::Display) -> Option<Draft> {
    tracing::warn!(session, "kickoff draft skipped: {why}");
    None
}

/// `target` and, for a plan, every other file name the plan can have.
fn spellings(target: &Path) -> Vec<PathBuf> {
    let mut all = vec![target.to_path_buf()];
    let (Some(dir), Some(name)) = (target.parent(), target.file_name().and_then(|n| n.to_str()))
    else {
        return all;
    };
    let Some(slug) = plan_slug(name) else {
        return all;
    };
    for prefix in ["", IN_PROGRESS_PREFIX, DONE_PREFIX, RETIRED_PREFIX] {
        let file = format!("{prefix}PLAN-{slug}.md");
        all.push(dir.join(&file));
        all.push(dir.join("archive").join(file));
    }
    all
}
