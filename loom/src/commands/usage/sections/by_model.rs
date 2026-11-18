//! Compares model and execution scope together, which makes it possible to
//! decide whether a model change belongs in the coordinator or its workers.

use std::collections::BTreeMap;

use crate::commands::usage::accounting::UsageSummary;
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
    #[serde(flatten)]
    pub(crate) summary: UsageSummary,
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
                summary: UsageSummary::default(),
            });
            row.requests += 1;
            row.summary.add(&request.usage);
        }
    }
    let mut rows = grouped.into_values().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right
            .summary
            .accounting
            .s2
            .total_cmp(&left.summary.accounting.s2)
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
            format_u64(item.summary.usage.input),
            format_u64(item.summary.usage.cache_read),
            format_u64(item.summary.usage.output)
        );
        println!(
            "    S1: {}  S2: {}  S3: {}",
            format_u64(item.summary.accounting.s1),
            format_f64(item.summary.accounting.s2),
            format_u64(item.summary.accounting.s3)
        );
    }
}

fn scope_label(scope: Scope) -> &'static str {
    match scope {
        Scope::Main => "main",
        Scope::Subagent => "subagent",
    }
}

#[cfg(test)]
#[path = "by_model_tests.rs"]
mod tests;
