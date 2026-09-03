mod claude;
mod codex;

use anyhow::{Context, Result};
use std::fs;
use std::path::{Path, PathBuf};

use crate::commands::skill_index;
use crate::completions;
use crate::fs::permissions::install_loom_hooks_to;
use crate::skills::SkillLayout;

#[derive(Debug, Clone, PartialEq, Eq)]
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

fn write_managed_file(dest: &Path, body: &str) -> Result<Option<PathBuf>> {
    if dest.exists() {
        let existing = fs::read_to_string(dest)
            .with_context(|| format!("Failed to read {}", dest.display()))?;
        if managed_body(&existing) == Some(body) {
            return Ok(None);
        }
        remove_old_backups(dest)?;
        let backup = backup_path(dest)?;
        fs::rename(dest, &backup).with_context(|| {
            format!(
                "Failed to back up {} to {}",
                dest.display(),
                backup.display()
            )
        })?;
        write_headered(dest, body)?;
        return Ok(Some(backup));
    }
    write_headered(dest, body)?;
    Ok(None)
}

fn managed_body(content: &str) -> Option<&str> {
    let (header, body) = content.split_once("\n\n")?;
    let marker = "# claude-loom | updated ";
    (header.starts_with("# ─") && header.lines().nth(1)?.starts_with(marker)).then_some(body)
}

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
        let file_name = entry.file_name();
        if is_loom_backup(file_name.to_string_lossy().as_ref(), &prefix) {
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
