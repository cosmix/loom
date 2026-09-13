use std::collections::BTreeMap;

use serde::Serialize;

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct FreshStartSummary {
    pub(crate) first_observed_rows: usize,
    pub(crate) true_fresh_starts: usize,
    pub(crate) known_not_fresh_starts: usize,
    pub(crate) unknown: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct StageAttributionSummary {
    pub(crate) known: usize,
    pub(crate) unknown: usize,
    pub(crate) not_applicable: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct ToolCounts {
    pub(crate) total: usize,
    pub(crate) by_name: BTreeMap<String, usize>,
    pub(crate) unavailable_rows: usize,
}

#[derive(Debug, Clone, Default, Serialize)]
pub(crate) struct TurnoverRatios {
    pub(crate) cache_read_to_fresh_input: Option<f64>,
    pub(crate) cache_creation_to_fresh_input: Option<f64>,
}
