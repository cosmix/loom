//! Build the prompt sent to the Anthropic API for a single dispute.
//!
//! The prompt has two parts:
//! - `system`: rules the model must follow when emitting a verdict.
//! - `user`: the evidence (stage definition, plan acceptance criteria,
//!   diff of the evidence commit, worktree listing, failure output).
//!
//! The total prompt is hard-capped to roughly 100 KiB; oversized
//! diff/failure-output sections are truncated from the tail first so the
//! structured-instructions section at the top is always intact.

use anyhow::{Context, Result};
use std::path::Path;
use std::process::Command;

use crate::models::dispute::DisputeRequest;
use crate::models::stage::Stage;
use crate::plan::schema::AcceptanceCriterion;

/// Total prompt byte budget. The Anthropic API has a much higher input
/// limit; this cap exists so we never accidentally ship hundreds of KB
/// of diff into the prompt (which would slow inference and blow cost).
pub const MAX_PROMPT_BYTES: usize = 100_000;

const TRUNCATION_MARKER: &str = "\n... [truncated] ...\n";

/// The two halves of a fully assembled prompt, ready to be POSTed to
/// `/v1/messages`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Prompt {
    pub system: String,
    pub user: String,
}

impl Prompt {
    /// Total byte length of the assembled prompt. Used by tests + the
    /// truncation pass to enforce [`MAX_PROMPT_BYTES`].
    pub fn total_len(&self) -> usize {
        self.system.len() + self.user.len()
    }
}

/// Build the SYSTEM + USER prompt for the supplied dispute.
///
/// `plan_path` is the live plan markdown (used to surface acceptance
/// criteria as the agent sees them). `work_dir` is the `.work/` root
/// (only needed when we later need to read other on-disk evidence —
/// today the function uses it for `git show` working-tree context).
pub fn build(
    plan_path: &Path,
    stage: &Stage,
    dispute: &DisputeRequest,
    work_dir: &Path,
) -> Result<Prompt> {
    let system = build_system_prompt();
    let user = build_user_prompt(plan_path, stage, dispute, work_dir)?;
    let mut prompt = Prompt { system, user };
    truncate_to_budget(&mut prompt);
    Ok(prompt)
}

fn build_system_prompt() -> String {
    // The system prompt is intentionally terse and rule-shaped. It also
    // double-pins the JSON-only output requirement (the user prompt
    // repeats it). Anti-chatty wording: "Output ONLY the JSON object"
    // — no leading prose, no markdown code fences.
    let mut s = String::new();
    s.push_str("You are the Loom adjudicator. You judge whether an agent's dispute of\n");
    s.push_str("an acceptance criterion should be ACCEPTED, REJECTED, or whether more\n");
    s.push_str("EVIDENCE is required.\n\n");
    s.push_str("Verdict semantics:\n");
    s.push_str("- accept: the criterion is wrong (impossible / over-specified / mismatched\n");
    s.push_str("  to the actual goal); propose a plan_patch that fixes it.\n");
    s.push_str("- reject: the criterion is correct; the agent must fix the implementation.\n");
    s.push_str("- needs-more-evidence: cannot decide from supplied evidence; list the\n");
    s.push_str("  specific questions the agent must answer.\n\n");
    s.push_str("Citations on Accept/Reject MUST quote real lines from the supplied\n");
    s.push_str("diff/files. A citation has: file, line (optional), excerpt, claim.\n\n");
    s.push_str("Output ONLY a single valid JSON object — no prose, no markdown fences,\n");
    s.push_str("no comments. Schema:\n");
    s.push_str("{\n");
    s.push_str("  \"verdict\": \"accept\"|\"reject\"|\"needs-more-evidence\",\n");
    s.push_str("  \"reasoning\": \"...\" (required on accept/reject),\n");
    s.push_str("  \"citations\": [ {file, line?, excerpt, claim}, ... ] (accept/reject; ≥1),\n");
    s.push_str("  \"plan_patch\": { ...AmendmentRequest JSON... } (accept only),\n");
    s.push_str("  \"questions\": [\"...\", ...] (needs-more-evidence; ≥1)\n");
    s.push_str("}\n");
    s
}

fn build_user_prompt(
    plan_path: &Path,
    stage: &Stage,
    dispute: &DisputeRequest,
    work_dir: &Path,
) -> Result<String> {
    let mut u = String::new();
    u.push_str("# Dispute\n\n");
    u.push_str(&format!("Stage: {}\n", stage.id));
    u.push_str(&format!("Stage name: {}\n", stage.name));
    u.push_str(&format!("Criterion index: {}\n", dispute.criterion_index));
    if let Some(criterion) = stage.acceptance.get(dispute.criterion_index) {
        u.push_str(&format!(
            "Criterion command: `{}`\n",
            criterion.command().replace('`', "'")
        ));
    }
    u.push_str(&format!(
        "Fix attempts before dispute: {}\n\n",
        dispute.fix_attempts_at_dispute
    ));
    u.push_str("## Agent's reason\n\n");
    u.push_str(&dispute.reason);
    u.push_str("\n\n");

    u.push_str("## Stage acceptance criteria (all)\n\n");
    for (i, c) in stage.acceptance.iter().enumerate() {
        let marker = if i == dispute.criterion_index { "→" } else { " " };
        u.push_str(&format!("{marker} [{i}] {}\n", criterion_display(c)));
    }
    u.push('\n');

    if let Some(commit) = dispute.evidence_commit.as_deref() {
        u.push_str("## Evidence commit diff (git show)\n\n");
        u.push_str(&format!("Commit: {commit}\n\n"));
        let diff = run_git_show(work_dir, commit).unwrap_or_else(|e| {
            format!("(git show failed: {e})")
        });
        u.push_str("```diff\n");
        u.push_str(&diff);
        u.push_str("\n```\n\n");
    }

    if let Some(out) = dispute.failure_output.as_deref() {
        u.push_str("## Failure output (what the criterion produced)\n\n");
        u.push_str("```\n");
        u.push_str(out);
        u.push_str("\n```\n\n");
    }

    u.push_str("## Plan acceptance criteria source (from plan file)\n\n");
    let plan_excerpt = read_plan_excerpt(plan_path, &stage.id)
        .unwrap_or_else(|_| "(plan file not available)".to_string());
    u.push_str("```yaml\n");
    u.push_str(&plan_excerpt);
    u.push_str("\n```\n\n");

    u.push_str("## Worktree top-level files (3-deep listing)\n\n");
    let listing = run_listing(work_dir).unwrap_or_else(|e| format!("(listing failed: {e})"));
    u.push_str("```\n");
    u.push_str(&listing);
    u.push_str("\n```\n\n");

    u.push_str(
        "## Required output\n\n\
        Return a single JSON object per the schema in the system prompt.\n\
        Do NOT include prose outside the JSON.\n",
    );
    Ok(u)
}

fn criterion_display(c: &AcceptanceCriterion) -> String {
    c.command().to_string()
}

fn run_git_show(work_dir: &Path, commit: &str) -> Result<String> {
    // Defence-in-depth: the dispute RPC writes `evidence_commit` straight
    // through from the agent. A value starting with `-` (e.g.
    // `--output=/tmp/x`) would be parsed by `git show` as an option.
    // Require a SHA-shaped string (4–64 hex chars — 4 matches git's
    // minimum unambiguous short SHA) and pass `--` so any remaining
    // shape oddity still lands in the positional slot.
    let is_sha = commit.len() >= 4
        && commit.len() <= 64
        && commit.chars().all(|c| c.is_ascii_hexdigit());
    if !is_sha {
        anyhow::bail!("refusing git show: evidence_commit is not a SHA-shaped string");
    }
    let project_root = work_dir.parent().unwrap_or(work_dir);
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

fn run_listing(work_dir: &Path) -> Result<String> {
    let project_root = work_dir.parent().unwrap_or(work_dir);
    // Use `find` with maxdepth 3. If `find` is missing we degrade gracefully.
    let output = Command::new("find")
        .args([".", "-maxdepth", "3", "-not", "-path", "*/.*"])
        .current_dir(project_root)
        .output();
    match output {
        Ok(out) if out.status.success() => {
            Ok(String::from_utf8_lossy(&out.stdout).into_owned())
        }
        Ok(out) => {
            let stderr = String::from_utf8_lossy(&out.stderr).into_owned();
            anyhow::bail!("find exited non-zero: {stderr}")
        }
        Err(e) => Err(anyhow::anyhow!("could not run find: {e}")),
    }
}

fn read_plan_excerpt(plan_path: &Path, stage_id: &str) -> Result<String> {
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
/// YAML parser in the prompt path.
fn find_stage_block(yaml: &str, stage_id: &str) -> Option<String> {
    let needle = format!("id: {stage_id}");
    let pos = yaml.find(&needle)?;
    // Walk backwards to the start of the surrounding `- ` list item.
    let mut start = pos;
    for (i, ch) in yaml[..pos].char_indices().rev() {
        if ch == '\n' {
            if yaml[i + 1..].starts_with("    - ") || yaml[i + 1..].starts_with("  - ") {
                start = i + 1;
                break;
            }
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

/// Enforce [`MAX_PROMPT_BYTES`] by truncating the diff / failure-output
/// sections of the user prompt first, then the listing, then the plan
/// excerpt. The system prompt is never truncated.
fn truncate_to_budget(prompt: &mut Prompt) {
    if prompt.total_len() <= MAX_PROMPT_BYTES {
        return;
    }
    // Strategy: repeatedly halve the diff fence first, then the failure
    // output fence, then the listing fence, then the plan excerpt.
    let candidates = [
        "## Evidence commit diff (git show)",
        "## Failure output (what the criterion produced)",
        "## Worktree top-level files (3-deep listing)",
        "## Plan acceptance criteria source (from plan file)",
    ];
    for header in candidates {
        while prompt.total_len() > MAX_PROMPT_BYTES {
            if !halve_section(&mut prompt.user, header) {
                break;
            }
        }
        if prompt.total_len() <= MAX_PROMPT_BYTES {
            return;
        }
    }
    // Hard cap: if every section was already trimmed and we still
    // exceed budget, truncate the entire user prompt's tail.
    if prompt.total_len() > MAX_PROMPT_BYTES {
        let allowed_user = MAX_PROMPT_BYTES.saturating_sub(prompt.system.len());
        truncate_string(&mut prompt.user, allowed_user);
    }
}

/// Find a "## Header" section in `user` and halve the contents of the
/// first triple-backtick fence inside it. Returns `false` if no further
/// trimming is possible (no header, no fence, or fence already empty).
fn halve_section(user: &mut String, header: &str) -> bool {
    let Some(header_pos) = user.find(header) else {
        return false;
    };
    let after_header = header_pos + header.len();
    let fence_open_rel = match user[after_header..].find("```") {
        Some(p) => p,
        None => return false,
    };
    let fence_open = after_header + fence_open_rel;
    // Skip the rest of the fence-open line.
    let body_start = match user[fence_open..].find('\n') {
        Some(p) => fence_open + p + 1,
        None => return false,
    };
    let fence_close_rel = match user[body_start..].find("```") {
        Some(p) => p,
        None => return false,
    };
    let body_end = body_start + fence_close_rel;
    let body_len = body_end - body_start;
    if body_len <= TRUNCATION_MARKER.len() {
        return false;
    }
    // Keep the first half; replace the rest with the marker.
    let keep = body_len / 2;
    let mut new_body = String::with_capacity(keep + TRUNCATION_MARKER.len());
    let kept_slice = utf8_safe_prefix(&user[body_start..body_end], keep);
    new_body.push_str(kept_slice);
    if !new_body.ends_with('\n') {
        new_body.push('\n');
    }
    new_body.push_str(TRUNCATION_MARKER);
    user.replace_range(body_start..body_end, &new_body);
    true
}

/// Hard-truncate `s` to at most `max_bytes` bytes, respecting UTF-8
/// boundaries and leaving a trailing truncation marker so a downstream
/// reader sees the cut.
fn truncate_string(s: &mut String, max_bytes: usize) {
    if s.len() <= max_bytes {
        return;
    }
    let allowance = max_bytes.saturating_sub(TRUNCATION_MARKER.len());
    let prefix = utf8_safe_prefix(s, allowance).to_string();
    *s = prefix;
    s.push_str(TRUNCATION_MARKER);
}

fn utf8_safe_prefix(s: &str, max_bytes: usize) -> &str {
    if max_bytes >= s.len() {
        return s;
    }
    let mut idx = max_bytes;
    while idx > 0 && !s.is_char_boundary(idx) {
        idx -= 1;
    }
    &s[..idx]
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::plan::schema::AcceptanceCriterion;
    use chrono::Utc;

    fn stage_with_criteria(criteria: Vec<&str>) -> Stage {
        let mut stage = Stage::default();
        stage.id = "demo".to_string();
        stage.name = "Demo".to_string();
        stage.acceptance = criteria
            .into_iter()
            .map(|s| AcceptanceCriterion::Simple(s.to_string()))
            .collect();
        stage
    }

    fn dispute(criterion_index: usize) -> DisputeRequest {
        DisputeRequest {
            id: 1,
            stage_id: "demo".to_string(),
            criterion_index,
            reason: "criterion impossible".to_string(),
            evidence_commit: None,
            failure_output: Some("err: something broke".to_string()),
            fix_attempts_at_dispute: 2,
            created_at: Utc::now(),
        }
    }

    #[test]
    fn build_includes_stage_and_criterion() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = tmp.path().join("PLAN.md");
        std::fs::write(&plan, "stub plan").unwrap();
        let work = tmp.path().join(".work");
        std::fs::create_dir_all(&work).unwrap();
        let stage = stage_with_criteria(vec!["cargo test", "cargo clippy"]);
        let req = dispute(0);
        let p = build(&plan, &stage, &req, &work).unwrap();
        assert!(p.system.contains("adjudicator"));
        assert!(p.user.contains("Criterion command: `cargo test`"));
        assert!(p.user.contains("→ [0]"));
        assert!(p.user.contains("err: something broke"));
    }

    #[test]
    fn truncation_keeps_total_under_budget() {
        let tmp = tempfile::tempdir().unwrap();
        let plan = tmp.path().join("PLAN.md");
        std::fs::write(&plan, "stub plan".repeat(50_000)).unwrap();
        let work = tmp.path().join(".work");
        std::fs::create_dir_all(&work).unwrap();
        let stage = stage_with_criteria(vec!["cargo test"]);
        let mut req = dispute(0);
        req.failure_output = Some("err: ".repeat(50_000));
        req.evidence_commit = None;
        let p = build(&plan, &stage, &req, &work).unwrap();
        assert!(
            p.total_len() <= MAX_PROMPT_BYTES,
            "prompt {} exceeded {}",
            p.total_len(),
            MAX_PROMPT_BYTES,
        );
    }

    #[test]
    fn utf8_safe_prefix_does_not_split_multibyte() {
        let s = "héllo wörld";
        // 1 byte past the start of 'é' (UTF-8 2-byte char) — must back up.
        let prefix = utf8_safe_prefix(s, 2);
        assert!(s.starts_with(prefix));
        assert_eq!(prefix.len(), 1, "got: {prefix:?}");
    }

    #[test]
    fn run_git_show_rejects_non_sha_evidence_commit() {
        // The agent supplies evidence_commit via the dispute RPC. A value
        // that doesn't look like a SHA must be rejected BEFORE git is
        // invoked, so option-injection (`--output=...`) cannot reach the
        // process. The SHA check also bounds the input to a small ASCII-
        // hex string so even creative byte sequences cannot become
        // arguments after the positional `--`.
        let tmp = tempfile::tempdir().unwrap();
        let work = tmp.path().join(".work");
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
    fn halve_section_shrinks_diff_fence() {
        let mut u = String::from(
            "## Evidence commit diff (git show)\n\n```diff\n",
        );
        for _ in 0..1000 {
            u.push_str("- old\n+ new\n");
        }
        u.push_str("```\n");
        let before = u.len();
        assert!(halve_section(&mut u, "## Evidence commit diff (git show)"));
        assert!(u.len() < before);
        assert!(u.contains(TRUNCATION_MARKER));
    }
}
