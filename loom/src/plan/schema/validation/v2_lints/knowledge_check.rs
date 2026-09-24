//! A knowledge stage's `loom knowledge check --strict` criterion that cannot
//! pass: without `--baseline` it fails on every structural issue, including
//! the ones the tree already has before the stage starts.

use std::path::Path;

use crate::fs::knowledge::catalog;
use crate::fs::knowledge::KnowledgeDir;
use crate::plan::schema::{detect_stage_type, StageDefinition, StageType};

use super::super::shell_lex::Word;
use super::{runs_loom, visit_argvs, LintContext, LintFinding};

pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) {
    let Some(repo_root) = ctx.repo_root else {
        return;
    };
    // Built at most once, and only when some criterion needs it.
    let mut issues: Option<usize> = None;
    for stage in ctx
        .metadata
        .loom
        .stages
        .iter()
        .filter(|s| is_knowledge_stage(s))
    {
        for (idx, criterion) in stage.acceptance.iter().enumerate() {
            let command = criterion.command();
            if !runs_strict_check_without_baseline(command) {
                continue;
            }
            let count = *issues.get_or_insert_with(|| structural_issue_count(repo_root));
            if count > 0 {
                let finding = LintFinding::in_stage(stage, message(idx, command, count), true);
                out.push(finding);
            }
        }
    }
}

fn is_knowledge_stage(stage: &StageDefinition) -> bool {
    matches!(
        detect_stage_type(stage),
        StageType::Knowledge | StageType::KnowledgeDistill
    )
}

fn message(idx: usize, command: &str, count: usize) -> String {
    format!(
        "Acceptance criterion #{} `{command}` fails on every structural issue in the knowledge \
         tree, and the tree has {count} now, before the stage starts. Fix them, or record \
         them with `loom knowledge check --write-baseline <file>` and pass `--baseline <file>`",
        idx + 1
    )
}

fn runs_strict_check_without_baseline(command: &str) -> bool {
    let mut found = false;
    visit_argvs(command, 0, &mut |argv| {
        found |= is_strict_check_without_baseline(argv)
    });
    found
}

fn is_strict_check_without_baseline(argv: &[&Word]) -> bool {
    let words: Vec<&str> = argv.iter().map(|word| word.value.as_str()).collect();
    if !runs_loom(argv) || words.get(1..3) != Some(&["knowledge", "check"][..]) {
        return false;
    }
    let options = &words[3..];
    let strict = options
        .iter()
        .any(|word| matches!(*word, "--strict" | "--strict-evidence"));
    let baseline = options.iter().any(|word| {
        let name = word.split_once('=').map_or(*word, |(name, _)| name);
        matches!(name, "--baseline" | "--write-baseline")
    });
    strict && !baseline
}

/// Structural (non-review) issues in the repository's knowledge tree, through
/// the catalog checks `loom knowledge check` runs, minus its git-backed
/// evidence pass (`catalog::structural_issues` runs no process). Zero when
/// there is no tree or it cannot be read: nothing to report then.
fn structural_issue_count(repo_root: &Path) -> usize {
    let knowledge = KnowledgeDir::new(repo_root);
    let root = knowledge.root();
    if !root.exists() {
        return 0;
    }
    catalog::structural_issues(root).map_or(0, |issues| issues.len())
}
