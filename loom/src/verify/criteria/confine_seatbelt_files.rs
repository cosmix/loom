//! Seatbelt rules for single host files outside the tree: the Cargo and Maven
//! homes' secrets, hidden by name, and the per-user Git files Cargo needs.

use anyhow::Result;
use std::path::{Path, PathBuf};

use super::{ancestors, quote, quote_checked};
use crate::verify::criteria::confine::scratch::linked_home::STATE_FILES;

/// Maven files that never reach a confined command: the user's settings, which
/// can hold repository credentials, and the JDK toolchains file that
/// `actions/setup-java` writes. Maven fails on a file it can see but not read
/// (`Non-readable toolchains`), so they are hidden like the Cargo config files.
pub(super) const HIDDEN_IN_M2: [&str; 3] =
    ["settings.xml", "settings-security.xml", "toolchains.xml"];

/// Git's per-user ignore and attributes files, which it and Cargo's Git
/// library read by default when no config names others (the user's config is
/// already out: `GIT_CONFIG_GLOBAL=/dev/null` in `apply_environment`). The
/// stage environment carries no `XDG_CONFIG_HOME`, so they sit under
/// `$HOME/.config/git`. Unreadable, the ignore file failed every Cargo build
/// script's first fingerprint with "Could not read repository exclude", and
/// hiding it does not help: the library fails on any open error other than
/// `ENOENT`, and Seatbelt denies with `EPERM`. They hold path patterns, no
/// credentials, so they are readable.
pub(super) const READABLE_IN_GIT_CONFIG: [&str; 2] = ["ignore", "attributes"];

/// Rules letting a confined command read each existing file of `paths`, at its
/// path and at its symlink target.
pub(super) fn readable_file_rules(paths: &[PathBuf]) -> Result<String> {
    let mut rules = String::new();
    for path in paths {
        let Ok(target) = path.canonicalize() else {
            continue;
        };
        for file in std::iter::once(path.clone()).chain((target != *path).then_some(target)) {
            rules.push_str(&format!("(allow file-read* (literal {}))", quote(&file)?));
        }
    }
    Ok(rules)
}

/// Files in a Cargo home that never reach a confined command, as Bubblewrap's
/// private home keeps them out: the user's config and registry credentials.
const HIDDEN_IN_CARGO_HOME: [&str; 4] =
    ["config", "config.toml", "credentials", "credentials.toml"];

/// Rules for the host Cargo home read in place. Its lock and last-use files are
/// readable, so Cargo opens them and only warns when it cannot write them;
/// nothing in the home is writable. Its config and credentials files are
/// hidden by name, metadata included: Cargo fails on a config file it can see
/// but not read. Each existing file is hidden at its path and its target.
pub(super) fn host_cargo_home_rules(home: &Path) -> Result<String> {
    let home = home.canonicalize().unwrap_or_else(|_| home.to_path_buf());
    let mut rules = String::new();
    for name in STATE_FILES {
        let state = quote(&home.join(name))?;
        rules.push_str(&format!("(allow file-read* (literal {state}))"));
    }
    rules.push_str(&hidden_rules(
        &HIDDEN_IN_CARGO_HOME.map(|name| home.join(name)),
    )?);
    Ok(rules)
}

/// Rules hiding each existing file of `paths`, metadata included, at its path
/// and at its symlink target.
pub(super) fn hidden_rules(paths: &[PathBuf]) -> Result<String> {
    let mut hidden = Vec::new();
    for path in paths {
        if path.symlink_metadata().is_err() {
            continue;
        }
        match path.canonicalize() {
            Ok(target) if &target != path => hidden.push(target),
            _ => {}
        }
        hidden.push(path.clone());
    }
    Ok(ancestors::seatbelt_rules(&hidden, quote_checked(&hidden)?))
}
