//! Quantifies context prefix work that was not served from cache, which is the
//! clearest signal for changing task boundaries, cache timing, or tool loops.
//!
//! `rewritten_vs_cache_creation` is an estimate divided by an actual, not a
//! true share: the numerator (`resident - cache_read`, counted only for pairs
//! above the 10,000-token threshold) approximates how much prefix had to be
//! rebuilt, while the denominator is the transcript's actual cache-creation
//! total. The ratio can exceed 100%.

use std::collections::BTreeMap;

use crate::commands::usage::transcript::{Entry, Scope, Transcript};
use crate::commands::usage::transcript_types::SYNTHETIC_MODEL;
use crate::context::untrusted::inline_safe;

use super::fmt::{format_u64, heading, no_data};

// See `SYNTHETIC_MODEL`'s doc in `usage::transcript_types` for what this
// sentinel means and why it is filtered per-site rather than once in
// `Transcript::requests()`. This module's own reason to keep the filter
// local: `collect_transcript`'s pairing loop below does not even go through
// `requests()` -- it walks `entries` directly to keep original indices -- so
// a source-level filter would miss this exact site regardless. Left
// unfiltered, pairing a synthetic row (all-zero `usage`) as the `current`
// side of a rewrite window turns `tokens = previous.usage.resident() -
// current.usage.cache_read` into the whole prior residency (`cache_read`
// reads as 0), which reliably clears the 10,000-token gate and reports a
// giant phantom rewrite attributed to the `<synthetic>` model.

#[derive(Debug, serde::Serialize)]
pub struct CacheRewrites {
    pub total_tokens: u64,
    pub rewrites: usize,
    pub rewritten_vs_cache_creation: f64,
    pub by_scope: Vec<RewriteBreakdown>,
    pub by_gap: Vec<RewriteBreakdown>,
    pub by_preceded_by: Vec<RewriteBreakdown>,
    pub by_model: Vec<RewriteBreakdown>,
}

#[derive(Debug, serde::Serialize)]
pub struct RewriteBreakdown {
    pub label: String,
    pub rewrites: usize,
    pub tokens: u64,
}

pub fn build(transcripts: &[Transcript]) -> CacheRewrites {
    let mut collector = Collector::default();
    for transcript in transcripts {
        collect_transcript(&mut collector, transcript);
    }
    let cache_creation = transcripts
        .iter()
        .map(|item| item.total_usage().cache_creation)
        .sum();
    CacheRewrites {
        total_tokens: collector.total,
        rewrites: collector.rewrites,
        rewritten_vs_cache_creation: share(collector.total, cache_creation),
        by_scope: collector.rows(Kind::Scope),
        by_gap: collector.rows(Kind::Gap),
        by_preceded_by: collector.rows(Kind::Preceded),
        by_model: collector.rows(Kind::Model),
    }
}

pub fn render(rewrites: &CacheRewrites) {
    heading("Cache rewrites");
    if rewrites.rewrites == 0 {
        no_data("rewrites over 10,000 tokens");
        return;
    }
    println!(
        "  total re-written prefix: {} tokens across {} requests",
        format_u64(rewrites.total_tokens),
        rewrites.rewrites
    );
    println!(
        "  re-written prefix vs. cache-creation tokens: {:.1}% (estimate over an actual; can exceed 100%)",
        rewrites.rewritten_vs_cache_creation
    );
    render_rows("scope", &rewrites.by_scope);
    render_rows("gap since prior request", &rewrites.by_gap);
    render_rows("what preceded it", &rewrites.by_preceded_by);
    render_rows("model", &rewrites.by_model);
}

#[derive(Default)]
struct Collector {
    total: u64,
    rewrites: usize,
    scope: BTreeMap<String, (usize, u64)>,
    gap: BTreeMap<String, (usize, u64)>,
    preceded: BTreeMap<String, (usize, u64)>,
    model: BTreeMap<String, (usize, u64)>,
}

#[derive(Copy, Clone)]
enum Kind {
    Scope,
    Gap,
    Preceded,
    Model,
}

fn collect_transcript(collector: &mut Collector, transcript: &Transcript) {
    let requests = transcript
        .entries
        .iter()
        .enumerate()
        .filter_map(|(index, entry)| match entry {
            Entry::Assistant(request) => Some((index, request)),
            Entry::User(_) => None,
        })
        .collect::<Vec<_>>();
    for pair in requests.windows(2) {
        let (previous_index, previous) = pair[0];
        let (current_index, current) = pair[1];
        if current.model == SYNTHETIC_MODEL {
            continue;
        }
        let tokens = previous
            .usage
            .resident()
            .saturating_sub(current.usage.cache_read);
        if tokens > 10_000 {
            collect_pair(
                collector,
                transcript,
                previous_index,
                current_index,
                current,
                tokens,
            );
        }
    }
}

fn collect_pair(
    collector: &mut Collector,
    transcript: &Transcript,
    previous: usize,
    current: usize,
    request: &crate::commands::usage::transcript::Request,
    tokens: u64,
) {
    collector.total += tokens;
    collector.rewrites += 1;
    add_row(&mut collector.scope, scope_label(transcript.scope), tokens);
    add_row(
        &mut collector.gap,
        gap_label(
            request
                .timestamp
                .signed_duration_since(entry_time(&transcript.entries[previous])),
        ),
        tokens,
    );
    add_row(
        &mut collector.preceded,
        preceding_label(&transcript.entries, previous, current),
        tokens,
    );
    add_row(&mut collector.model, request.model.clone(), tokens);
}

fn entry_time(entry: &Entry) -> chrono::DateTime<chrono::Utc> {
    entry.timestamp()
}

fn preceding_label(entries: &[Entry], previous: usize, current: usize) -> String {
    let has_result = entries[previous + 1..current]
        .iter()
        .any(|entry| matches!(entry, Entry::User(user) if user.tool_use_id.is_some()));
    if has_result {
        "tool result".to_owned()
    } else {
        "finished assistant turn".to_owned()
    }
}

fn gap_label(gap: chrono::Duration) -> String {
    let seconds = gap.num_seconds();
    let label = if seconds < 60 {
        "<1m"
    } else if seconds < 300 {
        "1-5m"
    } else if seconds < 1_800 {
        "5-30m"
    } else {
        "30m+"
    };
    label.to_owned()
}

fn add_row(rows: &mut BTreeMap<String, (usize, u64)>, label: String, tokens: u64) {
    let row = rows.entry(label).or_default();
    row.0 += 1;
    row.1 += tokens;
}

impl Collector {
    fn rows(&self, kind: Kind) -> Vec<RewriteBreakdown> {
        let source = match kind {
            Kind::Scope => &self.scope,
            Kind::Gap => &self.gap,
            Kind::Preceded => &self.preceded,
            Kind::Model => &self.model,
        };
        let mut rows = source
            .iter()
            .map(|(label, (rewrites, tokens))| RewriteBreakdown {
                label: label.clone(),
                rewrites: *rewrites,
                tokens: *tokens,
            })
            .collect::<Vec<_>>();
        rows.sort_by(|left, right| {
            right
                .tokens
                .cmp(&left.tokens)
                .then_with(|| left.label.cmp(&right.label))
        });
        rows
    }
}

fn render_rows(label: &str, rows: &[RewriteBreakdown]) {
    println!("  by {label}:");
    for row in rows {
        println!(
            "    {:<24} {:>7} rewrites  {:>12} tokens",
            inline_safe(&row.label),
            row.rewrites,
            format_u64(row.tokens)
        );
    }
}

fn scope_label(scope: Scope) -> String {
    let label = match scope {
        Scope::Main => "main",
        Scope::Subagent => "subagent",
    };
    label.to_owned()
}

fn share(value: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        value as f64 * 100.0 / total as f64
    }
}

#[cfg(test)]
#[path = "rewrites_tests.rs"]
mod tests;
