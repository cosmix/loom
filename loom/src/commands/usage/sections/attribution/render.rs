//! Terminal rendering of the attribution section.

use crate::context::untrusted::inline_safe;

use super::super::fmt::{format_f64, format_u64, heading, row_line, share};
use super::{Attribution, AttributionRow, StartupRow};

pub fn render(attribution: &Attribution) {
    heading("Attribution");
    for line in lines(attribution) {
        println!("{line}");
    }
}

/// The section's body, below its heading.
fn lines(attribution: &Attribution) -> Vec<String> {
    if attribution.total_s2 == 0.0
        && attribution.rows.is_empty()
        && attribution.startup_excluded == 0
    {
        return vec![row_line("attribution", "no data")];
    }
    let total = attribution.total_s2;
    let part = |value: f64| {
        format!(
            "{} ({:.1}% of total)",
            format_f64(value),
            share(value, total)
        )
    };
    let mut lines = vec![
        "  estimate: S2 by what entered the context, at 4 characters per token".to_owned(),
        row_line("total S2", format_f64(total)),
        row_line("startup S2", part(attribution.startup_s2)),
        row_line("output S2", part(attribution.output_s2)),
        row_line(
            "assistant replay S2 (upper bound, thinking may not be replayed)",
            part(attribution.assistant_replay_s2),
        ),
        row_line("residual S2", part(attribution.residual_s2)),
        row_line(
            "over-attributed S2",
            format_f64(attribution.over_attributed_s2),
        ),
        row_line(
            "transcripts without a provable start",
            format_u64(attribution.startup_excluded),
        ),
    ];
    push_classes(&mut lines, &attribution.rows, total);
    push_startup(&mut lines, &attribution.startup_by_agent);
    lines
}

fn push_classes(lines: &mut Vec<String>, rows: &[AttributionRow], total: f64) {
    if rows.is_empty() {
        lines.push(row_line("attributed classes", "no data"));
    } else {
        lines.push("  attributed classes (estimate):".to_owned());
    }
    for item in rows {
        lines.push(format!(
            "    {}: {} items, {} chars, S2 {} ({:.1}%)",
            inline_safe(&item.class),
            format_u64(item.items),
            format_u64(item.chars),
            format_f64(item.s2),
            share(item.s2, total)
        ));
    }
}

fn push_startup(lines: &mut Vec<String>, rows: &[StartupRow]) {
    if rows.is_empty() {
        return;
    }
    lines.push("  startup by agent type (medians):".to_owned());
    for item in rows {
        lines.push(format!(
            "    {}: {} transcripts, resident {} tokens",
            inline_safe(&item.agent_type),
            format_u64(item.transcripts),
            format_u64(item.median_resident)
        ));
        for kind in &item.kinds {
            lines.push(format!(
                "      {}: {} tokens",
                inline_safe(&kind.kind),
                format_u64(kind.median_tokens)
            ));
        }
    }
}

#[cfg(test)]
#[path = "render_tests.rs"]
mod tests;
