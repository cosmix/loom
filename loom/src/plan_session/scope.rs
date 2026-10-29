//! Per-session limits on authoring new planning documents.

use std::collections::HashSet;
use std::fmt;
use std::path::{Component, Path, PathBuf};

use anyhow::{ensure, Context, Result};

use super::kind::SessionDir;
use crate::fs::safe_fs::{safe_open_dirfd, safe_replace_outside_workdir};
use crate::fs::safe_read::{is_not_found, read_to_string_bounded};
use crate::planning::kinds::is_feature_name;
use crate::planning::paths::symlink_component;
use crate::planning::slug::{plan_home, plan_slug};

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
    pub existing: Vec<PathBuf>,
    pub grant: Option<KickoffGrant>,
}

#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(tag = "scope", rename_all = "snake_case", deny_unknown_fields)]
pub enum SessionScope {
    Open,
    Kickoff(KickoffScope),
    Team {
        documents: Vec<PathBuf>,
        bundles: Vec<PathBuf>,
    },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArmedTarget {
    pub document: PathBuf,
    pub bundle: Option<PathBuf>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArmRefusal {
    NotKickoff,
    AlreadyConfirmed,
    InvalidName(String),
    Exists(PathBuf),
    Unreadable(String),
}

impl fmt::Display for ArmRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotKickoff => write!(f, "This session is not a kickoff session."),
            Self::AlreadyConfirmed => write!(f, "This kickoff already has a confirmed target."),
            Self::InvalidName(name) => write!(f, "{name:?} is not a valid feature or plan name."),
            Self::Exists(path) => write!(f, "The target already exists at {}.", path.display()),
            Self::Unreadable(reason) => write!(f, "Cannot confirm the kickoff: {reason}."),
        }
    }
}

impl std::error::Error for ArmRefusal {}

impl SessionScope {
    pub fn kickoff(project_root: &Path) -> Result<SessionScope> {
        Ok(Self::Kickoff(KickoffScope {
            existing: snapshot(project_root)?,
            grant: None,
        }))
    }

    pub fn team(documents: Vec<PathBuf>, bundles: Vec<PathBuf>) -> Result<SessionScope> {
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
        Ok(Self::Team { documents, bundles })
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
                let target = target(grant.size, &grant.name);
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
            Self::Team { documents, bundles } => {
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

fn target(size: KickoffSize, name: &str) -> ArmedTarget {
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
        SessionScope::Team { documents, bundles } => {
            SessionScope::team(documents.clone(), bundles.clone())?;
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

pub fn arm_kickoff(
    dir: &SessionDir,
    project_root: &Path,
    size: KickoffSize,
    name: &str,
) -> Result<ArmedTarget, ArmRefusal> {
    let mut scope = load(dir).map_err(unreadable)?;
    let SessionScope::Kickoff(kickoff) = &mut scope else {
        return Err(ArmRefusal::NotKickoff);
    };
    if kickoff.grant.is_some() {
        return Err(ArmRefusal::AlreadyConfirmed);
    }
    if !is_feature_name(name) {
        return Err(ArmRefusal::InvalidName(name.to_owned()));
    }
    let target = target(size, name);
    refuse_existing(&kickoff.existing, size, name, &target)?;
    refuse_existing(
        &snapshot(project_root).map_err(unreadable)?,
        size,
        name,
        &target,
    )?;
    refuse_live(project_root, size, name, &target)?;
    kickoff.grant = Some(KickoffGrant {
        size,
        name: name.to_owned(),
    });
    write(dir, &scope).map_err(unreadable)?;
    Ok(target)
}

fn unreadable(error: anyhow::Error) -> ArmRefusal {
    ArmRefusal::Unreadable(format!("{error:#}"))
}

fn refuse_existing(
    existing: &[PathBuf],
    size: KickoffSize,
    name: &str,
    target: &ArmedTarget,
) -> Result<(), ArmRefusal> {
    let document = PathBuf::from(target.document.as_os_str().to_ascii_lowercase());
    let bundle = target
        .bundle
        .as_ref()
        .map(|path| PathBuf::from(path.as_os_str().to_ascii_lowercase()));
    let feature = Path::new("doc/plans").join(name.to_ascii_lowercase());
    for path in existing {
        let lower = PathBuf::from(path.as_os_str().to_ascii_lowercase());
        let clash = size == KickoffSize::Plan
            && plan_home(path).is_some()
            && path
                .file_name()
                .and_then(|file| file.to_str())
                .and_then(plan_slug)
                .is_some_and(|slug| slug.eq_ignore_ascii_case(name));
        if clash
            || (size == KickoffSize::Feature && lower == feature)
            || document.starts_with(&lower)
            || bundle
                .as_ref()
                .is_some_and(|bundle| bundle.starts_with(&lower))
        {
            return Err(ArmRefusal::Exists(path.clone()));
        }
    }
    Ok(())
}

fn refuse_live(
    root: &Path,
    size: KickoffSize,
    name: &str,
    target: &ArmedTarget,
) -> Result<(), ArmRefusal> {
    let paths = match size {
        KickoffSize::Feature => vec![
            Path::new("doc/product").join(name),
            Path::new("doc/plans").join(name),
        ],
        KickoffSize::Plan => std::iter::once(target.document.clone())
            .chain(target.bundle.clone())
            .collect(),
    };
    let fd = safe_open_dirfd(root).map_err(unreadable)?;
    for path in paths {
        if let (Some(parent), Some(name)) = (path.parent(), path.file_name()) {
            for (entry, _) in entries(&fd, parent).map_err(unreadable)? {
                if entry.eq_ignore_ascii_case(name) {
                    return Err(ArmRefusal::Exists(parent.join(entry)));
                }
            }
        }
    }
    Ok(())
}

#[cfg(test)]
#[path = "scope_tests.rs"]
mod tests;
