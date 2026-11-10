//! The copied HOME a probe's Claude Code runs under, and the check that the real one stays
//! untouched.

use std::fs;
use std::io;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

use serde_json::Value;
use sha2::{Digest, Sha256};

/// A file larger than this is not copied.
const MAX_FILE: u64 = 16 * 1024 * 1024;
/// The copied trees stop growing past this many bytes.
const MAX_TOTAL: u64 = 512 * 1024 * 1024;
/// Copied as files, relative to HOME.
const FILES: [&str; 3] = [
    ".claude.json",
    ".claude/.credentials.json",
    ".claude/settings.json",
];
/// Copied as trees, so personal and plugin skills and commands appear in the session.
const TREES: [&str; 4] = [
    ".claude/skills",
    ".claude/commands",
    ".claude/agents",
    ".claude/plugins",
];
/// Hashed before and after each probe.
const WATCHED: [&str; 1] = [".claude/settings.json"];

/// The real HOME, from the test process's environment.
pub fn real_home() -> PathBuf {
    PathBuf::from(std::env::var_os("HOME").expect("the probes need HOME"))
}

/// Builds `<base>/home` from `real`; returns it. Nothing under `real` is written.
pub fn copy_home(real: &Path, base: &Path) -> io::Result<PathBuf> {
    let home = base.join("home");
    fs::create_dir_all(home.join(".claude"))?;
    for name in FILES {
        let from = real.join(name);
        if from.is_file() {
            fs::copy(&from, home.join(name))?;
        }
    }
    let credentials = home.join(".claude/.credentials.json");
    if credentials.exists() {
        fs::set_permissions(&credentials, fs::Permissions::from_mode(0o600))?;
    }
    let fence = real.join(".claude").canonicalize()?;
    let mut budget = MAX_TOTAL;
    for name in TREES {
        let from = real.join(name);
        if from.is_dir() {
            copy_tree(&from, &home.join(name), &fence, &mut budget)?;
        }
    }
    repoint_plugins(&home.join(".claude/plugins"), real, &home)?;
    Ok(home)
}

enum Kind {
    Dir,
    File(u64),
}

/// What `path` is, a symlink followed only while its target stays inside `fence` and is not
/// an ancestor of `path` (a loop); `None` for anything else, a socket, fifo or device.
fn kind(path: &Path, fence: &Path) -> Option<Kind> {
    let mut meta = fs::symlink_metadata(path).ok()?;
    if meta.file_type().is_symlink() {
        let target = path.canonicalize().ok()?;
        let parent = path.parent()?.canonicalize().ok()?;
        if !target.starts_with(fence) || parent.starts_with(&target) {
            return None;
        }
        meta = fs::metadata(&target).ok()?;
    }
    if meta.is_dir() {
        Some(Kind::Dir)
    } else if meta.is_file() {
        Some(Kind::File(meta.len()))
    } else {
        None
    }
}

fn copy_tree(from: &Path, to: &Path, fence: &Path, budget: &mut u64) -> io::Result<()> {
    fs::create_dir_all(to)?;
    for entry in fs::read_dir(from)? {
        let entry = entry?;
        let (source, target) = (entry.path(), to.join(entry.file_name()));
        match kind(&source, fence) {
            Some(Kind::Dir) => copy_tree(&source, &target, fence, budget)?,
            Some(Kind::File(len)) if len <= MAX_FILE && len <= *budget => {
                fs::copy(&source, &target)?;
                *budget -= len;
            }
            _ => {}
        }
    }
    Ok(())
}

/// The plugin registries name install paths under the real `~/.claude`: point the copies at
/// the copied tree, so the session neither loads nor sweeps the real one.
fn repoint_plugins(plugins: &Path, real: &Path, home: &Path) -> io::Result<()> {
    let Ok(entries) = fs::read_dir(plugins) else {
        return Ok(());
    };
    let from = format!("{}/", real.join(".claude").display());
    let to = format!("{}/", home.join(".claude").display());
    for entry in entries.filter_map(Result::ok) {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "json") && path.is_file() {
            let text = fs::read_to_string(&path)?;
            fs::write(&path, text.replace(&from, &to))?;
        }
    }
    Ok(())
}

/// The account fields of `<home>/.claude.json` a pane or record may show, with their tokens.
pub fn identity(home: &Path) -> Vec<(String, &'static str)> {
    let text = fs::read_to_string(home.join(".claude.json")).unwrap_or_default();
    let config: Value = serde_json::from_str(&text).unwrap_or_default();
    let fields = [
        ("emailAddress", "<EMAIL>"),
        ("displayName", "<NAME>"),
        ("fullName", "<NAME>"),
        ("organizationName", "<ORG>"),
        ("accountUuid", "<ACCOUNT>"),
        ("organizationUuid", "<ORG-ID>"),
    ];
    let account = &config["oauthAccount"];
    let found = fields.into_iter().filter_map(|(field, token)| {
        let value = account[field].as_str()?;
        Some((value.to_owned(), token))
    });
    found.collect()
}

/// The user name the redaction removes: `$USER`, else the real HOME's last component.
pub fn user_name(real: &Path) -> String {
    let name = real.file_name().map(|n| n.to_string_lossy().into_owned());
    std::env::var("USER").ok().or(name).unwrap_or_default()
}

/// The real HOME's state when a probe starts. `~/.claude/settings.json` is hashed. Every
/// running Claude Code rewrites `~/.claude.json`, so a hash of it would fail whenever the
/// operator has another session open; the probe instead checks that the file never names
/// the probe's temp base (a trust or project entry leaking out of the copied HOME).
pub struct RealHome {
    files: Vec<(PathBuf, Option<String>)>,
    state: PathBuf,
    marker: String,
}

fn digest(path: &Path) -> Option<String> {
    fs::read(path)
        .ok()
        .map(|bytes| hex::encode(Sha256::digest(&bytes)))
}

impl RealHome {
    pub fn watch(real: &Path, base: &Path) -> RealHome {
        let files = WATCHED.iter().map(|name| real.join(name));
        RealHome {
            files: files.map(|path| (path.clone(), digest(&path))).collect(),
            state: real.join(".claude.json"),
            marker: base.display().to_string(),
        }
    }

    /// The hashed files whose content differs from when `watch` ran, and `~/.claude.json`
    /// when it names the probe's temp base.
    pub fn changed(&self) -> Vec<PathBuf> {
        let differs = |(path, was): &&(PathBuf, Option<String>)| digest(path) != *was;
        let mut changed: Vec<PathBuf> = self
            .files
            .iter()
            .filter(differs)
            .map(|(path, _)| path.clone())
            .collect();
        let state = fs::read_to_string(&self.state).unwrap_or_default();
        if state.contains(&self.marker) {
            changed.push(self.state.clone());
        }
        changed
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::os::unix::fs::symlink;

    #[test]
    fn copy_home_follows_only_symlinks_inside_claude_and_repoints_plugins() {
        let tmp = tempfile::TempDir::new().unwrap();
        let (real, base) = (tmp.path().join("real"), tmp.path().join("base"));
        let claude = real.join(".claude");
        fs::create_dir_all(claude.join("skills/a")).unwrap();
        fs::create_dir_all(claude.join("agents")).unwrap();
        fs::create_dir_all(claude.join("plugins")).unwrap();
        fs::create_dir_all(tmp.path().join("outside")).unwrap();
        fs::write(claude.join("skills/a/SKILL.md"), "a").unwrap();
        fs::write(claude.join("agents/x.md"), "x").unwrap();
        fs::write(real.join(".claude.json"), "{}").unwrap();
        fs::write(claude.join(".credentials.json"), "{}").unwrap();
        let installed = format!(
            r#"{{"installPath": "{}/plugins/cache/p"}}"#,
            claude.display()
        );
        fs::write(claude.join("plugins/installed_plugins.json"), installed).unwrap();
        symlink(tmp.path().join("outside"), claude.join("skills/out")).unwrap();
        symlink(claude.join("agents"), claude.join("skills/inner")).unwrap();
        symlink(claude.join("skills"), claude.join("skills/a/loop")).unwrap();

        let home = copy_home(&real, &base).unwrap();
        let skills = home.join(".claude/skills");
        assert!(skills.join("a/SKILL.md").is_file());
        assert!(
            skills.join("inner/x.md").is_file(),
            "a link inside ~/.claude is copied"
        );
        assert!(
            !skills.join("out").exists(),
            "a link out of ~/.claude is skipped"
        );
        assert!(
            !skills.join("a/loop").exists(),
            "a link to an ancestor is skipped"
        );
        let mode = fs::metadata(home.join(".claude/.credentials.json")).unwrap();
        assert_eq!(mode.permissions().mode() & 0o777, 0o600);
        let repointed = fs::read_to_string(home.join(".claude/plugins/installed_plugins.json"));
        let want = format!("{}/plugins/cache/p", home.join(".claude").display());
        assert!(repointed.unwrap().contains(&want));
        let original = fs::read_to_string(claude.join("plugins/installed_plugins.json"));
        assert!(original.unwrap().contains(&claude.display().to_string()));
    }
}
