//! Confirming a kickoff: the size and name become the one target the session may create.

use std::fmt;
use std::path::{Path, PathBuf};

use anyhow::Result;

use super::{
    entries, kickoff_target, load, snapshot, write, ArmedTarget, KickoffGrant, KickoffSize,
    SessionScope,
};
use crate::fs::safe_fs::safe_open_dirfd;
use crate::plan_session::kind::SessionDir;
use crate::planning::kinds::is_feature_name;
use crate::planning::slug::{plan_home, plan_slug};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ArmRefusal {
    NotKickoff,
    AlreadyConfirmed,
    /// The kickoff was started as this size; it cannot be confirmed as the other.
    WrongSize(KickoffSize),
    InvalidName(String),
    Exists(PathBuf),
    Unreadable(String),
}

impl fmt::Display for ArmRefusal {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NotKickoff => write!(f, "This session is not a kickoff session."),
            Self::AlreadyConfirmed => write!(f, "This kickoff already has a confirmed target."),
            Self::WrongSize(KickoffSize::Feature) => {
                write!(f, "This kickoff was started as a feature.")
            }
            Self::WrongSize(KickoffSize::Plan) => {
                write!(f, "This kickoff was started as a standalone plan.")
            }
            Self::InvalidName(name) => write!(f, "{name:?} is not a valid feature or plan name."),
            Self::Exists(path) => write!(f, "The target already exists at {}.", path.display()),
            Self::Unreadable(reason) => write!(f, "Cannot confirm the kickoff: {reason}."),
        }
    }
}

impl std::error::Error for ArmRefusal {}

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
    if kickoff.size != size {
        return Err(ArmRefusal::WrongSize(kickoff.size));
    }
    if !is_feature_name(name) {
        return Err(ArmRefusal::InvalidName(name.to_owned()));
    }
    let target = kickoff_target(size, name);
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
