//! Assertion-free fixture helpers of the guard read tests: the credential files and the seeded tree.

use std::fs;
use std::path::{Path, PathBuf};

use anyhow::Result;

use crate::plan_session::kind::SessionKind;

pub(super) const CREDENTIALS: &[&str] = &[
    ".ssh/id_rsa",
    ".aws/credentials",
    ".config/gcloud/credentials.db",
    ".gnupg/private.key",
    ".claude/.credentials.json",
    ".config/gh/hosts.yml",
    ".netrc",
    ".npmrc",
    ".git-credentials",
    ".docker/config.json",
    ".kube/config",
    ".pypirc",
    ".cargo/credentials.toml",
    ".cargo/credentials",
    ".codex/auth.json",
];

pub(super) fn put(path: impl AsRef<Path>) -> Result<()> {
    let path = path.as_ref();
    fs::create_dir_all(path.parent().unwrap())?;
    fs::write(path, "# Fixture\n")?;
    Ok(())
}

pub(super) fn seed_tree(root: &Path, home: &Path) -> Result<()> {
    for dir in [
        ".git",
        "loom",
        ".claude",
        ".loom/work",
        "doc/product",
        "doc/notes",
        "doc/plans/archive",
        "doc/plans/briefs/a",
        "doc/plans/briefs/b",
        "doc/loom/knowledge",
        "doc/loom/summaries",
    ] {
        fs::create_dir_all(root.join(dir))?;
    }
    for file in [
        "README.md",
        "doc/plans/PLAN-a.md",
        "doc/plans/IN_PROGRESS-PLAN-b.md",
        "doc/plans/REVIEW-PLAN-b.md",
        "doc/plans/archive/DONE-PLAN-c.md",
        "doc/plans/briefs/b/x.md",
        "doc/plans/briefs/a/y.md",
        "loom/src/lib.rs",
        ".loom/work/admin.token",
        ".loom/work/user.token",
        ".loom/work/completion-attestation.key",
        ".loom/work/terminal-history/log",
    ] {
        put(root.join(file))?;
    }
    for credential in CREDENTIALS {
        put(home.join(credential))?;
    }
    Ok(())
}

pub(super) fn kinds() -> [SessionKind; 3] {
    [
        SessionKind::Authoring,
        SessionKind::Summary,
        SessionKind::Pressure {
            plan: PathBuf::from("doc/plans/PLAN-a.md"),
        },
    ]
}
