//! Compares model and execution scope together, which makes it possible to
//! decide whether a model change belongs in the coordinator or its workers.

use std::collections::BTreeMap;

use crate::commands::usage::accounting::Accounting;
use crate::commands::usage::transcript::{Scope, Transcript};
use crate::commands::usage::transcript_types::SYNTHETIC_MODEL;
use crate::context::untrusted::inline_safe;

use super::fmt::{format_f64, format_u64, heading, no_data};

#[derive(Debug, serde::Serialize)]
pub struct ByModel {
    pub rows: Vec<ModelRow>,
}

#[derive(Debug, serde::Serialize)]
pub struct ModelRow {
    pub model: String,
    pub scope: String,
    pub requests: usize,
    pub fresh_input: u64,
    pub cache_creation: u64,
    pub cache_reads: u64,
    pub output: u64,
    pub cache_writes_5m: u64,
    pub cache_writes_1h: u64,
    pub s1: u64,
    pub s2: f64,
    pub s3: u64,
}

pub fn build(transcripts: &[Transcript]) -> ByModel {
    let mut grouped = BTreeMap::<(String, String), ModelRow>::new();
    for transcript in transcripts {
        let scope = scope_label(transcript.scope).to_owned();
        for request in transcript.requests() {
            if request.model == SYNTHETIC_MODEL {
                continue;
            }
            let key = (request.model.clone(), scope.clone());
            let row = grouped.entry(key).or_insert_with(|| ModelRow {
                model: request.model.clone(),
                scope: scope.clone(),
                requests: 0,
                fresh_input: 0,
                cache_creation: 0,
                cache_reads: 0,
                output: 0,
                cache_writes_5m: 0,
                cache_writes_1h: 0,
                s1: 0,
                s2: 0.0,
                s3: 0,
            });
            row.requests += 1;
            add_usage(row, request);
        }
    }
    let mut rows = grouped.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .s2
            .total_cmp(&left.s2)
            .then_with(|| left.model.cmp(&right.model))
            .then_with(|| left.scope.cmp(&right.scope))
    });
    ByModel { rows }
}

pub fn render(report: &ByModel) {
    heading("By model and scope");
    if report.rows.is_empty() {
        no_data("models");
        return;
    }
    for item in &report.rows {
        println!("  {} ({})", inline_safe(&item.model), item.scope);
        println!(
            "    requests: {}  input: {}  reads: {}  output: {}",
            item.requests,
            format_u64(item.fresh_input),
            format_u64(item.cache_reads),
            format_u64(item.output)
        );
        println!(
            "    S1: {}  S2: {}  S3: {}",
            format_u64(item.s1),
            format_f64(item.s2),
            format_u64(item.s3)
        );
    }
}

fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::Main => "main",
        Scope::Subagent => "subagent",
    }
}

fn add_usage(row: &mut ModelRow, request: &crate::commands::usage::transcript::Request) {
    let usage = &request.usage;
    let accounting = Accounting::of(usage);
    row.fresh_input += usage.input;
    row.cache_creation += usage.cache_creation;
    row.cache_reads += usage.cache_read;
    row.output += usage.output;
    row.cache_writes_5m += usage.ephemeral_5m;
    row.cache_writes_1h += usage.ephemeral_1h;
    row.s1 += accounting.s1;
    row.s2 += accounting.s2;
    row.s3 += accounting.s3;
}

#[cfg(test)]
#[path = "by_model_tests.rs"]
mod tests;
