//! The briefing for a disputed artifact pattern. It follows the acceptance
//! briefing (`criterion.rs`) and reuses its verdict rules, its reading of the
//! run and its evidence sections; "the criterion" in that shared text means
//! the disputed pattern. What differs is the entry (a glob, not a command),
//! how to observe it from the execution site, and the value a verdict puts in
//! the list: a plain glob string, not YAML.

use std::path::Path;

use super::criterion::{
    index_list, judge_every_failing, judge_not_fix, push_dispute_header, push_entry_list,
    push_failure_context, push_plan_and_listing, run_before_judging, schema_json, verdict_rules,
    what_the_run_decides, worktree_gone_warning, PATCHES_RULE,
};
use super::{KindPromptInput, Prompt};
use crate::models::dispute::CriterionField;
use crate::testrun::command::shell_quote;

/// The noun the shared text pluralises or repeats for this list.
const NOUN: &str = "artifact pattern";

/// The briefing for artifact patterns `indices` (0-based) of the stage's
/// `artifacts`.
pub(super) fn build(input: &KindPromptInput<'_>, plan_path: &Path, indices: &[usize]) -> Prompt {
    Prompt {
        instructions: build_instructions(input, indices),
        evidence: build_evidence(input, plan_path, indices),
    }
}

/// What the session is for, how to observe the patterns, and what each
/// verdict means.
fn build_instructions(input: &KindPromptInput<'_>, indices: &[usize]) -> String {
    let mut s = String::from("## Your Job\n\n");
    s.push_str(&format!(
        "You are the adjudication session for the disputed {NOUN}s: entries {} of\n\
         the stage's `artifacts` list. The stage agent could not satisfy them and filed a\n\
         dispute saying each is itself wrong. You decide, entry by entry, whether it\n\
         is. Below, \"the criterion\" means each disputed {NOUN}.\n\n",
        index_list(indices)
    ));
    s.push_str(&judge_not_fix());
    s.push_str(&run_the_patterns(input, indices));
    s.push_str(&verdict_rules(&input.stage.id));
    s.push_str(&input.verdict_protocol(&verdict_schema()));
    s
}

/// Step 1: run the stage's checks, then list what each disputed pattern
/// matches from the directory the check resolves it against.
fn run_the_patterns(input: &KindPromptInput<'_>, indices: &[usize]) -> String {
    let site = input.site;
    let mut s = format!("## Step 1 — RUN THE {}S\n\n", NOUN.to_uppercase());
    s.push_str(&run_before_judging());
    s.push_str(&format!(
        "First run `loom check {} --no-cache` from {} and read its `[artifact n]`\n\
         lines; it runs the stage's checks in a daemon host job, so run it again while\n\
         its first line reads `Host job: requested` or `pending`.\n\n",
        input.stage.id,
        shell_quote(&site.path.to_string_lossy())
    ));
    s.push_str(&judge_every_failing(NOUN));
    s.push_str("List what each disputed pattern matches from that directory:\n\n");
    for &index in indices {
        match input.stage.artifacts.get(index) {
            Some(pattern) => s.push_str(&format!(
                "```bash\n# artifact pattern {index}\ncd {}\nrg --files --no-ignore --hidden --glob {} .\n\
                 echo \"exit: $?\"\n```\n\n",
                site.path.display(),
                shell_quote(pattern)
            )),
            None => s.push_str(&format!(
                "The stage no longer has an artifact pattern at index {index} — it may have\n\
                 been amended away since the dispute was filed. Say so, and patch nothing at\n\
                 that index.\n\n"
            )),
        }
    }
    s.push_str(&format!(
        "That directory is the stage's worktree root joined with its `working_dir`\n\
         (`{}`), which the check resolves each pattern against. A pattern passes when it\n\
         matches at least one file, no matched file is empty, and no matched file outside\n\
         markdown holds a stub marker (a placeholder comment, an unimplemented macro,\n\
         a not-implemented exception and the like). `rg --glob` reads a pattern without\n\
         a `/` at any depth, which the check does not, so trust the check's own lines\n\
         where the two differ.\n\n",
        site.working_dir
    ));
    s.push_str(&worktree_gone_warning(site));
    s.push_str(&what_the_run_decides());
    s
}

/// Step 1 of recording the verdict: the criterion schema with `field` pinned
/// to `artifacts`, and what a patch `value` is for this list.
fn verdict_schema() -> String {
    let field = CriterionField::Artifacts.as_str();
    let mut s = schema_json(&format!("\"{field}\""));
    s.push_str(&format!(
        "`field` must be `\"{field}\"`: a verdict on this dispute may amend only the\n\
         stage's `artifacts` list. `index` is a 0-based index into that list.\n\n\
         For this list a patch `value` is a plain glob string, taken verbatim (trimmed,\n\
         never parsed as YAML), not the YAML body the schema comment above names: write\n\
         `docs/*.md`, not `\"docs/*.md\"`. It must be a valid glob that names a\n\
         path inside the stage's working directory (no `..`, no absolute path).\n\n"
    ));
    s.push_str(PATCHES_RULE);
    s
}

/// The dispute, the whole list with the disputed entries marked, what the
/// agent produced, and the plan and tree.
fn build_evidence(input: &KindPromptInput<'_>, plan_path: &Path, indices: &[usize]) -> String {
    let mut disputed = format!(
        "Field: {}\nIndices: {}\n",
        CriterionField::Artifacts.as_str(),
        index_list(indices)
    );
    for &index in indices {
        if let Some(pattern) = input.stage.artifacts.get(index) {
            disputed.push_str(&format!(
                "Disputed {NOUN} {index}: `{}`\n",
                pattern.replace('`', "'")
            ));
        }
    }
    let lines: Vec<String> = input.stage.artifacts.clone();
    let mut u = String::new();
    push_dispute_header(&mut u, input, &disputed);
    push_entry_list(&mut u, "Stage artifact patterns (all)", indices, &lines);
    push_failure_context(&mut u, input.request, input.work_dir);
    push_plan_and_listing(
        &mut u,
        input,
        plan_path,
        &input.stage.id,
        "Plan stage source",
    );
    u
}
