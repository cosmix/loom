//! Text for `loom knowledge bootstrap`: the terminal report, the session
//! brief, the appended system prompt and the claude argv. Everything here is
//! a pure function of its inputs, so the text is testable without a graph or
//! a session.

use std::path::Path;

use super::clusters::{Cluster, KNOWLEDGE_PREFIX};
use super::receipt::{ClusterStatus, RefreshPlan};
use super::WorkItem;
use crate::fs::knowledge::KnowledgeFile;

/// Tools the session may not use; knowledge goes through the loom CLI. One
/// comma-joined value, because `--disallowedTools` is variadic.
const DISALLOWED_TOOLS: &str = "Edit,Write,NotebookEdit";

const SESSION_RULES: &str = "\
- You are bootstrapping this repository's loom knowledge base (`doc/loom/knowledge/`) for future coding agents. Read the brief file named in the first message in full before anything else.
- Write knowledge ONLY with `loom knowledge update`, `loom knowledge replace-section` and `loom knowledge annotate`, run from the repository root. File-editing tools are disabled. Never modify source files, commit, or create branches.
- Before writing, read `doc/loom/knowledge/INDEX.md` and the existing sections you would touch. Keep human-written entries. Correct a stale claim with `replace-section` and name the wrong claim in the replacement.
- Every file except `mistakes.md` and `mistakes/*` states current truth only: no dated headings, no \"was\"/\"used to\" history, no change log. A lesson worth keeping goes into a mistakes topic; the current-state entry links to it instead of retelling it.
- Tier routing: a finding of about 40 lines or fewer goes into its tier-1 file (architecture, entry-points, patterns, conventions, mistakes, stack, concerns). A larger one goes to `loom knowledge update <category>/<slug>`, plus `loom knowledge annotate <category>/<slug> --blurb \"<at most 80 chars>\"`, plus a 2-4 line summary and link in the tier-1 file.
- Every claim cites `path:line` evidence that you or a subagent read. Record only durable facts: architecture, entry points, patterns, conventions, stack, real concerns. Leave out anything git history or a quick grep answers.";

const PROCEDURE: &str = "\
## Procedure

1. Read `doc/loom/knowledge/INDEX.md` and the tier-1 files.
2. Group the work-set clusters into at most 6 assignments of at most about 120 files each, keeping neighbouring directories together. If more work remains, run another wave after the first returns.
3. Spawn one `Explore` subagent per assignment, all in one message. Each explores its clusters with `loom map --outline <file>`, `loom map --impact <symbol|path>` and `loom map --find-all <symbol>` before opening files. Each RETURNS proposed entries (target file, heading, body, evidence `path:line`) and does NOT write knowledge itself.
4. Merge the proposals: drop duplicates, add cross-cluster links, decide tier routing, then write with the loom knowledge CLI.
5. Fill every tier-1 gap listed above.
6. Run `loom knowledge check`; fix any issue you introduced.
7. Touch the marker (the system prompt names it).
";

/// Everything the report and the brief describe.
pub(super) struct PlanSummary<'a> {
    pub repo_root: &'a Path,
    pub source_revision: &'a str,
    /// `CoverageReport`'s one-line summary.
    pub coverage: &'a str,
    pub refresh: bool,
    /// Every current cluster, in `plan.statuses` order.
    pub clusters: &'a [Cluster],
    pub plan: &'a RefreshPlan,
    pub work: &'a [WorkItem<'a>],
    pub gaps: &'a [KnowledgeFile],
}

/// argv (after the binary) for the session. `--append-system-prompt` sits
/// between the variadic `--disallowedTools` and the positional prompt, so
/// the prompt is not swallowed as another tool name.
pub(super) fn claude_args(brief: &Path, marker: &Path, model: &str, effort: &str) -> Vec<String> {
    vec![
        "--permission-mode".to_string(),
        "auto".to_string(),
        "--model".to_string(),
        model.to_string(),
        "--effort".to_string(),
        effort.to_string(),
        "--disallowedTools".to_string(),
        DISALLOWED_TOOLS.to_string(),
        "--append-system-prompt".to_string(),
        system_prompt(marker),
        initial_prompt(brief),
    ]
}

/// The rules appended to Claude's system prompt, ending with the completion
/// signal the driver waits for.
pub(super) fn system_prompt(marker: &Path) -> String {
    format!("{SESSION_RULES}\n- {}", completion_instruction(marker))
}

/// The session's first message.
pub(super) fn initial_prompt(brief: &Path) -> String {
    format!(
        "Read {} in full, then bootstrap the knowledge base as it instructs.",
        brief.display()
    )
}

/// `commands/pressure/spawn.rs::completion_instruction`, worded for bootstrap.
fn completion_instruction(marker: &Path) -> String {
    format!(
        "AUTONOMOUS RUN: this Claude session was launched by `loom knowledge bootstrap`; no human will end it for you. \
         When the knowledge base is FULLY written (after every subagent has finished) and `loom knowledge check` shows \
         no new structural issue you caused, your FINAL action MUST be to run exactly this shell command and nothing \
         after it: touch {}. Do not run it earlier. That path is inside the repo's gitignored `.loom/work/` because the \
         agent sandbox mounts /tmp read-only; creating this one marker is the sanctioned exception to the rule against \
         writing under `.loom/work/` directly. Once that file exists the driver closes this session.",
        marker.display()
    )
}

/// The terminal plan: every cluster (status only with `--refresh`), removed
/// ids, tier-1 gaps, and the work estimate.
pub(super) fn render_report(summary: &PlanSummary<'_>) -> String {
    let width = summary
        .clusters
        .iter()
        .map(|cluster| cluster.id.len())
        .fold("cluster".len(), usize::max);
    let status_head = summary.refresh.then_some("status");
    let mut out = report_row(
        width,
        "cluster",
        "files",
        "symbols",
        status_head,
        "hot files",
    );
    for (cluster, (_, status)) in summary.clusters.iter().zip(&summary.plan.statuses) {
        out.push_str(&report_row(
            width,
            &cluster.id,
            &cluster.files.len().to_string(),
            &cluster.symbols.to_string(),
            summary.refresh.then(|| status_label(*status)),
            &hot_files(cluster),
        ));
    }
    let removed = summary.plan.removed.iter().map(String::as_str);
    let gaps = summary.gaps.iter().map(|file| file.filename());
    out.push_str(&format!("\nremoved clusters: {}\n", list_or_none(removed)));
    out.push_str(&format!("tier-1 gaps: {}\n", list_or_none(gaps)));
    out.push_str(&format!("{}\n", work_estimate(summary)));
    out
}

fn report_row(
    width: usize,
    id: &str,
    files: &str,
    symbols: &str,
    status: Option<&str>,
    hot: &str,
) -> String {
    let status = status.map(|s| format!("{s:<9}  ")).unwrap_or_default();
    format!("{id:<width$}  {files:>5}  {symbols:>7}  {status}{hot}\n")
}

fn work_estimate(summary: &PlanSummary<'_>) -> String {
    let files: usize = summary
        .work
        .iter()
        .map(|item| item.cluster.files.len())
        .sum();
    let symbols: usize = summary.work.iter().map(|item| item.cluster.symbols).sum();
    format!(
        "work: {} clusters, {files} files, {symbols} symbols, {} template-only tier-1 files",
        summary.work.len(),
        summary.gaps.len()
    )
}

/// The session brief, written to a file the first message names.
pub(super) fn render_brief(summary: &PlanSummary<'_>) -> String {
    let mode = if summary.refresh { "refresh" } else { "full" };
    let mut out = format!(
        "# Knowledge bootstrap brief\n\n\
         - Repository root: `{}`\n\
         - Source revision: `{}`\n\
         - Mode: {mode}\n\
         - {}\n\n",
        summary.repo_root.display(),
        summary.source_revision,
        summary.coverage
    );
    out.push_str(&brief_work_set(summary));
    out.push_str(&brief_removed(&summary.plan.removed));
    out.push_str(&brief_gaps(summary.gaps));
    out.push_str(PROCEDURE);
    out
}

fn brief_work_set(summary: &PlanSummary<'_>) -> String {
    let ids: Vec<String> = summary
        .clusters
        .iter()
        .map(|cluster| format!("`{}`", cluster.id))
        .collect();
    let mut out = format!(
        "## Work set\n\n\
         A cluster owns the files under its directory that no deeper cluster owns. \
         Current clusters: {}.\n\n",
        ids.join(", ")
    );
    if summary.work.is_empty() {
        out.push_str("No cluster needs exploring.\n\n");
        return out;
    }
    let (status_head, status_rule) = if summary.refresh {
        (" Status |", " --- |")
    } else {
        ("", "")
    };
    out.push_str(&format!(
        "| Cluster | Files | Symbols |{status_head} Hot files |\n\
         | --- | --- | --- |{status_rule} --- |\n"
    ));
    for item in summary.work {
        let status = if summary.refresh {
            format!(" {} |", status_label(item.status))
        } else {
            String::new()
        };
        let cluster = item.cluster;
        out.push_str(&format!(
            "| `{}` | {} | {} |{status} {} |\n",
            cluster.id,
            cluster.files.len(),
            cluster.symbols,
            hot_files(cluster)
        ));
    }
    out.push('\n');
    out
}

fn brief_removed(removed: &[String]) -> String {
    let mut out = String::from("## Removed clusters\n\n");
    if removed.is_empty() {
        out.push_str("None.\n\n");
        return out;
    }
    out.push_str(
        "These ids are no longer a cluster (deleted, or re-partitioned into the clusters above); \
         verify knowledge about these paths:\n\n",
    );
    for id in removed {
        out.push_str(&format!("- removed: {id}\n"));
    }
    out.push('\n');
    out
}

fn brief_gaps(gaps: &[KnowledgeFile]) -> String {
    let mut out = String::from("## Tier-1 gaps\n\n");
    if gaps.is_empty() {
        out.push_str("None.\n\n");
        return out;
    }
    out.push_str("These tier-1 files still hold only their template; fill each one:\n\n");
    for file in gaps {
        out.push_str(&format!("- `{KNOWLEDGE_PREFIX}{}`\n", file.filename()));
    }
    out.push('\n');
    out
}

fn status_label(status: ClusterStatus) -> &'static str {
    match status {
        ClusterStatus::New => "new",
        ClusterStatus::Changed => "changed",
        ClusterStatus::Unchanged => "unchanged",
    }
}

/// `path (fan-in)` for each hot file, or `-` when the cluster has none.
fn hot_files(cluster: &Cluster) -> String {
    if cluster.hot_files.is_empty() {
        return "-".to_string();
    }
    let labels: Vec<String> = cluster
        .hot_files
        .iter()
        .map(|(path, fan_in)| format!("{path} ({fan_in})"))
        .collect();
    labels.join(", ")
}

fn list_or_none<'a>(items: impl Iterator<Item = &'a str>) -> String {
    let items: Vec<&str> = items.collect();
    if items.is_empty() {
        "none".to_string()
    } else {
        items.join(", ")
    }
}
