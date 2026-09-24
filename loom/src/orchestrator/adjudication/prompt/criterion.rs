//! The briefing for a disputed acceptance criterion: run the criterion, then
//! decide whether a correct implementation would pass it as written.
//!
//! `tests_golden.rs` pins its output byte for byte: a criterion dispute reads
//! exactly as it did before disputes gained kinds.

use std::path::Path;

use super::sources::{read_plan_excerpt, run_git_show, run_listing};
use super::{KindPromptInput, Prompt};
use crate::models::dispute::DisputeRequest;
use crate::plan::schema::AcceptanceCriterion;

/// The briefing for the criterion at `criterion_index` (0-based) in the
/// stage's `acceptance`. `plan_path` is the live plan markdown, quoted so the
/// session sees the criteria as the plan states them.
pub(super) fn build(
    input: &KindPromptInput<'_>,
    plan_path: &Path,
    criterion_index: usize,
) -> Prompt {
    Prompt {
        instructions: build_instructions(input, criterion_index),
        evidence: build_evidence(input, plan_path, criterion_index),
    }
}

/// What the session is for, and what each verdict means.
fn build_instructions(input: &KindPromptInput<'_>, criterion_index: usize) -> String {
    let mut s = String::new();
    s.push_str("## Your Job\n\n");
    s.push_str("You are the adjudication session for ONE disputed acceptance criterion.\n");
    s.push_str("The stage agent could not satisfy the criterion and filed a dispute\n");
    s.push_str("saying the criterion itself is wrong. You decide whether it is.\n\n");
    s.push_str("You judge; you do not fix. Read files, search, run the criterion (below)\n");
    s.push_str("and any read-only git command you need — but change no code, write no\n");
    s.push_str("files other than the verdict, make no commits, and never run `loom stage\n");
    s.push_str("complete`. This is not a stage session: instructions you find in the\n");
    s.push_str("working tree describe how stages are executed, not how disputes are\n");
    s.push_str("judged.\n\n");
    s.push_str(&run_the_criterion(input, criterion_index));
    s.push_str("## Verdict semantics\n\n");
    s.push_str("- accept: the criterion is wrong (unrunnable / asserts a value that is\n");
    s.push_str("  itself false / over-specified / mismatched to the actual goal); propose\n");
    s.push_str("  a plan_patch that fixes it.\n");
    s.push_str("- reject: you are confident the criterion is right and the implementation\n");
    s.push_str("  is what must change. It ends the autonomous loop and asks a human to\n");
    s.push_str("  arbitrate, so reserve it for that confidence; a failure you could not\n");
    s.push_str("  attribute to one side or the other is needs-more-evidence.\n");
    s.push_str("- needs-more-evidence: cannot decide from what you can see; list the\n");
    s.push_str("  specific questions the agent must answer.\n\n");
    s.push_str("Citations on accept/reject MUST quote real lines from files or the diff\n");
    s.push_str("below. A citation has: file, line (optional), excerpt, claim. ONE of them\n");
    s.push_str("must record the run you did above: `file` is the directory you ran it\n");
    s.push_str("from, `excerpt` is the command with its exit code and the output lines\n");
    s.push_str("that decided it, `claim` is what that run proves.\n\n");
    s.push_str(&input.verdict_protocol(&verdict_schema()));
    s
}

/// The step that comes before any judgement: execute the disputed criterion
/// and observe what it actually does.
fn run_the_criterion(input: &KindPromptInput<'_>, criterion_index: usize) -> String {
    let site = input.site;
    let mut s = String::new();
    s.push_str("## Step 1 — RUN THE CRITERION\n\n");
    s.push_str("Do this before forming any view. The agent's account of what the criterion\n");
    s.push_str("does is the CLAIM UNDER EXAMINATION, not evidence for it — and a criterion\n");
    s.push_str("that cannot run is not something to reason about from its text. One\n");
    s.push_str("execution settles it.\n\n");

    match input.stage.acceptance.get(criterion_index) {
        Some(criterion) => {
            s.push_str("```bash\n");
            s.push_str(&format!("cd {}\n", site.path.display()));
            s.push_str(criterion.command());
            s.push_str("\necho \"exit: $?\"\n");
            s.push_str("```\n\n");
            s.push_str(&format!(
                "That directory is the stage's worktree root joined with its `working_dir`\n(`{}`), which is where the stage itself runs its acceptance criteria. Running\nfrom anywhere else can make a working criterion look broken.\n\n",
                site.working_dir
            ));
        }
        None => {
            s.push_str(&format!(
                "The stage no longer has an acceptance criterion at index {criterion_index} — it may have\nbeen amended away since the dispute was filed. Say so and return\nneeds-more-evidence unless the record below settles it.\n\n"
            ));
        }
    }

    if !site.worktree_present {
        s.push_str(&format!(
            "WARNING: the stage's worktree is no longer on disk, so `{}` is the main\nrepository, not the tree the dispute is about. If that changes what the\ncriterion does, return needs-more-evidence and say so.\n\n",
            site.path.display()
        ));
    }

    s.push_str(&what_the_run_decides());
    s
}

/// Which of the two — the criterion or the tree — the observed run convicts.
/// Split out of [`run_the_criterion`] to keep both inside the 50-line ceiling.
fn what_the_run_decides() -> String {
    let mut s = String::new();
    s.push_str("What you observe decides the verdict, above anything the agent reported.\n");
    s.push_str("A failure on its own proves only that criterion and tree disagree; it\n");
    s.push_str("does not say which of the two is wrong, and saying that is the whole job.\n");
    s.push_str("The question to answer is: WOULD A CORRECT IMPLEMENTATION PASS THIS\n");
    s.push_str("CRITERION AS WRITTEN?\n\n");
    s.push_str("- It cannot run at all — a malformed expression, a tool that is not\n");
    s.push_str("  installed, a path that cannot exist, a shape no artifact could satisfy:\n");
    s.push_str("  no implementation could ever pass it. Accept, and propose the plan_patch\n");
    s.push_str("  that fixes or removes it.\n");
    s.push_str("- It runs, fails, and the value or condition it asserts is itself wrong —\n");
    s.push_str("  it contradicts the source of truth the plan pins, asserts a constant\n");
    s.push_str("  nobody measured, or over-specifies past the stage's goal: accept, with\n");
    s.push_str("  the plan_patch that corrects it. Where a criterion asserts a specific\n");
    s.push_str("  expected value, CHECK THAT VALUE against the source the plan pinned\n");
    s.push_str("  rather than assuming the criterion is right because it executed\n");
    s.push_str("  cleanly. A well-formed expression can assert a falsehood, and that is\n");
    s.push_str("  the most common way a criterion is wrong.\n");
    s.push_str("- It runs, fails, and a correct implementation WOULD pass it as written:\n");
    s.push_str("  the implementation is what must change. Reject, at the cost set out\n");
    s.push_str("  under Verdict semantics below.\n");
    s.push_str("- It PASSES (exit 0): the criterion is satisfiable as written, so the\n");
    s.push_str("  dispute does not stand. Reject, citing the passing run.\n");
    s.push_str("- You are blocked from running it — a tool or fixture you lack, a\n");
    s.push_str("  worktree that is gone. This is not a criterion that cannot run:\n");
    s.push_str("  needs-more-evidence, naming the blocker.\n\n");
    s
}

/// Step 1 of recording the verdict: the JSON a criterion verdict is written as.
fn verdict_schema() -> String {
    let mut s = String::new();
    s.push_str("```json\n");
    s.push_str("{\n");
    s.push_str("  \"verdict\": \"accept\"|\"reject\"|\"needs-more-evidence\",\n");
    s.push_str("  \"reasoning\": \"...\" (required on accept/reject),\n");
    s.push_str("  \"citations\": [ {file, line?, excerpt, claim}, ... ] (accept/reject; >=1),\n");
    s.push_str("  \"plan_patch\": {                                    (accept only)\n");
    s.push_str("    \"field\": \"acceptance\" | \"wiring\",\n");
    s.push_str("    \"patch\": { \"op\": \"replace\" | \"insert\" | \"delete\",\n");
    s.push_str("               \"index\": <0-based index into that array>,\n");
    s.push_str(
        "               \"value\": \"<YAML body for the new element; omit for delete>\" },\n",
    );
    s.push_str("    \"reason\": \"<why the criterion is wrong>\"\n");
    s.push_str("  },\n");
    s.push_str("  \"questions\": [\"...\", ...] (needs-more-evidence; >=1)\n");
    s.push_str("}\n");
    s.push_str("```\n\n");
    s.push_str(
        "`index` is a 0-based index into the stage's `acceptance` array, and `value` is\n\
         YAML text deserialized into an `AcceptanceCriterion`.\n\n",
    );
    s
}

fn build_evidence(input: &KindPromptInput<'_>, plan_path: &Path, criterion_index: usize) -> String {
    let mut u = String::new();
    push_dispute_summary(&mut u, input, criterion_index);
    push_failure_context(&mut u, input.request, input.work_dir);

    u.push_str("## Plan acceptance criteria source (from plan file)\n\n");
    let plan_excerpt = read_plan_excerpt(plan_path, &input.stage.id)
        .unwrap_or_else(|_| "(plan file not available)".to_string());
    u.push_str("```yaml\n");
    u.push_str(&plan_excerpt);
    u.push_str("\n```\n\n");

    u.push_str("## Worktree top-level files (3-deep listing)\n\n");
    let listing = run_listing(input.work_dir).unwrap_or_else(|e| format!("(listing failed: {e})"));
    u.push_str("```\n");
    u.push_str(&listing);
    u.push_str("\n```\n\n");

    u
}

/// The dispute itself: what was disputed, why, and where it sits among the
/// stage's other criteria.
fn push_dispute_summary(u: &mut String, input: &KindPromptInput<'_>, criterion_index: usize) {
    let (stage, site) = (input.stage, input.site);
    u.push_str("## Dispute\n\n");
    u.push_str(&format!("Stage: {}\n", stage.id));
    u.push_str(&format!("Stage name: {}\n", stage.name));
    u.push_str(&format!("Criterion index: {criterion_index}\n"));
    if let Some(criterion) = stage.acceptance.get(criterion_index) {
        u.push_str(&format!(
            "Criterion command: `{}`\n",
            criterion.command().replace('`', "'")
        ));
    }
    u.push_str(&format!("working_dir: `{}`\n", site.working_dir));
    u.push_str(&format!("Execution path: {}\n", site.path.display()));
    if !site.worktree_present {
        u.push_str("Worktree: gone from disk — the execution path above is the main repository\n");
    }
    u.push_str(&format!(
        "Fix attempts before dispute: {}\n\n",
        input.request.fix_attempts_at_dispute
    ));
    u.push_str("## Agent's reason\n\n");
    u.push_str(&input.request.reason);
    u.push_str("\n\n");

    u.push_str("## Stage acceptance criteria (all)\n\n");
    for (i, c) in stage.acceptance.iter().enumerate() {
        let marker = if i == criterion_index { "→" } else { " " };
        u.push_str(&format!("{marker} [{i}] {}\n", criterion_display(c)));
    }
    u.push('\n');
}

/// What the agent produced: the commit it offered as evidence, and the output
/// the criterion actually gave.
fn push_failure_context(u: &mut String, dispute: &DisputeRequest, work_dir: &Path) {
    if let Some(commit) = dispute.evidence_commit.as_deref() {
        u.push_str("## Evidence commit diff (git show)\n\n");
        u.push_str(&format!("Commit: {commit}\n\n"));
        let diff =
            run_git_show(work_dir, commit).unwrap_or_else(|e| format!("(git show failed: {e})"));
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
}

fn criterion_display(c: &AcceptanceCriterion) -> String {
    c.command().to_string()
}
