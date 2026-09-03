// `crate::claude` and `crate::codex` also exist, so an unqualified `claude::`
// or `codex::` inside this module resolves to the sibling below, not to them.
mod claude;
mod codex;
mod placement;

use anyhow::{bail, ensure, Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::skill_index;
use crate::completions;
use crate::fs::permissions::install_loom_hooks_to;
use crate::skills::SkillLayout;

#[derive(Debug)]
pub struct InstallPaths {
    pub claude_dir: PathBuf,
    pub codex_dir: PathBuf,
}

#[derive(Debug)]
pub struct InstallReport {
    pub agents: usize,
    pub commands: usize,
    pub hooks: usize,
    pub skills_resident: usize,
    pub skills_catalogued: usize,
    pub codex_skills_resident: usize,
    pub codex_skills_catalogued: usize,
    pub backups: Vec<PathBuf>,
    pub layout: SkillLayout,
}

/// Return the standard Claude and Codex configuration directories.
pub fn default_paths() -> Result<InstallPaths> {
    let home = dirs::home_dir().context("Failed to determine home directory")?;
    Ok(InstallPaths {
        claude_dir: home.join(".claude"),
        codex_dir: home.join(".codex"),
    })
}

/// Place every embedded asset into the supplied Claude and Codex directories.
pub fn install_all(
    paths: &InstallPaths,
    layout: Option<SkillLayout>,
    refresh_completions: bool,
) -> Result<InstallReport> {
    ensure_distinct_dirs(&paths.claude_dir, &paths.codex_dir)?;
    let layout = layout.unwrap_or_else(|| SkillLayout::read(&paths.claude_dir));
    let (agents, commands) = claude::install_files(
        &paths.claude_dir,
        crate::assets::CLAUDE_AGENTS,
        crate::assets::CLAUDE_COMMANDS,
    )?;
    let hooks = install_loom_hooks_to(&paths.claude_dir.join("hooks/loom"))?;
    let (skills_resident, skills_catalogued) =
        claude::install_skills(&paths.claude_dir, crate::assets::SKILLS, layout)?;
    let (codex_skills_resident, codex_skills_catalogued) = codex::install_skills(
        &paths.codex_dir,
        crate::assets::SKILLS,
        crate::assets::CODEX_SKILLS,
        layout,
    )?;
    let backups = install_doctrine(paths)?;
    claude::write_layout(&paths.claude_dir, layout)?;
    skill_index::execute_in_claude_dir(&paths.claude_dir, false)?;
    if refresh_completions {
        let home = dirs::home_dir().context("Failed to determine home directory")?;
        completions::install::refresh_existing_in(&home)?;
    }
    Ok(InstallReport {
        agents,
        commands,
        hooks,
        skills_resident,
        skills_catalogued,
        codex_skills_resident,
        codex_skills_catalogued,
        backups,
        layout,
    })
}

/// Reject a Claude and Codex directory that resolve to the same place: the
/// second tree's install would silently overwrite the first's.
fn ensure_distinct_dirs(claude_dir: &Path, codex_dir: &Path) -> Result<()> {
    let same = match (claude_dir.canonicalize(), codex_dir.canonicalize()) {
        (Ok(claude), Ok(codex)) => claude == codex,
        _ => claude_dir == codex_dir,
    };
    ensure!(
        !same,
        "--claude-dir and --codex-dir both resolve to {}; each install tree needs its own directory",
        claude_dir.display()
    );
    Ok(())
}

/// Write the two managed doctrine files, returning any backups they displaced.
fn install_doctrine(paths: &InstallPaths) -> Result<Vec<PathBuf>> {
    let mut backups = Vec::new();
    for (dest, body) in [
        (
            paths.claude_dir.join("CLAUDE.md"),
            crate::assets::CLAUDE_MD_TEMPLATE,
        ),
        (
            paths.codex_dir.join("AGENTS.md"),
            crate::assets::AGENTS_MD_TEMPLATE,
        ),
    ] {
        if let Some(backup) = write_managed_file(&dest, body)? {
            backups.push(backup);
        }
    }
    Ok(backups)
}

/// Write the managed document at `dest`, returning the backup it displaced.
///
/// A file still carrying loom's header is loom's own output, so a stale one is
/// refreshed in place: backing it up would preserve nothing but an older copy
/// of the same template. Only a file loom did not write is ever backed up.
///
/// A symlink whose target is gone is refused rather than written through: the
/// write would follow the link and create a file outside the tree loom was
/// asked to install into, with no backup taken and nothing reported.
pub(crate) fn write_managed_file(dest: &Path, body: &str) -> Result<Option<PathBuf>> {
    let Some(metadata) = placement::symlink_metadata(dest)? else {
        write_headered(dest, body)?;
        return Ok(None);
    };
    if metadata.is_symlink() && !dest.exists() {
        let target = fs::read_link(dest)
            .with_context(|| format!("Failed to read the symlink {}", dest.display()))?;
        bail!(
            "{} is a symlink to {}, which does not exist; repoint or remove it, then reinstall",
            dest.display(),
            target.display()
        );
    }
    let existing =
        fs::read_to_string(dest).with_context(|| format!("Failed to read {}", dest.display()))?;
    if let Some(existing_body) = managed_body(&existing) {
        if existing_body != body {
            write_headered(dest, body)?;
        }
        return Ok(None);
    }
    back_up_and_replace(dest, body).map(Some)
}

/// Preserve the operator's own file beside itself, then write the managed one.
///
/// A symlinked destination is copied rather than renamed, and written through:
/// `~/.claude/CLAUDE.md` is commonly a link into a dotfiles repository, and
/// renaming the link would replace it with a regular file, then delete it as a
/// stale backup on the next install.
fn back_up_and_replace(dest: &Path, body: &str) -> Result<PathBuf> {
    remove_old_backups(dest)?;
    let backup = backup_path(dest)?;
    let metadata = dest
        .symlink_metadata()
        .with_context(|| format!("Failed to inspect {}", dest.display()))?;
    let outcome = if metadata.is_symlink() {
        fs::copy(dest, &backup).map(|_| ())
    } else {
        fs::rename(dest, &backup)
    };
    outcome.with_context(|| {
        format!(
            "Failed to back up {} to {}",
            dest.display(),
            backup.display()
        )
    })?;
    write_headered(dest, body)?;
    Ok(backup)
}

/// The body of a document loom itself wrote, if `content` is one.
///
/// `install.sh` stamps `installed` where this module stamps `updated`; both are
/// loom's own output, and a machine bootstrapped by the script would otherwise
/// have its `CLAUDE.md` backed up as if an operator had written it.
fn managed_body(content: &str) -> Option<&str> {
    let (header, body) = content.split_once("\n\n")?;
    let stamp = header.lines().nth(1)?;
    let managed = ["# claude-loom | updated ", "# claude-loom | installed "]
        .iter()
        .any(|marker| stamp.starts_with(marker));
    (header.starts_with("# ─") && managed).then_some(body)
}

/// Write `body` under loom's managed-file banner.
///
/// The single writer of that banner: `managed_body` recognises files by it, so
/// a second emitter anywhere in the tree would drift out of step with it.
fn write_headered(dest: &Path, body: &str) -> Result<()> {
    let parent = dest
        .parent()
        .context("Managed asset destination has no parent directory")?;
    fs::create_dir_all(parent).with_context(|| format!("Failed to create {}", parent.display()))?;
    let timestamp = chrono::Utc::now().format("%Y-%m-%d %H:%M:%S");
    let content = format!(
        "# ───────────────────────────────────────────────────────────\n\
         # claude-loom | updated {timestamp}\n\
         # ───────────────────────────────────────────────────────────\n\n\
         {body}"
    );
    fs::write(dest, content).with_context(|| format!("Failed to write {}", dest.display()))
}

fn backup_path(dest: &Path) -> Result<PathBuf> {
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .context("Managed asset destination has no UTF-8 file name")?;
    let timestamp = chrono::Utc::now().format("%Y%m%d-%H%M%S");
    Ok(dest.with_file_name(format!("{name}.bak.{timestamp}")))
}

fn remove_old_backups(dest: &Path) -> Result<()> {
    let parent = dest
        .parent()
        .context("Managed asset destination has no parent directory")?;
    let name = dest
        .file_name()
        .and_then(|name| name.to_str())
        .context("Managed asset destination has no UTF-8 file name")?;
    let prefix = format!("{name}.bak.");
    for entry in
        fs::read_dir(parent).with_context(|| format!("Failed to read {}", parent.display()))?
    {
        let entry = entry.with_context(|| format!("Failed to inspect {}", parent.display()))?;
        let file_type = entry
            .file_type()
            .with_context(|| format!("Failed to inspect {}", entry.path().display()))?;
        let file_name = entry.file_name();
        // Only ever a regular file loom wrote: `remove_file` on a directory
        // fails and would abort the install with the tree half written.
        if file_type.is_file() && is_loom_backup(file_name.to_string_lossy().as_ref(), &prefix) {
            fs::remove_file(entry.path())
                .with_context(|| format!("Failed to remove {}", entry.path().display()))?;
        }
    }
    Ok(())
}

fn is_loom_backup(name: &str, prefix: &str) -> bool {
    let Some(stamp) = name.strip_prefix(prefix) else {
        return false;
    };
    let bytes = stamp.as_bytes();
    bytes.len() == 15
        && bytes[8] == b'-'
        && bytes
            .iter()
            .enumerate()
            .all(|(index, byte)| index == 8 || byte.is_ascii_digit())
}
