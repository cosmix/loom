//! The evidence the briefing quotes, gathered from outside the process: the
//! evidence commit's diff, a shallow listing of the tree, and the disputed
//! stage's own block of the plan file.
//!
//! Every one of these degrades to a message rather than failing: a briefing
//! missing its diff is still a briefing, and the session can go and look.

use anyhow::{Context, Result};
use std::path::{Path, PathBuf};
use std::process::Command;

use crate::fs::work_dir::WorkDir;

/// The repository root `work_dir`'s state directory sits in — the directory
/// every one of these subprocesses runs from.
///
/// The hop count from the state root is layout-dependent (two for
/// `.loom/work`, one for a legacy `.work`) and lives in exactly one place:
/// `WorkDir::project_root`. A bare `parent()` here would be right for one
/// layout and wrong for the other, running `find` and `git show` from
/// `<repo>/.loom` — whose 3-deep listing is loom's own spool, not the tree the
/// adjudicator is being briefed on.
fn project_root_of(work_dir: &Path) -> PathBuf {
    WorkDir::new(work_dir)
        .ok()
        .and_then(|wd| wd.project_root().map(Path::to_path_buf))
        .unwrap_or_else(|| work_dir.to_path_buf())
}

pub(super) fn run_git_show(work_dir: &Path, commit: &str) -> Result<String> {
    // Defence-in-depth: the dispute RPC writes `evidence_commit` straight
    // through from the agent. A value starting with `-` (e.g.
    // `--output=/tmp/x`) would be parsed by `git show` as an option.
    // Require a SHA-shaped string (4–64 hex chars — 4 matches git's
    // minimum unambiguous short SHA) and pass `--` so any remaining
    // shape oddity still lands in the positional slot.
    let is_sha =
        commit.len() >= 4 && commit.len() <= 64 && commit.chars().all(|c| c.is_ascii_hexdigit());
    if !is_sha {
        anyhow::bail!("refusing git show: evidence_commit is not a SHA-shaped string");
    }
    let project_root = project_root_of(work_dir);
    let output = Command::new("git")
        .args(["show", "--no-color", "--stat", "-p", "--", commit])
        .current_dir(project_root)
        .output()
        .context("Failed to invoke git show")?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr).into_owned();
        anyhow::bail!("git show exited non-zero: {stderr}");
    }
    Ok(String::from_utf8_lossy(&output.stdout).into_owned())
}

pub(super) fn run_listing(work_dir: &Path) -> Result<String> {
    let project_root = project_root_of(work_dir);
    // Use `find` with maxdepth 3. If `find` is missing we degrade gracefully.
    let output = Command::new("find")
        .args([".", "-maxdepth", "3", "-not", "-path", "*/.*"])
        .current_dir(project_root)
        .output();
    match output {
        Ok(out) if out.status.success() => Ok(String::from_utf8_lossy(&out.stdout).into_owned()),
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            anyhow::bail!("find exited non-zero: {stderr}")
        }
        Err(e) => Err(anyhow::anyhow!("could not run find: {e}")),
    }
}

pub(super) fn read_plan_excerpt(plan_path: &Path, stage_id: &str) -> Result<String> {
    let raw = std::fs::read_to_string(plan_path).context("read plan")?;
    // Best-effort: extract the YAML block and surface the stage's
    // sub-document. If anything fails, return the entire (truncated)
    // file — the truncate_to_budget pass will keep us under the cap.
    if let Some(start) = raw.find("```yaml") {
        if let Some(end_rel) = raw[start + 7..].find("```") {
            let yaml = &raw[start + 7..start + 7 + end_rel];
            if let Some(stage_block) = find_stage_block(yaml, stage_id) {
                return Ok(stage_block);
            }
            return Ok(yaml.to_string());
        }
    }
    Ok(raw)
}

/// Find the YAML sub-block corresponding to a stage definition. The
/// extractor is intentionally string-based to avoid pulling in a full
/// YAML parser in the briefing path.
fn find_stage_block(yaml: &str, stage_id: &str) -> Option<String> {
    let needle = format!("id: {stage_id}");
    let pos = yaml.find(&needle)?;
    // Walk backwards to the start of the surrounding `- ` list item.
    let mut start = pos;
    for (i, ch) in yaml[..pos].char_indices().rev() {
        if ch == '\n' && (yaml[i + 1..].starts_with("    - ") || yaml[i + 1..].starts_with("  - "))
        {
            start = i + 1;
            break;
        }
    }
    // Forward until the next list-item marker at the same indent.
    let rest = &yaml[start..];
    let mut end = rest.len();
    let mut seen_first_newline = false;
    for (i, ch) in rest.char_indices() {
        if ch != '\n' {
            continue;
        }
        if !seen_first_newline {
            seen_first_newline = true;
            continue;
        }
        let after = &rest[i + 1..];
        if after.starts_with("    - ") || after.starts_with("  - ") {
            end = i;
            break;
        }
    }
    Some(rest[..end].trim_end().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn run_git_show_rejects_non_sha_evidence_commit() {
        // The agent supplies evidence_commit via the dispute RPC. A value
        // that doesn't look like a SHA must be rejected BEFORE git is
        // invoked, so option-injection (`--output=...`) cannot reach the
        // process. The SHA check also bounds the input to a small ASCII-
        // hex string so even creative byte sequences cannot become
        // arguments after the positional `--`.
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join(".loom").join("work");
        std::fs::create_dir_all(&work).unwrap();
        // Leading dash — classic option-injection attempt.
        let err = run_git_show(&work, "--output=/tmp/escape").unwrap_err();
        assert!(format!("{err:#}").contains("not a SHA-shaped string"));
        // Non-hex characters.
        let err = run_git_show(&work, "deadbeef; rm -rf /").unwrap_err();
        assert!(format!("{err:#}").contains("not a SHA-shaped string"));
        // Path-traversal shaped.
        let err = run_git_show(&work, "../etc/passwd").unwrap_err();
        assert!(format!("{err:#}").contains("not a SHA-shaped string"));
        // Empty.
        let err = run_git_show(&work, "").unwrap_err();
        assert!(format!("{err:#}").contains("not a SHA-shaped string"));
        // Too short (below git's 4-char short-SHA minimum). All-hex but
        // sub-minimum length must be rejected before git is invoked.
        for short in ["a", "ab", "abc"] {
            let err = run_git_show(&work, short).unwrap_err();
            assert!(
                format!("{err:#}").contains("not a SHA-shaped string"),
                "len {} should be rejected; got {err:#}",
                short.len(),
            );
        }
        // Too long (65 hex chars).
        let too_long = "a".repeat(65);
        let err = run_git_show(&work, &too_long).unwrap_err();
        assert!(format!("{err:#}").contains("not a SHA-shaped string"));
    }

    #[test]
    fn plan_excerpt_narrows_to_the_disputed_stage() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = tmp.path().join("PLAN.md");
        std::fs::write(
            &plan,
            "# Plan\n\n```yaml\nloom:\n  stages:\n    - id: other\n      name: other\n    - id: s1\n      name: mine\n```\n",
        )
        .unwrap();
        let excerpt = read_plan_excerpt(&plan, "s1").unwrap();
        assert!(excerpt.contains("id: s1"));
        assert!(!excerpt.contains("id: other"));
    }
}
