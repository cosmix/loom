//! The native worker's Seatbelt rules for host files outside its roots: the
//! directories above its worktree listed as the completion profile lists them,
//! and the Cargo home and per-user Git files its toolchains read
//! ([`toolchain_file_rules`]). The builders are the completion profile's own
//! (`verify::criteria::ancestors`, `verify::criteria::seatbelt_files`).

use anyhow::Result;
use std::path::{Path, PathBuf};

use crate::verify::criteria::ancestors;
use crate::verify::criteria::seatbelt_files::{git_config_rules, host_cargo_home_rules};

/// The host directories whose single files the boundary's toolchains read:
/// the Cargo home its environment names (`CARGO_HOME`, else `$HOME/.cargo`)
/// and its `HOME`, for the per-user Git files. Kept apart from
/// [`super::seatbelt_profile::SeatbeltBoundarySpec`]: they name single
/// files, not roots.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct HostFiles {
    pub cargo_home: Option<PathBuf>,
    pub home: Option<PathBuf>,
}

/// The completion profile's rules for files outside the roots, in its order
/// (`verify::criteria::confine_seatbelt::named_rules`): the directories above
/// `worktree` listable, then the Cargo files there hidden (after the
/// listings, so a deny wins whatever spelling names the file) and each
/// `package.json` there readable, then the host Cargo home and Git files.
/// They precede every protected and hidden deny, which keep winning.
pub(crate) fn toolchain_file_rules(
    worktree: &Path,
    readable: &[&PathBuf],
    host: &HostFiles,
) -> Result<String> {
    let readable: Vec<PathBuf> = readable
        .iter()
        .map(|root| root.canonicalize().unwrap_or_else(|_| root.to_path_buf()))
        .collect();
    let mut rules = ancestors::rules_above(worktree, &readable)?;
    if let Some(cargo_home) = &host.cargo_home {
        rules.push_str(&host_cargo_home_rules(cargo_home)?);
    }
    if let Some(home) = &host.home {
        rules.push_str(&git_config_rules(home)?);
    }
    Ok(rules)
}
