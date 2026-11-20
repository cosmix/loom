//! Per-session limits on authoring new planning documents.

use std::collections::HashSet;
use std::path::{Component, Path, PathBuf};

use anyhow::{ensure, Context, Result};

use super::kind::SessionDir;
use crate::fs::safe_fs::{safe_open_dirfd, safe_replace_outside_workdir};
use crate::fs::safe_read::{is_not_found, read_to_string_bounded};
use crate::planning::kinds::is_feature_name;
use crate::planning::paths::symlink_component;

mod arm;
pub use arm::{arm_kickoff, ArmRefusal};

#[path = "scope_snapshot.rs"]
mod scope_snapshot;
use scope_snapshot::{entries, snapshot};

pub const SCOPE_FILE: &str = "scope.json";
const MAX_SCOPE_BYTES: usize = 4 * 1024 * 1024;

#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum KickoffSize {
    Feature,
    Plan,
}

/// The one target a confirmed kickoff may create.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KickoffGrant {
    pub size: KickoffSize,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KickoffScope {
    /// The size the session was started with; confirming any other is refused.
    pub size: KickoffSize,
    pub existing: Vec<PathBuf>,
    pub grant: Option<KickoffGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionScope {
    Open,
    Kickoff(KickoffScope),
    Team {
        run: TeamRun,
        documents: Vec<PathBuf>,
        bundles: Vec<PathBuf>,
    },
}

/// Which offer a team scope was started for.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub enum TeamRun {
    /// Draft all missing sections or plans: every document is new.
    Draft,
    /// Address the findings: existing documents, and the ledger only the lead writes.
    Address { ledger: PathBuf },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmedTarget {
    pub document: PathBuf,
    pub bundle: Option<PathBuf>,
}

impl SessionScope {
    pub fn kickoff(project_root: &Path, size: KickoffSize) -> Result<SessionScope> {
        Ok(Self::Kickoff(KickoffScope {
            size,
            existing: snapshot(project_root)?,
            grant: None,
        }))
    }

    /// The scope of a continued kickoff whose target is already confirmed. The grant is held
    /// at once, so the target and bundle stay writable although they exist; they are the
    /// draft's own, so no existing-target check runs and `existing` stays empty (only
    /// `arm_kickoff` reads it, and it refuses a scope that already has a grant).
    pub fn kickoff_resumed(size: KickoffSize, name: &str) -> Result<SessionScope> {
        ensure!(is_feature_name(name), "Invalid kickoff grant name");
        Ok(Self::Kickoff(KickoffScope {
            size,
            existing: Vec::new(),
            grant: Some(KickoffGrant {
                size,
                name: name.to_owned(),
            }),
        }))
    }

    /// The scope of a draft-all run.
    pub fn team(documents: Vec<PathBuf>, bundles: Vec<PathBuf>) -> Result<SessionScope> {
        Self::checked_team(TeamRun::Draft, documents, bundles)
    }

    /// The scope of an address run: the documents it may edit, `ledger` among them.
    pub fn address(documents: Vec<PathBuf>, ledger: PathBuf) -> Result<SessionScope> {
        Self::checked_team(TeamRun::Address { ledger }, documents, Vec::new())
    }

    fn checked_team(
        run: TeamRun,
        documents: Vec<PathBuf>,
        bundles: Vec<PathBuf>,
    ) -> Result<SessionScope> {
        if let TeamRun::Address { ledger } = &run {
            ensure!(
                documents.contains(ledger),
                "An address scope lists its ledger among its documents"
            );
        }
        ensure!(
            !documents.is_empty(),
            "A team scope needs at least one document"
        );
        ensure!(
            documents.len() <= 64,
            "A team scope allows at most 64 documents"
        );
        let mut seen = HashSet::new();
        for path in documents.iter().chain(&bundles) {
            ensure!(
                plain_doc_path(path),
                "Invalid team scope path: {}",
                path.display()
            );
            ensure!(
                seen.insert(path),
                "Duplicate team scope path: {}",
                path.display()
            );
        }
        Ok(Self::Team {
            run,
            documents,
            bundles,
        })
    }

    pub fn allows_write(&self, rel: &Path) -> Result<(), String> {
        match self {
            Self::Open => Ok(()),
            Self::Kickoff(KickoffScope { grant: None, .. }) => {
                Err("No document may be written until the size and name are confirmed.".into())
            }
            Self::Kickoff(KickoffScope {
                grant: Some(grant), ..
            }) => {
                let target = kickoff_target(grant.size, &grant.name);
                if rel == target.document
                    || target
                        .bundle
                        .as_ref()
                        .is_some_and(|bundle| inside(rel, bundle))
                {
                    Ok(())
                } else {
                    let bundle = target
                        .bundle
                        .as_ref()
                        .map(|bundle| format!(" and paths inside {}", bundle.display()))
                        .unwrap_or_default();
                    Err(format!(
                        "This kickoff may write only {}{bundle}.",
                        target.document.display()
                    ))
                }
            }
            Self::Team {
                documents, bundles, ..
            } => {
                if documents.iter().any(|path| path == rel)
                    || bundles.iter().any(|bundle| inside(rel, bundle))
                {
                    Ok(())
                } else {
                    Err("This team may write only the documents this run was started for and paths inside their brief bundles.".into())
                }
            }
        }
    }

    pub fn peer_tools(&self) -> bool {
        matches!(self, Self::Team { .. })
    }
}

fn plain_doc_path(path: &Path) -> bool {
    path.starts_with("doc")
        && path != Path::new("doc")
        && !path.starts_with("doc/loom")
        && path.components().all(|part| matches!(part, Component::Normal(_)))
        // Path::components normalizes interior dots, so inspect the spelling too.
        && !path.as_os_str().as_encoded_bytes()
            .split(|byte| *byte == b'/')
            .any(|part| matches!(part, b"." | b".."))
}

fn inside(path: &Path, directory: &Path) -> bool {
    path != directory && path.starts_with(directory)
}

/// The document, and for a plan its brief bundle, a kickoff of `size` named `name` writes.
pub fn kickoff_target(size: KickoffSize, name: &str) -> ArmedTarget {
    match size {
        KickoffSize::Feature => ArmedTarget {
            document: Path::new("doc/product").join(name).join("index.md"),
            bundle: None,
        },
        KickoffSize::Plan => ArmedTarget {
            document: Path::new("doc/plans").join(format!("PLAN-{name}.md")),
            bundle: Some(Path::new("doc/plans/briefs").join(name)),
        },
    }
}

pub fn load(dir: &SessionDir) -> Result<SessionScope> {
    let text = match read_to_string_bounded(dir.path(), Path::new(SCOPE_FILE), MAX_SCOPE_BYTES) {
        Ok(text) => text,
        Err(error) if is_not_found(&error) => return Ok(SessionScope::Open),
        Err(error) => return Err(error.context(format!("cannot read {}", dir.scope().display()))),
    };
    let value: serde_json::Value = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a session scope", dir.scope().display()))?;
    // serde's `deny_unknown_fields` does not apply to a unit variant of a tagged enum.
    ensure!(
        value.get("scope") != Some(&serde_json::json!("open"))
            || value.as_object().is_some_and(|fields| fields.len() == 1),
        "{} is not a session scope: unknown fields",
        dir.scope().display()
    );
    let scope: SessionScope = serde_json::from_value(value)
        .with_context(|| format!("{} is not a session scope", dir.scope().display()))?;
    match &scope {
        SessionScope::Team {
            run,
            documents,
            bundles,
        } => {
            SessionScope::checked_team(run.clone(), documents.clone(), bundles.clone())?;
        }
        SessionScope::Kickoff(KickoffScope {
            grant: Some(grant), ..
        }) => {
            ensure!(is_feature_name(&grant.name), "Invalid kickoff grant name");
        }
        _ => {}
    }
    Ok(scope)
}

pub fn write(dir: &SessionDir, scope: &SessionScope) -> Result<()> {
    let bytes = serde_json::to_vec_pretty(scope)?;
    ensure!(
        bytes.len() <= MAX_SCOPE_BYTES,
        "Session scope exceeds {MAX_SCOPE_BYTES} bytes"
    );
    let _fd = safe_open_dirfd(dir.path())?;
    ensure!(
        symlink_component(dir.path(), Path::new(SCOPE_FILE))?.is_none(),
        "Refusing a symlinked session scope"
    );
    safe_replace_outside_workdir(&dir.scope(), dir.path(), &bytes)
        .with_context(|| format!("cannot write {}", dir.scope().display()))
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;
