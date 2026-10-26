//! `loom plan check`: the planning checklist and the problems of the doc/ tree.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::context::untrusted::inline_safe;
use crate::planning::checklist::{Checklist, CurrentStep, Step};
use crate::planning::problems::Problem;
use crate::planning::project::Project;

#[cfg(test)]
mod tests;

#[derive(Serialize)]
pub(crate) struct CheckReport<'a> {
    schema: u8,
    root: &'a Path,
    checklist: &'a Checklist,
    problems: &'a [Problem],
}

pub(crate) fn report(project: &Project) -> CheckReport<'_> {
    CheckReport {
        schema: 1,
        root: &project.root,
        checklist: &project.checklist,
        problems: &project.problems,
    }
}

fn step_name(step: Step) -> &'static str {
    match step {
        Step::Vision => "Vision",
        Step::ProductSpec => "Product spec",
        Step::Metaplan => "Metaplan",
        Step::Plans => "Plans",
        Step::PressureTest => "Pressure test",
        Step::Review => "Review",
        Step::Run => "Run",
    }
}

fn human(project: &Project) -> String {
    let checklist = &project.checklist;
    let mut out = String::new();
    for (step, mark) in &checklist.steps {
        let _ = writeln!(out, "{} {}: {mark:?}", *step as u8, step_name(*step));
    }
    for item in &checklist.items {
        let _ = writeln!(out, "  {:?}: {}", item.status, inline_safe(&item.label));
    }
    let current = match &checklist.current {
        CurrentStep::Step(step) => step_name(*step),
        CurrentStep::Ready => "Ready",
        CurrentStep::ReadyAllDone => "Ready (all done)",
    };
    let _ = writeln!(out, "Current: {current}");
    if let Some(offer) = &checklist.primary {
        let _ = writeln!(out, "Primary: {}", inline_safe(&offer.id()));
    }
    for offer in &checklist.secondary {
        let _ = writeln!(out, "Secondary: {}", inline_safe(&offer.id()));
    }
    for problem in &project.problems {
        let path = inline_safe(&problem.path.to_string_lossy());
        let line = problem.line.map(|n| format!(":{n}")).unwrap_or_default();
        let _ = writeln!(
            out,
            "{path}{line}: {}: {}",
            inline_safe(problem.code),
            inline_safe(&problem.message)
        );
    }
    out
}

/// Print the project checklist and convention problems for `loom plan check`.
pub fn execute(json: bool) -> Result<()> {
    let cwd = match std::env::current_dir() {
        Ok(cwd) => cwd,
        Err(error) => {
            eprintln!("loom plan check: {}", inline_safe(&error.to_string()));
            std::process::exit(2);
        }
    };
    let root = match crate::git::runner::run_git_checked(&["rev-parse", "--show-toplevel"], &cwd) {
        Ok(root) => PathBuf::from(root.trim()),
        Err(_) => {
            eprintln!(
                "loom plan check: {} is not inside a git repository",
                inline_safe(&cwd.to_string_lossy())
            );
            std::process::exit(2);
        }
    };
    let project = match Project::load(&root) {
        Ok(project) => project,
        Err(error) => {
            eprintln!("loom plan check: {}", inline_safe(&format!("{error:#}")));
            std::process::exit(2);
        }
    };
    if json {
        println!("{}", serde_json::to_string_pretty(&report(&project))?);
    } else {
        print!("{}", human(&project));
    }
    if !project.problems.is_empty() {
        std::process::exit(1);
    }
    Ok(())
}
