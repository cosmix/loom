//! Terminal rendering of the attribution section.

use crate::context::untrusted::inline_safe;

use super::super::fmt::{format_f64, format_u64, heading, no_data, row, share};
use super::{Attribution, AttributionRow, StartupRow};

pub fn render(attribution: &Attribution) {
    heading("Attribution");
    if attribution.total_s2 == 0.0
        && attribution.rows.is_empty()
        && attribution.startup_excluded == 0
    {
        no_data("attribution");
        return;
    }
    let total = attribution.total_s2;
    let part = |value: f64| {
        format!(
            "{} ({:.1}% of total)",
            format_f64(value),
            share(value, total)
        )
    };
    println!("  estimate: S2 by what entered the context, at 4 characters per token");
    row("total S2", format_f64(total));
    row("startup S2", part(attribution.startup_s2));
    row("output S2", part(attribution.output_s2));
    row(
        "assistant replay S2 (upper bound, thinking may not be replayed)",
        part(attribution.assistant_replay_s2),
    );
    row("residual S2", part(attribution.residual_s2));
    row(
        "over-attributed S2",
        format_f64(attribution.over_attributed_s2),
    );
    row(
        "transcripts without a provable start",
        format_u64(attribution.startup_excluded),
    );
    render_classes(&attribution.rows, total);
    render_startup(&attribution.startup_by_agent);
}

fn render_classes(rows: &[AttributionRow], total: f64) {
    if rows.is_empty() {
        row("attributed classes", "no data");
    } else {
        println!("  attributed classes (estimate):");
    }
    for item in rows {
        println!(
            "    {}: {} items, {} chars, S2 {} ({:.1}%)",
            inline_safe(&item.class),
            format_u64(item.items),
            format_u64(item.chars),
            format_f64(item.s2),
            share(item.s2, total)
        );
    }
}

fn render_startup(rows: &[StartupRow]) {
    if rows.is_empty() {
        return;
    }
    println!("  startup by agent type (medians):");
    for item in rows {
        println!(
            "    {}: {} transcripts, resident {} tokens",
            inline_safe(&item.agent_type),
            format_u64(item.transcripts),
            format_u64(item.median_resident)
        );
        for kind in &item.kinds {
            println!(
                "      {}: {} tokens",
                inline_safe(&kind.kind),
                format_u64(kind.median_tokens)
            );
        }
    }
}
