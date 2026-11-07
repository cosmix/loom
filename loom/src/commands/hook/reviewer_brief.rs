//! `loom hook reviewer-brief`: the stage's current test-integrity events, with
//! each lost assertion statement, as the brief the spawn guard appends to a
//! `loom-code-reviewer` spawn. An integration-verify stage also gets the
//! plan-wide change. Every failure prints `{}`; the guard then adds nothing.

use anyhow::Result;
use serde::Serialize;
use std::io::Write;
use std::path::PathBuf;
use uuid::Uuid;

use super::reviewer_brief_plan::{clean, render_integration, IntegrationContext};
use super::target::non_empty_env;
use crate::context::render::fence_for;
use crate::fs::work_dir::WorkDir;
use crate::models::stage::{Stage, StageType};
use crate::validation::validate_id;
use crate::verify::integrity::{self, EventKind, IntegrityEvent};

/// Cut applied to each lost statement; statements are agent-written.
const MAX_STATEMENT_BYTES: usize = 400;
const MAX_STATEMENTS_PER_EVENT: usize = 25;
const MAX_ID_BYTES: usize = 300;
const MAX_REASON_BYTES: usize = 300;
/// Cap on the brief body.
const MAX_BRIEF_BYTES: usize = 16 * 1024;
/// Room kept for the line announcing omitted events.
const OMISSION_RESERVE: usize = 256;
const MARKER_PREFIX: &str = "<!-- loom-";
const NO_RECORD: &str = "not accepted";

#[derive(Serialize)]
struct Envelope {
    nonce: String,
    brief: String,
}

struct Config {
    work_dir: PathBuf,
    stage: Stage,
}

impl Config {
    fn from_env() -> Option<Self> {
        let stage_id = non_empty_env("LOOM_STAGE_ID")?;
        validate_id(&stage_id).ok()?;
        let resolved = WorkDir::new(non_empty_env("LOOM_WORK_DIR")?).ok()?;
        let work_dir = resolved.root().to_path_buf();
        if !work_dir.is_dir() {
            return None;
        }
        let stage = crate::verify::load_stage(&stage_id, &work_dir).ok()?;
        Some(Self { work_dir, stage })
    }
}

/// An integrity event with the reason `integrity.json` does not accept it.
pub(super) struct EventView {
    pub event: IntegrityEvent,
    /// `None` when the event is accepted.
    pub shortfall: Option<String>,
}

/// `loom hook reviewer-brief`: print one `{"nonce":"<32 hex>","brief":"<text>"}`
/// line, or `{}`. Never an `Err` for a missing environment, an unreadable
/// stage or a scan failure. Reads no stdin.
pub fn emit_reviewer_brief() -> Result<()> {
    let line = brief_line(Config::from_env().as_ref());
    let _ = writeln!(std::io::stdout().lock(), "{line}");
    Ok(())
}

fn brief_line(config: Option<&Config>) -> String {
    let Some(config) = config else {
        return "{}".to_string();
    };
    build_envelope(config)
        .and_then(|envelope| serde_json::to_string(&envelope).ok())
        .unwrap_or_else(|| "{}".to_string())
}

fn build_envelope(config: &Config) -> Option<Envelope> {
    let stage = &config.stage;
    let (worktree, target) =
        crate::commands::stage::stage_worktree_and_target(&config.work_dir, stage).ok()?;
    let scan = integrity::scan(&worktree, &target, &stage.ratchet_files).ok()?;
    let accepted = integrity::load_accepted(&config.work_dir, &stage.id).ok()?;
    let events: Vec<EventView> = scan
        .events
        .into_iter()
        .map(|event| EventView {
            shortfall: integrity::shortfall(&event, &accepted),
            event,
        })
        .collect();
    let integration = (stage.stage_type == StageType::IntegrationVerify)
        .then(|| integration_context(config, &worktree, &target));
    let nonce = Uuid::new_v4().simple().to_string();
    let brief = render(&nonce, &stage.id, &target, &events, integration.as_ref())?;
    Some(Envelope { nonce, brief })
}

fn integration_context(
    config: &Config,
    worktree: &std::path::Path,
    target: &str,
) -> IntegrationContext {
    let stages = crate::verify::transitions::list_all_stages(&config.work_dir).unwrap_or_default();
    IntegrationContext::collect(
        &config.work_dir,
        worktree,
        target,
        &stages,
        &config.stage.id,
    )
}

/// The brief text, or `None` when there is nothing to say (no event and not an
/// integration-verify stage) or a line other than the first would read as a
/// loom marker. The integration section is appended last and its size is
/// reserved first, so the cap drops events, never that section.
pub(super) fn render(
    nonce: &str,
    stage_id: &str,
    target: &str,
    events: &[EventView],
    integration: Option<&IntegrationContext>,
) -> Option<String> {
    if events.is_empty() && integration.is_none() {
        return None;
    }
    let integration_text = integration.map(render_integration);
    let reserved = integration_text.as_ref().map_or(0, String::len) + OMISSION_RESERVE;
    let mut body = format!("{MARKER_PREFIX}reviewer-brief nonce={nonce} -->\n");
    if !events.is_empty() {
        push_events(
            &mut body,
            stage_id,
            target,
            events,
            MAX_BRIEF_BYTES.saturating_sub(reserved),
        );
    }
    if let Some(text) = integration_text {
        body.push_str(&text);
    }
    let forged = body
        .lines()
        .skip(1)
        .any(|line| line.trim_start().starts_with(MARKER_PREFIX));
    (!forged).then_some(body)
}

fn push_events(
    body: &mut String,
    stage_id: &str,
    target: &str,
    events: &[EventView],
    budget: usize,
) {
    if budget == 0 {
        body.push_str(&omission_line(stage_id));
        return;
    }
    body.push_str(&format!(
        "## Test-integrity events (computed by loom)\n\
         Stage `{stage_id}` has {} test-integrity event(s) against `{}`. For every lost \
         assertion statement below, report a finding unless a check that is equal or \
         stronger now covers the same guard; name that check with file:line in the finding. \
         An event the stage description's INTEGRITY line (pasted into your prompt) names as \
         expected, with its reason, needs a finding only when the changed statement no \
         longer asserts what that reason describes. Accepted events are shown for context.\n",
        events.len(),
        clean(target, MAX_ID_BYTES),
    ));
    for view in events {
        let block = render_event(view, stage_id);
        if body.len() + block.len() > budget {
            body.push_str(&omission_line(stage_id));
            return;
        }
        body.push_str(&block);
    }
}

fn omission_line(stage_id: &str) -> String {
    format!("Further events omitted; run loom stage review integrity {stage_id}.\n")
}

fn render_event(view: &EventView, stage_id: &str) -> String {
    let event = &view.event;
    let mut out = format!(
        "### {} ({})\n",
        clean(&event.id, MAX_ID_BYTES),
        status_text(view.shortfall.as_deref())
    );
    match event.kind {
        EventKind::AssertionEdit => out.push_str(&statements_block(event, stage_id)),
        EventKind::DeclTotal | EventKind::AssertTotal => out.push_str(&format!(
            "{}\nCount events carry no statement; check that each removed test has a replacement.\n",
            integrity::describe(event)
        )),
        EventKind::Ratchet => out.push_str("Ratchet file loosened from base.\n"),
    }
    out
}

fn status_text(shortfall: Option<&str>) -> String {
    match shortfall {
        None => "accepted".to_string(),
        Some(NO_RECORD) => "not accepted: no accepted dispute covers it".to_string(),
        Some(reason) => format!("not accepted: {}", clean(reason, MAX_REASON_BYTES)),
    }
}

/// The lost statements, one per line in a `text` fence one backtick longer
/// than any backtick run inside it, so no statement can close the fence.
fn statements_block(event: &IntegrityEvent, stage_id: &str) -> String {
    let lines: Vec<String> = event
        .detail
        .iter()
        .take(MAX_STATEMENTS_PER_EVENT)
        .map(|statement| clean(statement, MAX_STATEMENT_BYTES))
        .collect();
    let fence = fence_for(&lines.join("\n"));
    let mut out = format!(
        "Lost assertion statements ({}):\n{fence}text\n",
        event.detail.len()
    );
    for line in &lines {
        out.push_str(line);
        out.push('\n');
    }
    out.push_str(&fence);
    out.push('\n');
    let rest = event.detail.len().saturating_sub(lines.len());
    if rest > 0 {
        out.push_str(&format!(
            "... {rest} more; run loom stage review integrity {stage_id}\n"
        ));
    }
    out
}

#[cfg(test)]
#[path = "reviewer_brief_tests.rs"]
mod tests;
