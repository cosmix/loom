//! Estimates where a session's S2 cost went, by what entered the context.
//!
//! A tool result, user message or attachment admitted after `k` requests is
//! written to the cache by request `k + 1` and read by the `n - k - 1`
//! requests after that, because one request's resident context is the next
//! request's cache read. Anything admitted before the first request is
//! startup: already inside that request's resident context, so it is reported
//! apart from the rows. What the definitions leave unexplained is the residual;
//! an excess is reported as over-attribution, never hidden in another
//! transcript's residual.

mod analyze;
mod classify;
mod render;
mod startup;

use std::collections::BTreeMap;

use crate::commands::usage::transcript::Transcript;

pub use render::render;
pub use startup::StartupRow;

/// Rows kept before the tail merges into one `other` row.
const MAX_ROWS: usize = 24;

#[derive(Debug, serde::Serialize)]
pub struct Attribution {
    pub total_s2: f64,
    pub startup_s2: f64,
    pub output_s2: f64,
    /// Upper bound: thinking blocks may not be replayed in later requests.
    pub assistant_replay_s2: f64,
    pub residual_s2: f64,
    pub over_attributed_s2: f64,
    /// Transcripts with requests whose start is not provably in range.
    pub startup_excluded: u64,
    pub rows: Vec<AttributionRow>,
    pub startup_by_agent: Vec<StartupRow>,
}

#[derive(Debug, serde::Serialize)]
pub struct AttributionRow {
    pub class: String,
    pub items: u64,
    pub chars: u64,
    pub s2: f64,
}

pub fn build(transcripts: &[Transcript]) -> Attribution {
    let mut totals = Totals::default();
    let mut rows = BTreeMap::<String, AttributionRow>::new();
    let mut startups = Vec::new();
    for transcript in transcripts {
        let Some(analysis) = analyze::analyze(transcript) else {
            continue;
        };
        totals.add(&analysis);
        for item in &analysis.items {
            let row = rows
                .entry(item.class.clone())
                .or_insert_with(|| AttributionRow {
                    class: item.class.clone(),
                    items: 0,
                    chars: 0,
                    s2: 0.0,
                });
            row.items += 1;
            row.chars += item.chars as u64;
            row.s2 += item.s2;
        }
        match analysis.startup {
            Some(startup) => startups.push((agent_key(transcript), startup)),
            None => totals.startup_excluded += 1,
        }
    }
    Attribution {
        total_s2: totals.total,
        startup_s2: totals.startup,
        output_s2: totals.output,
        assistant_replay_s2: totals.replay,
        residual_s2: totals.residual,
        over_attributed_s2: totals.over_attributed,
        startup_excluded: totals.startup_excluded,
        rows: bounded(rows.into_values().collect()),
        startup_by_agent: startup::by_agent(startups),
    }
}

#[derive(Default)]
struct Totals {
    total: f64,
    startup: f64,
    output: f64,
    replay: f64,
    residual: f64,
    over_attributed: f64,
    startup_excluded: u64,
}

impl Totals {
    /// Residual and over-attribution are settled per transcript, so one
    /// transcript's excess cannot offset another's unexplained cost.
    fn add(&mut self, analysis: &analyze::Analysis) {
        let attributed: f64 = analysis.items.iter().map(|item| item.s2).sum();
        let unexplained = analysis.total_s2
            - analysis.startup_s2
            - analysis.output_s2
            - analysis.replay_s2
            - attributed;
        self.total += analysis.total_s2;
        self.startup += analysis.startup_s2;
        self.output += analysis.output_s2;
        self.replay += analysis.replay_s2;
        self.residual += unexplained.max(0.0);
        self.over_attributed += (-unexplained).max(0.0);
    }
}

fn agent_key(transcript: &Transcript) -> String {
    use crate::commands::usage::transcript_types::Scope;
    match (&transcript.agent_type, transcript.scope) {
        (Some(agent_type), _) => agent_type.clone(),
        (None, Scope::Main) => "main".to_owned(),
        (None, Scope::Subagent) => "unknown".to_owned(),
    }
}

/// Largest rows first; past `MAX_ROWS` the tail merges into one `other` row.
fn bounded(mut rows: Vec<AttributionRow>) -> Vec<AttributionRow> {
    rows.sort_by(|left, right| {
        right
            .s2
            .total_cmp(&left.s2)
            .then_with(|| left.class.cmp(&right.class))
    });
    if rows.len() <= MAX_ROWS {
        return rows;
    }
    let mut other = AttributionRow {
        class: "other".to_owned(),
        items: 0,
        chars: 0,
        s2: 0.0,
    };
    for row in rows.split_off(MAX_ROWS - 1) {
        other.items += row.items;
        other.chars += row.chars;
        other.s2 += row.s2;
    }
    rows.push(other);
    rows
}

#[cfg(test)]
mod more_tests;
#[cfg(test)]
mod tests;
