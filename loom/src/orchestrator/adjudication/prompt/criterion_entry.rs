//! The briefing for a disputed wiring check or wiring test. It follows the
//! acceptance briefing (`criterion.rs`) and reuses its verdict rules, its
//! reading of the run, its schema and its evidence sections; "the criterion"
//! in that shared text means the disputed entry. What differs is the entry
//! itself, how to observe it from the execution site, and the one list a
//! verdict on it may amend.

use std::path::Path;

use super::criterion::{
    index_list, judge_every_failing, judge_not_fix, push_dispute_header, push_entry_list,
    push_failure_context, push_plan_and_listing, run_before_judging, schema_json, verdict_rules,
    what_the_run_decides, worktree_gone_warning, PATCHES_RULE,
};
use super::execution_site::ExecutionSite;
use super::{KindPromptInput, Prompt};
use crate::models::dispute::CriterionField;
use crate::models::stage::{Stage, WiringCheck, WiringTest};
use crate::testrun::command::shell_quote;
use crate::verify::transitions::load_stage;

/// How a plan version 2 stage reads a wiring check (`verify/goal_backward/wiring_v2.rs`).
const V2_RULES: &str = "\
This stage's plan is version 2. A `source` holding `*`, `?` or `[` is a glob
expanded from that directory with no ignore rules, so a glob with no `/` names
only that directory's own entries (rg matches it at any depth).
`literal: true` matches `pattern` as plain text. A line that defines the name
the pattern finds does not count: a pattern that finds only the item's own
definition is a gap, so the check fails when every line rg prints is such a
definition, even though rg exits 0.\n\n";

/// How a plan version 1 stage reads a wiring check.
const V1_RULES: &str = "\
This stage's plan is version 1: `source` is one literal path, and `pattern` is
always a regex (`literal` is ignored).\n\n";

/// The disputed list and entries: everything the briefing says that depends
/// on which list it is.
struct Entry {
    /// `Wiring` or `WiringTests`; its `as_str()` is the patch's `field`.
    field: CriterionField,
    /// `wiring check` or `wiring test`.
    noun: &'static str,
    /// The list as the plan's YAML names it: `wiring` or `wiring_tests`.
    list: &'static str,
    /// The type a patch `value` deserializes into, with its keys.
    value_type: &'static str,
    /// The disputed entries' 0-based indices into the list.
    indices: Vec<usize>,
    /// Step 1's body per disputed index: the entry and how to observe it.
    /// `None` when the index names no entry.
    observe: Vec<(usize, Option<String>)>,
    /// One line per entry of the list.
    lines: Vec<String>,
    /// The completed stage that owns the list, when the integration-verify
    /// stage disputes an entry its aggregated wiring re-verification failed.
    owner: Option<String>,
}

impl Entry {
    /// `the stage's` or `stage 'a''s`, before the list's name.
    fn whose(&self) -> String {
        match &self.owner {
            Some(owner) => format!("stage '{owner}''s"),
            None => "the stage's".to_string(),
        }
    }
}

/// The briefing for wiring checks `indices` (0-based) of the stage's
/// `wiring`, or of `owner`'s when the integration-verify stage disputes a
/// completed stage's entry. An owner that cannot be read degrades to an
/// empty list, so every index reads as gone.
pub(super) fn build_wiring(
    input: &KindPromptInput<'_>,
    plan_path: &Path,
    indices: &[usize],
    owner: Option<&str>,
) -> Prompt {
    let owner_stage = owner.and_then(|id| load_stage(id, input.work_dir).ok());
    let wiring = match (owner, &owner_stage) {
        (None, _) => input.stage.wiring.clone(),
        (Some(_), Some(owner_stage)) => owner_stage.wiring.clone(),
        (Some(_), None) => Vec::new(),
    };
    let owner_site = owner_stage.as_ref().map(|o| owner_site(input.site, o));
    // The filing stage still records the verdict; only where its entries are
    // observed moves to the owner's directory.
    let observed = KindPromptInput {
        site: owner_site.as_ref().unwrap_or(input.site),
        ..*input
    };
    let entry = Entry {
        field: CriterionField::Wiring,
        noun: "wiring check",
        list: "wiring",
        value_type: "`WiringCheck` (keys `source`, `pattern`, `description`, optional `literal`)",
        indices: indices.to_vec(),
        observe: observe_each(indices, |i| {
            wiring.get(i).map(|c| observe_check(&observed, c))
        }),
        lines: wiring.iter().map(check_line).collect(),
        owner: owner.map(str::to_string),
    };
    assemble(&observed, plan_path, &entry)
}

/// Where the aggregated re-verification runs `owner`'s wiring: the
/// integration-verify worktree root joined with the OWNER's `working_dir`, the
/// directory its `source` paths are relative to.
fn owner_site(site: &ExecutionSite, owner: &Stage) -> ExecutionSite {
    let working_dir = owner.working_dir.clone().unwrap_or_else(|| ".".into());
    let path = if working_dir == "." {
        site.root.clone()
    } else {
        site.root.join(&working_dir)
    };
    ExecutionSite {
        path,
        working_dir,
        ..site.clone()
    }
}

/// The briefing for wiring tests `indices` (0-based) of the stage's
/// `wiring_tests`.
pub(super) fn build_wiring_test(
    input: &KindPromptInput<'_>,
    plan_path: &Path,
    indices: &[usize],
) -> Prompt {
    let tests = &input.stage.wiring_tests;
    let entry = Entry {
        field: CriterionField::WiringTests,
        noun: "wiring test",
        list: "wiring_tests",
        value_type:
            "`WiringTest` (keys `name`, `command`, `success_criteria`, optional `description`)",
        indices: indices.to_vec(),
        observe: observe_each(indices, |i| tests.get(i).map(|t| observe_test(input, t))),
        lines: tests.iter().map(test_line).collect(),
        owner: None,
    };
    assemble(input, plan_path, &entry)
}

fn observe_each(
    indices: &[usize],
    observe: impl Fn(usize) -> Option<String>,
) -> Vec<(usize, Option<String>)> {
    indices
        .iter()
        .map(|&index| (index, observe(index)))
        .collect()
}

fn assemble(input: &KindPromptInput<'_>, plan_path: &Path, entry: &Entry) -> Prompt {
    Prompt {
        instructions: build_instructions(input, entry),
        evidence: build_evidence(input, plan_path, entry),
    }
}

/// What the session is for, how to observe the entries, and what each
/// verdict means.
fn build_instructions(input: &KindPromptInput<'_>, entry: &Entry) -> String {
    let noun = entry.noun;
    let mut s = String::from("## Your Job\n\n");
    s.push_str(&format!(
        "You are the adjudication session for the disputed {noun}s: entries {} of\n\
         {} `{}` list. The stage agent could not satisfy them and filed a\n\
         dispute saying each is itself wrong. You decide, entry by entry, whether it\n\
         is. Below, \"the criterion\" means each disputed {noun}.\n\n",
        index_list(&entry.indices),
        entry.whose(),
        entry.list
    ));
    s.push_str(&judge_not_fix());
    s.push_str(&format!("## Step 1 — RUN THE {}S\n\n", noun.to_uppercase()));
    s.push_str(&run_before_judging());
    s.push_str(&match &entry.owner {
        Some(owner) => owner_run_note(&input.stage.id, owner),
        None => own_run_note(input, noun),
    });
    for (index, observe) in &entry.observe {
        match observe {
            Some(observe) => {
                s.push_str(&format!("Disputed {noun} {index}:\n\n"));
                s.push_str(observe);
            }
            None => s.push_str(&format!(
                "The stage no longer has a {noun} at index {index} — it may have been\n\
                 amended away since the dispute was filed. Say so, and patch nothing at\n\
                 that index.\n\n"
            )),
        }
    }
    s.push_str(&worktree_gone_warning(input.site));
    s.push_str(&what_the_run_decides());
    s.push_str(&verdict_rules(&input.stage.id));
    s.push_str(&input.verdict_protocol(&verdict_schema(entry)));
    s
}

/// How to run the stage's own entries: `loom check` runs them, then every
/// failing entry is judged.
fn own_run_note(input: &KindPromptInput<'_>, noun: &str) -> String {
    let mut s = format!(
        "First run `loom check {} --no-cache` from {} and inspect the reported\n\
         results; it runs them in a daemon host job, so run it again while its first line\n\
         reads `Host job: requested` or `pending`. Use the direct checks below to\n\
         diagnose differences.\n\n",
        input.stage.id,
        shell_quote(&input.site.path.to_string_lossy())
    );
    s.push_str(&judge_every_failing(noun));
    s
}

/// How to observe a completed stage's wiring the integration-verify stage
/// re-verifies: `loom check` never runs that re-verification, so its green
/// result decides nothing about these entries, and the failing entries to
/// judge are the ones the re-verification reports.
fn owner_run_note(stage_id: &str, owner: &str) -> String {
    format!(
        "These entries belong to completed stage '{owner}'. Integration-verify stage\n\
         '{stage_id}' re-verifies them on its merged tree only inside `loom stage complete`\n\
         (aggregated wiring re-verification); `loom check {stage_id}` does NOT run them,\n\
         so a passing `loom check` says nothing about these entries. Judge each from the\n\
         direct search below, run in the integration-verify worktree.\n\n\
         Judge every disputed wiring check AND every other check of stage '{owner}'\n\
         the re-verification reports failing, each a `{owner}: [wiring n]` line of\n\
         `loom stage complete` (quoted under Failure output when the agent attached\n\
         it); a verdict may patch any of them. The aggregate command exit status\n\
         decides nothing.\n\n"
    )
}

/// Step 1's body for a wiring check: the entry, the search that does what the
/// check does, and the rules the stage's plan version applies.
fn observe_check(input: &KindPromptInput<'_>, check: &WiringCheck) -> String {
    let v2 = input.stage.plan_version == 2;
    let glob = v2 && check.source.contains(['*', '?', '[']);
    let mut s = String::new();
    s.push_str(&format!("- source: {}\n", code(&check.source)));
    s.push_str(&format!("- pattern: {}\n", code(&check.pattern)));
    s.push_str(&format!("- literal: {}\n", check.literal));
    s.push_str(&format!("- description: {}\n\n", check.description));
    s.push_str("Search the way the check does:\n\n```bash\n");
    s.push_str(&format!("cd {}\n", input.site.path.display()));
    s.push_str(&search_command(check, glob, v2 && check.literal));
    s.push_str("\necho \"exit: $?\"\n```\n\n");
    s.push_str(&format!(
        "`source` is relative to that directory, the worktree root joined with the\n\
         `working_dir` (`{}`) of the stage that owns the check. The check reads each file whole, so a pattern that\n\
         spans lines needs `rg -U` to match as it does.\n\n",
        input.site.working_dir
    ));
    s.push_str(if v2 { V2_RULES } else { V1_RULES });
    s
}

/// The `rg` search for what `check` looks for: `-F` when the pattern is
/// literal, `--glob` over the directory when `source` is a glob.
fn search_command(check: &WiringCheck, glob: bool, literal: bool) -> String {
    let flags = if literal { "-n -F" } else { "-n" };
    let pattern = shell_quote(&check.pattern);
    let source = shell_quote(&check.source);
    if glob {
        format!("rg {flags} -e {pattern} --no-ignore --hidden --glob {source} .")
    } else {
        format!("rg {flags} -e {pattern} -- {source}")
    }
}

/// Step 1's body for a wiring test: the entry, and its command run from the
/// execution site.
fn observe_test(input: &KindPromptInput<'_>, test: &WiringTest) -> String {
    let criteria = serde_json::to_string(&test.success_criteria).unwrap_or_default();
    let mut s = String::new();
    s.push_str(&format!("- name: {}\n", test.name));
    s.push_str(&format!("- command: {}\n", code(&test.command)));
    s.push_str(&format!("- success_criteria: {}\n", code(&criteria)));
    if let Some(description) = &test.description {
        s.push_str(&format!("- description: {description}\n"));
    }
    s.push_str("\nRun it from where the stage runs it:\n\n```bash\n");
    s.push_str(&format!("cd {}\n", input.site.path.display()));
    s.push_str(&test.command);
    s.push_str("\necho \"exit: $?\"\n```\n\n");
    s.push_str(&format!(
        "That directory is the stage's worktree root joined with its `working_dir`\n\
         (`{}`). The test passes only when the run meets every `success_criteria` key:\n\
         the exit code (`exit_code`, 0 when unset), each `stdout_contains` and\n\
         `stderr_contains` string present, each `stdout_not_contains` string absent,\n\
         and an empty stderr when `stderr_empty` is true.\n\n",
        input.site.working_dir
    ));
    s
}

/// Step 1 of recording the verdict: the criterion schema with `field` pinned
/// to the disputed list.
fn verdict_schema(entry: &Entry) -> String {
    let field = entry.field.as_str();
    let mut s = schema_json(&format!("\"{field}\""));
    s.push_str(&format!(
        "`field` must be `\"{field}\"`: a verdict on this dispute may amend only\n\
         {} `{}` list. `index` is a 0-based index into that list, and `value` is\n\
         YAML text deserialized into a {}.\n\n",
        entry.whose(),
        entry.list,
        entry.value_type
    ));
    s.push_str(PATCHES_RULE);
    s
}

/// The dispute, the whole list with the disputed entry marked, what the agent
/// produced, and the plan and tree.
fn build_evidence(input: &KindPromptInput<'_>, plan_path: &Path, entry: &Entry) -> String {
    let mut disputed = format!(
        "Field: {}\nIndices: {}\n",
        entry.field.as_str(),
        index_list(&entry.indices)
    );
    if let Some(owner) = &entry.owner {
        disputed.push_str(&format!("Owner stage: {owner}\n"));
    }
    for &index in &entry.indices {
        if let Some(line) = entry.lines.get(index) {
            disputed.push_str(&format!("Disputed {} {index}: {line}\n", entry.noun));
        }
    }
    let heading = match &entry.owner {
        Some(owner) => format!("Stage '{owner}' {}s (all)", entry.noun),
        None => format!("Stage {}s (all)", entry.noun),
    };
    let mut u = String::new();
    push_dispute_header(&mut u, input, &disputed);
    push_entry_list(&mut u, &heading, &entry.indices, &entry.lines);
    push_failure_context(&mut u, input.request, input.work_dir);
    let excerpt_stage = entry.owner.as_deref().unwrap_or(&input.stage.id);
    push_plan_and_listing(&mut u, input, plan_path, excerpt_stage, "Plan stage source");
    u
}

/// A wiring check as one line of the evidence's list.
fn check_line(check: &WiringCheck) -> String {
    let literal = if check.literal { " (literal)" } else { "" };
    format!(
        "{}{literal} in {} — {}",
        code(&check.pattern),
        code(&check.source),
        check.description
    )
}

/// A wiring test as one line of the evidence's list.
fn test_line(test: &WiringTest) -> String {
    format!("{}: {}", test.name, code(&test.command))
}

/// `text` as inline code, each backtick read as `'`.
fn code(text: &str) -> String {
    format!("`{}`", text.replace('`', "'"))
}
