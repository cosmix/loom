//! Human-readable `loom plan verify` output.

use colored::Colorize;

use super::report::{VerifyError, VerifyReport, VerifyStageLevel, VerifyWarnings};
use crate::commands::graph::colors::stage_color;

pub(super) fn print_human(report: &VerifyReport, strict: bool) {
    let id_str = report.plan.id.as_deref().unwrap_or("unknown");
    let name_str = report.plan.name.as_deref().unwrap_or("(no title)");
    println!("{}", format!("── Plan: {id_str} ──").cyan().bold());
    println!("   {} ({})", name_str.bold(), report.plan.source.dimmed());
    println!();
    print_errors(&report.errors);
    print_warnings(&report.warnings, &report.notes);
    print_levels(&report.levels);
    print_summary(report.errors.len(), report.warnings.total(), strict);
}

fn print_errors(errors: &[VerifyError]) {
    if errors.is_empty() {
        return;
    }
    println!("{}", "Errors".red().bold());
    for e in errors {
        let prefix = match &e.stage_id {
            Some(id) => format!("[{id}] "),
            None => String::new(),
        };
        println!("  {} {}{}", "✗".red().bold(), prefix.red(), e.message);
    }
    println!();
}

/// Warnings sections, then notes on checks that were skipped.
fn print_warnings(warnings: &VerifyWarnings, notes: &[String]) {
    for (title, items) in warnings.sections() {
        if items.is_empty() {
            continue;
        }
        println!("{}", title.yellow().bold());
        for w in items {
            println!("  {} {}", "⚠".yellow(), w);
        }
        println!();
    }
    for note in notes {
        println!("{}", format!("Note: {note}").dimmed());
        println!();
    }
}

fn print_levels(levels: &[Vec<VerifyStageLevel>]) {
    if levels.is_empty() {
        return;
    }
    println!("{}", "Stages by Level".bold());
    for (level_num, stage_list) in levels.iter().enumerate() {
        println!(
            "  {}",
            format!("Level {level_num} ({} stage(s))", stage_list.len()).dimmed()
        );
        for stage in stage_list {
            print_stage(stage);
        }
    }
    println!();
}

fn print_stage(stage: &VerifyStageLevel) {
    let color = stage_color(&stage.id);
    let indicator = if stage.dependencies.is_empty() {
        "●".green().to_string()
    } else {
        "○".yellow().to_string()
    };
    let type_label = stage.stage_type.dimmed().to_string();
    let deps_str = if stage.dependencies.is_empty() {
        String::new()
    } else {
        format!("  ← {}", stage.dependencies.join(", ").dimmed())
    };
    println!(
        "    {}  {}  {}  {}{}",
        indicator,
        stage.id.color(color),
        stage.name.bold(),
        type_label,
        deps_str,
    );
}

fn print_summary(total_errors: usize, total_warnings: usize, strict: bool) {
    let strict_note = if strict && total_warnings > 0 {
        " (strict: failing)"
    } else {
        ""
    };
    let summary = format!("{total_errors} error(s), {total_warnings} warning(s){strict_note}");
    if total_errors > 0 || (strict && total_warnings > 0) {
        println!("{}", summary.red());
    } else if total_warnings > 0 {
        println!("{}", summary.yellow());
    } else {
        println!("{}", summary.green());
    }
}
