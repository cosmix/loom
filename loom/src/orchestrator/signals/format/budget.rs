use std::path::Path;

use crate::quota::{self, ProviderQuota, QuotaSnapshot, QuotaWindow, WindowKind};

pub(crate) const QUOTA_PRESSURE_PERCENT: f64 = 75.0;
pub(crate) const PRESSURE_MIN_SECS_TO_RESET: i64 = 1800;

const CLAUDE_ROUTING: &str = "Routing: Claude quota is under pressure. Give every straightforward, well-specified piece to codex where this stage licenses codex and its Codex Implementers section says codex is available, and every piece whose brief settles every decision to haiku. Never move a piece below the tier Rule 7 names for it.";
const CODEX_ROUTING: &str = "Routing: Codex quota is under pressure. Give straightforward, well-specified pieces to the matching Claude tier: terra- and sol-tier work to sonnet, boilerplate and simple unit tests to haiku.";
const BOTH_ROUTING: &str = "Routing: both providers are under pressure. Allocate as Rule 7 says.";

/// A reading counts only when it is recent, error-free and carries at least one
/// window: pollers write `windows: []` for a provider that reports none, and a
/// section holding only the heading would tell the agent nothing.
fn fresh(provider: Option<&ProviderQuota>, now: i64) -> Option<&ProviderQuota> {
    provider.filter(|p| {
        p.error.is_none()
            && !p.windows.is_empty()
            && quota::age_secs(p.observed_at, now) < quota::STALE_AFTER_SECS
    })
}

fn under_pressure(provider: Option<&ProviderQuota>, now: i64) -> bool {
    provider.is_some_and(|p| {
        p.windows.iter().any(|w| {
            w.used_percent >= QUOTA_PRESSURE_PERCENT
                && w.resets_at
                    .is_none_or(|reset| reset.saturating_sub(now) > PRESSURE_MIN_SECS_TO_RESET)
        })
    })
}

fn format_window(window: &QuotaWindow, now: i64) -> String {
    let mut text = format!("{} {:.0}% used", window.kind.label(), window.used_percent);
    if let Some(reset) = quota::reset_text(window.resets_at, now) {
        if reset == "now" {
            text.push_str(", resets now");
        } else {
            text.push_str(&format!(", resets in {reset}"));
        }
    }
    text
}

fn append_provider(out: &mut String, name: &str, provider: Option<&ProviderQuota>, now: i64) {
    if let Some(provider) = provider {
        let windows = [WindowKind::FiveHour, WindowKind::SevenDay]
            .into_iter()
            .flat_map(|kind| provider.windows.iter().filter(move |w| w.kind == kind))
            .map(|window| format_window(window, now))
            .collect::<Vec<_>>()
            .join("; ");
        out.push_str(&format!("{name}: {windows}.\n"));
    }
}

pub(crate) fn format_model_budget_section(quota: &QuotaSnapshot, now: i64) -> Option<String> {
    let claude = fresh(quota.claude.as_ref(), now);
    let codex = fresh(quota.codex.as_ref(), now);
    if claude.is_none() && codex.is_none() {
        return None;
    }
    let mut out = String::from("## Model Budget\n\n");
    append_provider(&mut out, "Claude", claude, now);
    append_provider(&mut out, "Codex", codex, now);
    let routing = match (under_pressure(claude, now), under_pressure(codex, now)) {
        (true, true) => Some(BOTH_ROUTING),
        (true, false) => Some(CLAUDE_ROUTING),
        (false, true) => Some(CODEX_ROUTING),
        (false, false) => None,
    };
    if let Some(routing) = routing {
        out.push_str(routing);
    }
    Some(out.trim_end_matches('\n').to_string())
}

pub(crate) fn current_model_budget(work_dir: &Path) -> Option<String> {
    format_model_budget_section(
        &quota::read_snapshot(work_dir),
        chrono::Utc::now().timestamp(),
    )
}

pub(crate) fn push_model_budget(budget: Option<&str>, out: &mut String) {
    if let Some(budget) = budget {
        out.push_str(budget);
        out.push_str("\n\n");
    }
}

#[cfg(test)]
#[path = "budget_tests.rs"]
mod tests;
