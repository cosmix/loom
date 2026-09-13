use std::ffi::OsStr;
use std::fs::{self, OpenOptions};
use std::io::Read;
use std::os::unix::ffi::OsStrExt;
use std::os::unix::fs::{MetadataExt, OpenOptionsExt};
use std::path::{Component, Path, PathBuf};

use anyhow::{bail, ensure, Context};

use super::is_safe_id;

const MAX_LOCATOR_BYTES: u64 = 1024 * 1024;
const MAX_WORKSPACE_CHILDREN: usize = 256;

pub fn read_bounded_prefix(path: &Path, max_bytes: usize) -> anyhow::Result<String> {
    let file = OpenOptions::new()
        .read(true)
        .custom_flags(libc::O_NOFOLLOW)
        .open(path)
        .with_context(|| format!("opening {} without following symlinks", path.display()))?;
    let metadata = file
        .metadata()
        .with_context(|| format!("reading metadata for {}", path.display()))?;
    ensure!(
        metadata.file_type().is_file(),
        "{} is not a regular file",
        path.display()
    );
    ensure!(
        metadata.uid() == effective_uid(),
        "{} is not owned by the effective user",
        path.display()
    );

    let limit = u64::try_from(max_bytes).context("read limit does not fit in u64")?;
    let mut bytes = Vec::with_capacity(max_bytes.min(8192));
    file.take(limit)
        .read_to_end(&mut bytes)
        .with_context(|| format!("reading bounded prefix from {}", path.display()))?;
    decode_utf8_prefix(bytes)
}

fn decode_utf8_prefix(bytes: Vec<u8>) -> anyhow::Result<String> {
    match String::from_utf8(bytes) {
        Ok(value) => Ok(value),
        Err(error) if error.utf8_error().error_len().is_none() => {
            let valid_up_to = error.utf8_error().valid_up_to();
            let bytes = error.into_bytes();
            let prefix = std::str::from_utf8(&bytes[..valid_up_to])
                .context("valid UTF-8 prefix could not be decoded")?;
            Ok(prefix.to_owned())
        }
        Err(error) => Err(anyhow::anyhow!(
            "invalid UTF-8 at byte {}",
            error.utf8_error().valid_up_to()
        )),
    }
}

fn effective_uid() -> u32 {
    // SAFETY: geteuid has no arguments and only reads process credentials.
    unsafe { libc::geteuid() }
}

pub fn default_task_output_roots() -> Vec<PathBuf> {
    vec![PathBuf::from(format!("/tmp/claude-{}", effective_uid()))]
}

pub fn validate_task_output_path(
    path: &Path,
    roots: &[PathBuf],
    parent_session_id: &str,
    task_id: &str,
) -> anyhow::Result<PathBuf> {
    ensure!(is_safe_id(parent_session_id), "unsafe parent session id");
    ensure!(is_safe_id(task_id), "unsafe task id");
    ensure!(path.is_absolute(), "task output path must be absolute");
    ensure!(
        !has_dot_component(path),
        "task output path contains . or .."
    );
    let expected_file = format!("{task_id}.output");

    for root in roots.iter().filter(|root| root.is_absolute()) {
        let Ok(relative) = path.strip_prefix(root) else {
            continue;
        };
        let Some(parts) = normal_components(relative) else {
            continue;
        };
        if parts.len() != 4
            || parts[1] != OsStr::new(parent_session_id)
            || parts[2] != OsStr::new("tasks")
            || parts[3] != OsStr::new(&expected_file)
        {
            continue;
        }
        let workspace = root.join(parts[0]);
        let session = workspace.join(parts[1]);
        let tasks = session.join(parts[2]);
        check_plain_directory(&workspace)?;
        check_plain_directory(&session)?;
        check_plain_directory(&tasks)?;
        check_regular_owned_bounded(path)?;
        return Ok(path.to_path_buf());
    }
    bail!("task output path does not match an allowed root and identity")
}

pub fn default_companion_state_roots(home: &Path, plugin_data: Option<&Path>) -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Some(plugin_data) = plugin_data {
        push_unique(&mut roots, plugin_data.join("state"));
    }
    push_unique(
        &mut roots,
        home.join(".claude/plugins/data/codex-openai-codex/state"),
    );
    push_unique(&mut roots, home.join(".codex/plugin-data/state"));
    push_unique(&mut roots, std::env::temp_dir().join("codex-companion"));
    push_unique(&mut roots, PathBuf::from("/tmp/codex-companion"));
    roots
}

fn push_unique(roots: &mut Vec<PathBuf>, root: PathBuf) {
    if !roots.contains(&root) {
        roots.push(root);
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LocatorResolution {
    Found(PathBuf),
    NotFound,
    Ambiguous(usize),
}

pub fn resolve_companion_locator(roots: &[PathBuf], job_id: &str) -> LocatorResolution {
    if !is_safe_id(job_id) {
        return LocatorResolution::NotFound;
    }
    let filename = format!("{job_id}.json");
    let mut matches = Vec::new();
    for root in roots.iter().filter(|root| root.is_absolute()) {
        let Ok(entries) = fs::read_dir(root) else {
            continue;
        };
        let children = entries
            .flatten()
            .filter(|entry| entry.file_type().is_ok_and(|file_type| file_type.is_dir()));
        for entry in children.take(MAX_WORKSPACE_CHILDREN) {
            let workspace = entry.path();
            let jobs = workspace.join("jobs");
            let candidate = jobs.join(&filename);
            if check_job_candidate(&workspace, &jobs, &candidate).is_ok()
                && !matches.contains(&candidate)
            {
                matches.push(candidate);
            }
        }
    }
    match matches.len() {
        0 => LocatorResolution::NotFound,
        1 => LocatorResolution::Found(matches.remove(0)),
        count => LocatorResolution::Ambiguous(count),
    }
}

pub fn validate_locator(
    locator: &Path,
    roots: &[PathBuf],
    job_id: &str,
) -> anyhow::Result<PathBuf> {
    ensure!(is_safe_id(job_id), "unsafe companion job id");
    ensure!(locator.is_absolute(), "companion locator must be absolute");
    ensure!(
        !has_dot_component(locator),
        "companion locator contains . or .."
    );
    let expected_file = format!("{job_id}.json");

    for root in roots.iter().filter(|root| root.is_absolute()) {
        let Ok(relative) = locator.strip_prefix(root) else {
            continue;
        };
        let Some(parts) = normal_components(relative) else {
            continue;
        };
        if parts.len() != 3
            || parts[1] != OsStr::new("jobs")
            || parts[2] != OsStr::new(&expected_file)
        {
            continue;
        }
        let workspace = root.join(parts[0]);
        let jobs = workspace.join(parts[1]);
        check_job_candidate(&workspace, &jobs, locator)?;
        return Ok(locator.to_path_buf());
    }
    bail!("companion locator is outside the allowed state roots")
}

fn check_job_candidate(workspace: &Path, jobs: &Path, file: &Path) -> anyhow::Result<()> {
    check_plain_directory(workspace)?;
    check_plain_directory(jobs)?;
    check_regular_owned_bounded(file)
}

fn check_plain_directory(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("reading directory metadata for {}", path.display()))?;
    ensure!(
        metadata.file_type().is_dir(),
        "{} is not a non-symlink directory",
        path.display()
    );
    Ok(())
}

fn check_regular_owned_bounded(path: &Path) -> anyhow::Result<()> {
    let metadata = fs::symlink_metadata(path)
        .with_context(|| format!("reading file metadata for {}", path.display()))?;
    ensure!(
        metadata.file_type().is_file(),
        "{} is not a regular file",
        path.display()
    );
    ensure!(
        metadata.uid() == effective_uid(),
        "{} is not owned by the effective user",
        path.display()
    );
    ensure!(
        metadata.len() <= MAX_LOCATOR_BYTES,
        "{} exceeds the 1 MiB limit",
        path.display()
    );
    Ok(())
}

fn normal_components(path: &Path) -> Option<Vec<&OsStr>> {
    path.components()
        .map(|component| match component {
            Component::Normal(value) => Some(value),
            _ => None,
        })
        .collect()
}

fn has_dot_component(path: &Path) -> bool {
    path.as_os_str()
        .as_bytes()
        .split(|byte| *byte == b'/')
        .any(|component| component == b"." || component == b"..")
}

#[cfg(test)]
#[path = "locator_tests.rs"]
mod tests;
