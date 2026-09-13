use std::fs::{self, File};
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use anyhow::Context;
use tempfile::TempDir;

use super::*;

const SESSION: &str = "session-123";
const TASK: &str = "task-456";
const JOB: &str = "job-789";

fn task_path(root: &Path, session: &str, task: &str) -> PathBuf {
    root.join("-workspace")
        .join(session)
        .join("tasks")
        .join(format!("{task}.output"))
}

fn write_task(root: &Path, session: &str, task: &str) -> anyhow::Result<PathBuf> {
    let path = task_path(root, session, task);
    fs::create_dir_all(path.parent().context("task path has no parent")?)?;
    fs::write(&path, b"output")?;
    Ok(path)
}

fn write_job(root: &Path, workspace: &str, job: &str) -> anyhow::Result<PathBuf> {
    let path = root
        .join(workspace)
        .join("jobs")
        .join(format!("{job}.json"));
    fs::create_dir_all(path.parent().context("job path has no parent")?)?;
    fs::write(&path, b"{}")?;
    Ok(path)
}

#[test]
fn task_output_accepts_exact_valid_path() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let path = write_task(temp.path(), SESSION, TASK)?;

    let validated = validate_task_output_path(&path, &[temp.path().into()], SESSION, TASK)?;

    assert_eq!(validated, path);
    Ok(())
}

#[test]
fn task_output_rejects_wrong_session_or_task_component() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let wrong_session = write_task(temp.path(), "session-other", TASK)?;
    let wrong_task = write_task(temp.path(), SESSION, "task-other")?;

    assert!(
        validate_task_output_path(&wrong_session, &[temp.path().into()], SESSION, TASK).is_err()
    );
    assert!(validate_task_output_path(&wrong_task, &[temp.path().into()], SESSION, TASK).is_err());
    Ok(())
}

#[test]
fn task_output_rejects_parent_component() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let valid = write_task(temp.path(), SESSION, TASK)?;
    let path = valid
        .parent()
        .context("task path has no parent")?
        .join("..")
        .join("tasks")
        .join(format!("{TASK}.output"));

    assert!(validate_task_output_path(&path, &[temp.path().into()], SESSION, TASK).is_err());
    Ok(())
}

#[test]
fn task_output_rejects_symlinked_file() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let target = temp.path().join("target.output");
    fs::write(&target, b"output")?;
    let path = task_path(temp.path(), SESSION, TASK);
    fs::create_dir_all(path.parent().context("task path has no parent")?)?;
    symlink(&target, &path)?;

    assert!(validate_task_output_path(&path, &[temp.path().into()], SESSION, TASK).is_err());
    Ok(())
}

#[test]
fn task_output_rejects_symlinked_intermediate_directory() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let actual = temp.path().join("actual-workspace");
    let task_dir = actual.join(SESSION).join("tasks");
    fs::create_dir_all(&task_dir)?;
    fs::write(task_dir.join(format!("{TASK}.output")), b"output")?;
    symlink(&actual, temp.path().join("-workspace"))?;
    let path = task_path(temp.path(), SESSION, TASK);

    assert!(validate_task_output_path(&path, &[temp.path().into()], SESSION, TASK).is_err());
    Ok(())
}

#[test]
fn task_output_rejects_foreign_root() -> anyhow::Result<()> {
    let allowed = TempDir::new()?;
    let foreign = TempDir::new()?;
    let path = write_task(foreign.path(), SESSION, TASK)?;

    assert!(validate_task_output_path(&path, &[allowed.path().into()], SESSION, TASK).is_err());
    Ok(())
}

#[test]
fn task_output_rejects_oversized_file() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let path = write_task(temp.path(), SESSION, TASK)?;
    File::options()
        .write(true)
        .open(&path)?
        .set_len(MAX_LOCATOR_BYTES + 1)?;

    assert!(validate_task_output_path(&path, &[temp.path().into()], SESSION, TASK).is_err());
    Ok(())
}

#[test]
fn bounded_prefix_drops_torn_trailing_utf8() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let path = temp.path().join("prefix");
    fs::write(&path, b"ok\xe2\x82\xac")?;

    assert_eq!(read_bounded_prefix(&path, 3)?, "ok");
    Ok(())
}

#[test]
fn default_companion_state_roots_include_temp_fallbacks_without_duplicates() {
    let home = Path::new("/home/tester");
    let temp_root = std::env::temp_dir().join("codex-companion");
    let fallback_root = PathBuf::from("/tmp/codex-companion");
    let roots = default_companion_state_roots(home, None);

    assert!(roots.contains(&temp_root));
    assert!(roots.contains(&fallback_root));
    assert_eq!(roots.iter().filter(|root| *root == &temp_root).count(), 1);
    assert_eq!(
        roots.iter().filter(|root| *root == &fallback_root).count(),
        1
    );
    assert_eq!(roots.len(), if temp_root == fallback_root { 3 } else { 4 });
}

#[test]
fn companion_locator_finds_exact_job_and_validates_it() -> anyhow::Result<()> {
    let root_one = TempDir::new()?;
    let root_two = TempDir::new()?;
    fs::create_dir(root_one.path().join("workspace-one"))?;
    let path = write_job(root_two.path(), "workspace-two", JOB)?;
    let roots = vec![root_one.path().into(), root_two.path().into()];

    assert_eq!(
        resolve_companion_locator(&roots, JOB),
        LocatorResolution::Found(path.clone())
    );
    assert_eq!(validate_locator(&path, &roots, JOB)?, path);
    Ok(())
}

#[test]
fn companion_locator_reports_not_found() -> anyhow::Result<()> {
    let root_one = TempDir::new()?;
    let root_two = TempDir::new()?;
    fs::create_dir(root_one.path().join("workspace-one"))?;
    fs::create_dir(root_two.path().join("workspace-two"))?;

    assert_eq!(
        resolve_companion_locator(&[root_one.path().into(), root_two.path().into()], JOB),
        LocatorResolution::NotFound
    );
    Ok(())
}

#[test]
fn companion_locator_reports_ambiguity_across_roots() -> anyhow::Result<()> {
    let root_one = TempDir::new()?;
    let root_two = TempDir::new()?;
    write_job(root_one.path(), "workspace-one", JOB)?;
    write_job(root_two.path(), "workspace-two", JOB)?;

    assert_eq!(
        resolve_companion_locator(&[root_one.path().into(), root_two.path().into()], JOB),
        LocatorResolution::Ambiguous(2)
    );
    Ok(())
}

#[test]
fn companion_locator_rejects_symlinked_jobs_file() -> anyhow::Result<()> {
    let temp = TempDir::new()?;
    let jobs = temp.path().join("workspace").join("jobs");
    fs::create_dir_all(&jobs)?;
    let target = temp.path().join("target.json");
    fs::write(&target, b"{}")?;
    symlink(&target, jobs.join(format!("{JOB}.json")))?;

    assert_eq!(
        resolve_companion_locator(&[temp.path().into()], JOB),
        LocatorResolution::NotFound
    );
    Ok(())
}

#[test]
fn validate_locator_rejects_path_outside_roots() -> anyhow::Result<()> {
    let allowed = TempDir::new()?;
    let foreign = TempDir::new()?;
    let path = write_job(foreign.path(), "workspace", JOB)?;

    assert!(validate_locator(&path, &[allowed.path().into()], JOB).is_err());
    Ok(())
}
