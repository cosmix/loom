//! `loom plan check`: the planning checklist and the problems of the doc/ tree.

use std::fmt::Write as _;
use std::path::{Path, PathBuf};

use anyhow::Result;
use serde::Serialize;

use crate::context::untrusted::inline_safe;
use crate::planning::checklist::{Checklist, CurrentStep, Offer, Step};
use crate::planning::problems::Problem;
use crate::planning::project::Project;
use crate::planning::slug::PlanState;

#[cfg(test)]
mod tests;

#[derive(Serialize)]
pub(crate) struct CheckReport<'a> {
    schema: u8,
    root: &'a Path,
    features: Vec<FeatureReport<'a>>,
    standalone: Vec<StandaloneReport<'a>>,
    moves: &'a [Offer],
    problems: &'a [Problem],
}

#[derive(Serialize)]
struct FeatureReport<'a> {
    name: &'a str,
    checklist: &'a Checklist,
}

#[derive(Serialize)]
struct StandaloneReport<'a> {
    slug: &'a str,
    path: &'a Path,
    state: PlanState,
}

fn standalone(project: &Project) -> impl Iterator<Item = StandaloneReport<'_>> {
    project
        .standalone
        .iter()
        .filter_map(|slug| project.tree.plan(slug))
        .map(|plan| StandaloneReport {
            slug: &plan.slug,
            path: &plan.path,
            state: plan.state,
        })
}

pub(crate) fn report(project: &Project) -> CheckReport<'_> {
    CheckReport {
        schema: 2,
        root: &project.root,
        features: project
            .features
            .iter()
            .map(|feature| FeatureReport {
                name: &feature.name,
                checklist: &feature.checklist,
            })
            .collect(),
        standalone: standalone(project).collect(),
        moves: &project.moves,
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

fn human_checklist(out: &mut String, checklist: &Checklist) {
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
}

fn human(project: &Project) -> String {
    let mut out = String::new();
    for feature in &project.features {
        let _ = writeln!(out, "{}", inline_safe(&feature.name));
        human_checklist(&mut out, &feature.checklist);
    }
    let _ = writeln!(out, "standalone plans");
    for plan in standalone(project) {
        let _ = writeln!(
            out,
            "  {}: {}",
            inline_safe(plan.slug),
            inline_safe(&format!("{:?}", plan.state))
        );
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
