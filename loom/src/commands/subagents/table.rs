//! Renders the human-facing subagent table. Fixed-width metadata columns keep
//! repeated `watch` output scannable while the agent-id handle stays whole.

use colored::Colorize;

use super::classify::SubagentSummary;

const LAST_TOOL_WIDTH: usize = 18;
const AGENT_TYPE_WIDTH: usize = 15;
const MODEL_WIDTH: usize = 14;
const REQS_WIDTH: usize = 5;
const PEAK_TOKENS_WIDTH: usize = 10;

pub(super) fn print_table(summaries: &[SubagentSummary]) {
    if summaries.is_empty() {
        println!("no subagent transcripts found");
        return;
    }

    let agent_id_width = agent_id_width(summaries);
    println!("{}", table_header(agent_id_width).bold());
    for summary in summaries {
        print_row(summary, agent_id_width);
    }
}

/// AGENT ID is the only variable-width column: real ids (user-chosen, up to
/// ~34 chars, e.g. "aexplore-doctrine-1d2b4f8be46cae08") exceed the historical
/// 28-char width, and a hardcoded width pushes every later column out of
/// alignment on any row with a long id. Compute the width from the data,
/// floored at the pre-existing 28-char width so a short list still looks
/// exactly as it did before. Ids are never truncated -- this is the handle a
/// human copy-pastes into `harvest --id`.
fn agent_id_width(summaries: &[SubagentSummary]) -> usize {
    const MIN_AGENT_ID_WIDTH: usize = 28;
    summaries
        .iter()
        .map(|summary| summary.agent_id.chars().count())
        .max()
        .unwrap_or(0)
        .max(MIN_AGENT_ID_WIDTH)
}

fn table_header(agent_id_width: usize) -> String {
    format!(
        "{:<agent_id_width$} {:<10} {:>9} {:>5}  {:<last_tool_width$} {:<agent_type_width$} {:<model_width$} {:>reqs_width$} {:>peak_tokens_width$}",
        "AGENT ID", "STATE", "IDLE(S)", "TURNS", "LAST TOOL", "AGENT TYPE", "MODEL", "REQS", "PEAK TOK",
        last_tool_width = LAST_TOOL_WIDTH,
        agent_type_width = AGENT_TYPE_WIDTH,
        model_width = MODEL_WIDTH,
        reqs_width = REQS_WIDTH,
        peak_tokens_width = PEAK_TOKENS_WIDTH,
        agent_id_width = agent_id_width,
    )
}

fn print_row(summary: &SubagentSummary, agent_id_width: usize) {
    let model = display_model(summary.model.as_deref());
    println!(
        "{:<agent_id_width$} {:<10} {:>9} {:>5}  {:<last_tool_width$} {:<agent_type_width$} {:<model_width$} {:>reqs_width$} {:>peak_tokens_width$}",
        summary.agent_id,
        summary.state.label(),
        summary.idle_secs,
        summary.turns,
        summary.last_tool.as_deref().unwrap_or("-"),
        text_cell(summary.agent_type.as_deref(), AGENT_TYPE_WIDTH),
        text_cell(model.as_deref(), MODEL_WIDTH),
        reqs_label(summary.request_count),
        peak_tokens_label(summary.peak_resident_tokens, summary.peak_tokens_over_ceiling),
        last_tool_width = LAST_TOOL_WIDTH,
        agent_type_width = AGENT_TYPE_WIDTH,
        model_width = MODEL_WIDTH,
        reqs_width = REQS_WIDTH,
        peak_tokens_width = PEAK_TOKENS_WIDTH,
        agent_id_width = agent_id_width,
    );
}

fn text_cell(value: Option<&str>, width: usize) -> String {
    value.unwrap_or("-").chars().take(width).collect()
}

/// Strip the `claude-` prefix and, when present, a trailing `-YYYYMMDD` date
/// segment: the date carries no information a reader of this table needs,
/// and left in place it consumes most of `MODEL_WIDTH` before truncation
/// even runs (`claude-haiku-4-5-20251001` -> `haiku-4-5-2025` unstripped).
fn display_model(model: Option<&str>) -> Option<String> {
    model.map(|model| {
        let stripped = model.strip_prefix("claude-").unwrap_or(model);
        strip_date_suffix(stripped).to_string()
    })
}

fn strip_date_suffix(model: &str) -> &str {
    let is_date = |suffix: &str| suffix.len() == 8 && suffix.bytes().all(|b| b.is_ascii_digit());
    match model.rsplit_once('-') {
        Some((prefix, suffix)) if is_date(suffix) => prefix,
        _ => model,
    }
}

/// `-` when the transcript had no parseable entry at all; a genuine zero
/// (some entries parsed, none carried a `requestId`) renders as `0`.
fn reqs_label(request_count: Option<usize>) -> String {
    match request_count {
        Some(count) => count.to_string(),
        None => "-".to_string(),
    }
}

/// Renders the already-computed ceiling flag rather than recomputing it from
/// `tokens` -- `classify::analyze` is the single source of truth for what
/// counts as over the ceiling (`metrics::PEAK_TOKENS_CEILING`).
fn peak_tokens_label(tokens: Option<u64>, over_ceiling: bool) -> String {
    match tokens {
        Some(tokens) if over_ceiling => format!("{tokens}!"),
        Some(tokens) => tokens.to_string(),
        None => "-".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn print_table_handles_empty_summaries_without_panicking() {
        print_table(&[]);
    }

    #[test]
    fn missing_ledger_type_renders_as_a_dash() {
        assert_eq!(text_cell(None, AGENT_TYPE_WIDTH), "-");
    }

    #[test]
    fn peak_at_ceiling_gets_greppable_marker() {
        assert_eq!(peak_tokens_label(Some(150_000), true), "150000!");
    }

    #[test]
    fn peak_under_ceiling_has_no_marker() {
        assert_eq!(peak_tokens_label(Some(150_000), false), "150000");
    }

    #[test]
    fn no_parseable_entry_renders_reqs_as_a_dash_not_zero() {
        assert_eq!(reqs_label(None), "-");
    }

    #[test]
    fn genuine_zero_reqs_renders_as_zero() {
        assert_eq!(reqs_label(Some(0)), "0");
    }

    #[test]
    fn display_model_drops_prefix_and_trailing_date() {
        assert_eq!(
            display_model(Some("claude-haiku-4-5-20251001")).as_deref(),
            Some("haiku-4-5")
        );
        assert_eq!(
            display_model(Some("claude-opus-4-1-20250805")).as_deref(),
            Some("opus-4-1")
        );
    }

    #[test]
    fn display_model_leaves_an_undated_id_alone() {
        assert_eq!(
            display_model(Some("claude-opus-5")).as_deref(),
            Some("opus-5")
        );
    }
}
