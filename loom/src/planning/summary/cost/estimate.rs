//! Rebuild estimates from recorded usage and local-time usage totals.

use chrono::{DateTime, Duration, Local};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;

use crate::planning::project::Project;
use crate::planning::summary::evaluate::Evaluation;
use crate::planning::summary::store::is_overview;

use super::{CostLine, Level, TokenVector};

/// Starting estimate for a document without usable history.
pub const DOCUMENT_STARTING_TOKENS: u64 = 40_000;
/// Starting estimate for an overview without usable history.
pub const OVERVIEW_STARTING_TOKENS: u64 = 150_000;

/// Source bytes by summary id; each overview sums only its own feature's members.
pub fn source_sizes(project: &Project, evaluation: &Evaluation) -> BTreeMap<String, u64> {
    let mut sizes: BTreeMap<String, u64> = evaluation
        .summaries
        .values()
        .filter(|summary| !is_overview(&summary.id))
        .map(|summary| {
            let bytes = summary
                .source
                .as_deref()
                .and_then(|path| project.tree.get(path))
                .map_or(0, |doc| doc.size);
            (summary.id.clone(), bytes)
        })
        .collect();
    for scope in evaluation.scopes.values() {
        for (kind, members) in [
            ("product", &scope.product),
            ("engineering", &scope.engineering),
        ] {
            let id = format!("overview/{}/{kind}", scope.feature);
            if evaluation.summaries.contains_key(&id) {
                let bytes = members
                    .iter()
                    .filter_map(|id| sizes.get(id))
                    .fold(0_u64, |total, bytes| total.saturating_add(*bytes));
                sizes.insert(id, bytes);
            }
        }
    }
    sizes
}

/// Evidence supporting an estimate, ordered from strongest to weakest.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EstimateBasis {
    /// The summary's most recently recorded individual cost.
    Last,
    /// The median fresh-token rate for summaries at the same level.
    PerByte,
    /// The level's fixed starting figure.
    Starting,
}

/// Estimated fresh tokens for a pending rebuild batch.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Estimate {
    /// Sum of the individual estimates, saturating on overflow.
    pub tokens: u64,
    /// Individual estimates in pending order.
    pub per_summary: Vec<(String, u64)>,
    /// Weakest evidence used by any individual estimate.
    pub basis: EstimateBasis,
}

/// Estimate each pending summary from its last cost, its level's rate, or a default.
///
/// Only individual records with all fresh-token components known are usable.
/// Cache reads are excluded. An empty batch has the neutral basis `Last`.
pub fn estimate(log: &[CostLine], pending: &[(String, Level, u64)]) -> Estimate {
    let mut result = Estimate {
        tokens: 0,
        per_summary: Vec::with_capacity(pending.len()),
        basis: EstimateBasis::Last,
    };
    for (id, level, bytes) in pending {
        let (tokens, basis) = estimate_one(log, id, level, *bytes);
        result.tokens = result.tokens.saturating_add(tokens);
        result.per_summary.push((id.clone(), tokens));
        result.basis = result.basis.max(basis);
    }
    result
}

/// Tokens a rebuild spent beyond cache reads, as the estimates count them; `None` when a part
/// is unknown.
pub fn fresh_tokens(tokens: &TokenVector) -> Option<u64> {
    Some(
        tokens
            .fresh_input?
            .saturating_add(tokens.cache_write?)
            .saturating_add(tokens.output?),
    )
}

fn estimate_one(log: &[CostLine], id: &str, level: &Level, bytes: u64) -> (u64, EstimateBasis) {
    let last = log.iter().rev().find_map(|line| {
        if line.level == *level && line.summaries.len() == 1 && line.summaries[0] == id {
            fresh_tokens(&line.tokens)
        } else {
            None
        }
    });
    if let Some(tokens) = last {
        return (tokens, EstimateBasis::Last);
    }
    if let Some(rate) = median_rate(log, level) {
        // Round up so fractional tokens do not understate the estimate. Float-to-int
        // conversion saturates if a large source would exceed the token counter.
        return ((rate * bytes as f64).ceil() as u64, EstimateBasis::PerByte);
    }
    let tokens = match level {
        Level::Document => DOCUMENT_STARTING_TOKENS,
        Level::Overview => OVERVIEW_STARTING_TOKENS,
    };
    (tokens, EstimateBasis::Starting)
}

fn median_rate(log: &[CostLine], level: &Level) -> Option<f64> {
    let mut rates: Vec<_> = log
        .iter()
        .filter(|line| line.level == *level && line.summaries.len() == 1)
        .filter_map(|line| {
            let bytes = line.source_bytes.filter(|bytes| *bytes > 0)?;
            Some(fresh_tokens(&line.tokens)? as f64 / bytes as f64)
        })
        .collect();
    if rates.is_empty() {
        return None;
    }
    rates.sort_unstable_by(f64::total_cmp);
    let middle = rates.len() / 2;
    Some(if rates.len().is_multiple_of(2) {
        (rates[middle - 1] + rates[middle]) / 2.0
    } else {
        rates[middle]
    })
}

/// Recorded usage for the current local day and the trailing seven days.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Totals {
    /// Known usage components since local midnight.
    pub today: TokenVector,
    /// Known usage components in the last seven times 24 hours, including now.
    pub week: TokenVector,
}

/// Sum known components, skipping invalid timestamps and records after `now`.
///
/// A component remains absent if no record in its window has a known value.
pub fn totals(log: &[CostLine], now: DateTime<Local>) -> Totals {
    let mut result = Totals {
        today: TokenVector::default(),
        week: TokenVector::default(),
    };
    let week_start = now - Duration::days(7);
    for line in log {
        let Ok(time) = DateTime::parse_from_rfc3339(&line.time) else {
            continue;
        };
        if time > now {
            continue;
        }
        if time.with_timezone(&Local).date_naive() == now.date_naive() {
            add_tokens(&mut result.today, &line.tokens);
        }
        if time >= week_start {
            add_tokens(&mut result.week, &line.tokens);
        }
    }
    result
}

fn add_tokens(total: &mut TokenVector, tokens: &TokenVector) {
    fn add(total: &mut Option<u64>, value: Option<u64>) {
        if let Some(value) = value {
            *total = Some(total.unwrap_or(0).saturating_add(value));
        }
    }
    add(&mut total.fresh_input, tokens.fresh_input);
    add(&mut total.cache_write, tokens.cache_write);
    add(&mut total.cache_read, tokens.cache_read);
    add(&mut total.output, tokens.output);
}
