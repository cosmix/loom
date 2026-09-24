//! `loom plan verify` lints (DESIGN D4): commands a stage runs that cannot mean
//! what they say. A finding reports as an error on a v2 plan when it sets
//! `error_in_v2`, and as a structural warning otherwise (so `--strict` fails
//! on it either way).
//!
//! Every lint reads the plan and the repository and writes nothing: no cache,
//! no index, no build. Commands are lexed with `shell_lex`, and the scripts
//! nested in command substitutions and `sh -c` are scanned the same way
//! `criterion_hazards` scans them.

use std::path::Path;

use crate::plan::schema::{LoomMetadata, StageDefinition};

use super::criterion_hazards::{command_start, nested_scripts, MAX_NESTING};
use super::shell_lex::{lex, simple_commands, Word};

mod knowledge_check;
mod loom_subcommands;
mod regex_patterns;
mod rust_filters;
mod sandbox_capability;

/// What the lints read: the parsed plan, and the repository it runs against
/// when one could be resolved.
pub(crate) struct LintContext<'a> {
    pub metadata: &'a LoomMetadata,
    pub repo_root: Option<&'a Path>,
}

/// One lint result. `stage_id` is `None` for a plan-wide finding.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct LintFinding {
    pub stage_id: Option<String>,
    pub message: String,
    /// `false` keeps the finding a warning in both plan versions.
    pub error_in_v2: bool,
}

impl LintFinding {
    fn in_stage(stage: &StageDefinition, message: String, error_in_v2: bool) -> Self {
        Self {
            stage_id: Some(stage.id.clone()),
            message,
            error_in_v2,
        }
    }
}

/// Run every lint over the plan (DESIGN D4's entry point; a new lint registers
/// here).
///
/// `notes` carries D4's "no base layer" note: when the Rust-filter lint finds
/// filters but no source-graph base layer to check them against, it skips
/// them and says so there (`plan verify`'s notes, which never count as
/// warnings), since a `LintFinding` cannot express a skipped check.
pub(crate) fn run(ctx: &LintContext<'_>, notes: &mut Vec<String>) -> Vec<LintFinding> {
    let mut out = Vec::new();
    loom_subcommands::check(ctx, &mut out);
    regex_patterns::check(ctx, &mut out);
    sandbox_capability::check(ctx, &mut out);
    knowledge_check::check(ctx, &mut out);
    notes.extend(rust_filters::check(ctx, &mut out));
    out
}

/// One command a stage runs, with the label its findings name it by.
struct StageCommand<'a> {
    label: String,
    text: &'a str,
}

impl<'a> StageCommand<'a> {
    fn new(label: String, text: &'a str) -> Self {
        Self { label, text }
    }

    /// `<label> `<command>` <problem>`, the shape `criterion_hazards` uses.
    fn describe(&self, problem: &str) -> String {
        format!("{} `{}` {problem}", self.label, self.text)
    }
}

/// Every command D4 scans: acceptance, setup, wiring tests, before/after-stage
/// checks and the dead-code check.
fn stage_commands(stage: &StageDefinition) -> Vec<StageCommand<'_>> {
    let numbered = |kind: &str, idx: usize| format!("{kind} #{}", idx + 1);
    let acceptance = stage.acceptance.iter().enumerate().map(|(idx, criterion)| {
        StageCommand::new(numbered("Acceptance criterion", idx), criterion.command())
    });
    let setup = stage
        .setup
        .iter()
        .enumerate()
        .map(|(idx, command)| StageCommand::new(numbered("Setup command", idx), command));
    let wiring = stage.wiring_tests.iter().enumerate().map(|(idx, test)| {
        let label = format!("{} '{}'", numbered("Wiring test", idx), test.name);
        StageCommand::new(label, &test.command)
    });
    let before =
        stage.before_stage.iter().enumerate().map(|(idx, check)| {
            StageCommand::new(numbered("before_stage check", idx), &check.command)
        });
    let after =
        stage.after_stage.iter().enumerate().map(|(idx, check)| {
            StageCommand::new(numbered("after_stage check", idx), &check.command)
        });
    let dead_code = stage
        .dead_code_check
        .iter()
        .map(|check| StageCommand::new("dead_code_check command".to_string(), &check.command));
    acceptance
        .chain(setup)
        .chain(wiring)
        .chain(before)
        .chain(after)
        .chain(dead_code)
        .collect()
}

/// Call `visit` with each stage command and the argv (past assignments and
/// wrappers) of every simple command it runs.
fn visit_stage_argvs(stage: &StageDefinition, visit: &mut dyn FnMut(&StageCommand<'_>, &[&Word])) {
    for command in stage_commands(stage) {
        visit_argvs(command.text, 0, &mut |argv| visit(&command, argv));
    }
}

fn visit_argvs(script: &str, depth: usize, visit: &mut dyn FnMut(&[&Word])) {
    let tokens = lex(script);
    for simple in simple_commands(&tokens) {
        let argv = &simple.words[command_start(&simple.words)..];
        if !argv.is_empty() {
            visit(argv);
        }
        if depth < MAX_NESTING {
            for nested in nested_scripts(&simple) {
                visit_argvs(&nested, depth + 1, visit);
            }
        }
    }
}

/// Whether `argv` runs `loom` (or a path ending in `/loom`).
fn runs_loom(argv: &[&Word]) -> bool {
    argv.first()
        .is_some_and(|word| word.command_name() == "loom")
}
