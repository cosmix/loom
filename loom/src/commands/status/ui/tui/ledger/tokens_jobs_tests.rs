//! Native Codex jobs in the tokens pane: labels, outcomes, durations, thread-less rows.

use super::*;
use crate::commands::status::ui::tui::ledger::tokens_pane::{
    live_lines, tokens_pane_lines, PANE_MAX_ROWS,
};
use crate::native_worker::model::Phase;
use crate::tokens::{PlanFlow, TokenFlow, TokenUsage};
use chrono::TimeZone;

fn at(secs: i64) -> DateTime<Utc> {
    Utc.timestamp_opt(1_700_000_000 + secs, 0).unwrap()
}

fn job(unit: &str, thread: Option<&str>, outcome: Option<Outcome>, ended: Option<i64>) -> CodexJob {
    CodexJob {
        unit_id: unit.into(),
        invocation_id: format!("inv-{unit}"),
        model: "gpt-6.1-terra".into(),
        effort: "xhigh".into(),
        phase: if outcome.is_some() {
            Phase::Terminal
        } else {
            Phase::Running
        },
        outcome,
        outcome_reason: None,
        thread_id: thread.map(Into::into),
        started_at: at(0),
        ended_at: ended.map(at),
        deadline_secs: 1500,
    }
}

fn codex_agent(thread: &str, unit: Option<&str>, active: bool) -> AgentFlow {
    AgentFlow {
        id: format!("codex-{}", &thread[..8]),
        kind: AgentKind::Codex,
        agent_type: None,
        forwarder: None,
        model: Some("gpt-6.1-terra".into()),
        effort: Some("xhigh".into()),
        unit_id: unit.map(Into::into),
        active,
        total: TokenUsage {
            input: 1_200,
            ..TokenUsage::default()
        },
        rate: TokenUsage::default(),
        series: vec![5; 4],
    }
}

fn flow(agents: Vec<AgentFlow>) -> TokenFlow {
    TokenFlow {
        observed_at: 1_700_000_000,
        bucket_secs: 30,
        plan: PlanFlow::default(),
        stages: vec![StageFlow {
            stage_id: "s1".into(),
            total: TokenUsage::default(),
            rate: TokenUsage::default(),
            series: vec![0; 4],
            hit_series: Vec::new(),
            agents,
        }],
        models: Vec::new(),
        quota_left: Vec::new(),
        unreadable_lines: 0,
    }
}

fn summary(jobs: Vec<CodexJob>) -> Vec<StageSummary> {
    vec![StageSummary {
        id: "s1".into(),
        codex_jobs: jobs,
        ..StageSummary::default()
    }]
}

fn rows(lines: &[ratatui::text::Line<'static>]) -> Vec<String> {
    lines.iter().map(ToString::to_string).collect()
}

#[test]
fn durations_read_as_seconds_minutes_and_hours() {
    for (secs, text) in [
        (-5, "0s"),
        (23, "23s"),
        (60, "1m00s"),
        (250, "4m10s"),
        (3_720, "1h02m"),
    ] {
        assert_eq!(duration_text(secs), text);
    }
}

/// The web view asserts the same table (`web/src/lib/codex-jobs.test.ts`).
#[test]
fn durations_and_deadlines_read_the_same_in_both_views() {
    let table = [
        (0, "0s", "0s"),
        (23, "23s", "23s"),
        (90, "1m30s", "1m30s"),
        (1_500, "25m00s", "25m"),
        (3_600, "1h00m", "1h00m"),
        (3_661, "1h01m", "1h01m"),
    ];
    for (secs, duration, deadline) in table {
        assert_eq!(duration_text(secs), duration);
        assert_eq!(deadline_text(secs as u64), deadline);
    }
}

#[test]
fn a_run_past_its_deadline_reads_overdue_and_a_finished_one_does_not() {
    let running = job("u", None, None, None);
    assert_eq!(state_text(&running, at(1_500)).0, "running 25m00s / 25m");
    assert_eq!(state_text(&running, at(1_501)).0, "overdue 25m01s / 25m");
    assert_eq!(state_text(&running, at(1_501)).1.fg, Some(Color::Yellow));
    let late = job("u", None, Some(Outcome::Succeeded), Some(1_800));
    assert_eq!(state_text(&late, at(2_000)).0, "succeeded 30m00s / 25m");
}

#[test]
fn a_deadline_reads_whole_minutes_bare_and_the_rest_as_a_duration() {
    for (secs, text) in [
        (1_500, "25m"),
        (60, "1m"),
        (90, "1m30s"),
        (45, "45s"),
        (3_600, "1h00m"),
    ] {
        assert_eq!(deadline_text(secs), text);
    }
    let mut short = job("u", None, Some(Outcome::Failed), Some(2));
    short.deadline_secs = 90;
    assert_eq!(state_text(&short, at(130)).0, "failed 2s / 1m30s");
}

#[test]
fn outcome_and_duration_come_from_the_job() {
    let cases = [
        (None, Some(130), "running 2m10s / 25m"),
        (Some(Outcome::Succeeded), Some(23), "succeeded 23s / 25m"),
        (Some(Outcome::Failed), Some(4), "failed 4s / 25m"),
        (Some(Outcome::Cancelled), Some(9), "cancelled 9s / 25m"),
        (
            Some(Outcome::TimedOut),
            Some(1_500),
            "timed out 25m00s / 25m",
        ),
    ];
    for (outcome, ended, text) in cases {
        let now = at(130);
        let ended = if outcome.is_some() { ended } else { None };
        assert_eq!(state_text(&job("u", None, outcome, ended), now).0, text);
    }
}

#[test]
fn a_terminal_job_without_an_outcome_is_never_running() {
    let mut finished = job("u", None, None, Some(5));
    finished.phase = Phase::Terminal;
    assert_eq!(state_text(&finished, at(130)).0, "unknown 5s / 25m");
}

#[test]
fn an_agent_matches_the_job_of_its_thread() {
    let thread = "01a1e59a-099e-7ef2-92c0-52e2b3314eb2";
    let jobs = [
        job("hex", Some("ffffffff-0000"), Some(Outcome::Failed), Some(3)),
        job("hex", Some(thread), Some(Outcome::Succeeded), Some(23)),
    ];
    let agent = codex_agent(thread, Some("hex"), false);
    assert_eq!(
        job_of(&agent, &jobs).unwrap().thread_id.as_deref(),
        Some(thread)
    );
    // No unit id yet: the thread alone still finds the job, as on the web dashboard.
    assert_eq!(
        job_of(&codex_agent(thread, None, false), &jobs)
            .unwrap()
            .thread_id
            .as_deref(),
        Some(thread)
    );
    assert!(job_of(&codex_agent("eeeeeeee-0000", None, false), &jobs).is_none());
}

#[test]
fn a_codex_row_shows_the_unit_effort_outcome_and_duration() {
    let thread = "01a1e59a-099e-7ef2-92c0-52e2b3314eb2";
    let jobs = summary(vec![job(
        "hex-test",
        Some(thread),
        Some(Outcome::Succeeded),
        Some(23),
    )]);
    let flow = flow(vec![codex_agent(thread, Some("hex-test"), true)]);
    let lines = rows(&tokens_pane_lines(&flow, &jobs, 132, PANE_MAX_ROWS));
    let row = lines.iter().find(|row| row.contains("codex")).unwrap();
    assert!(row.contains("codex hex-test"), "{row}");
    assert!(row.contains("gpt-6.1-terra/xhigh"), "{row}");
    assert!(row.contains("succeeded 23s / 25m"), "{row}");
    assert!(!row.contains("live") && !row.contains("01a1"), "{row}");
    assert_eq!(lines.len(), 3, "no extra row for the matched job");
}

#[test]
fn a_job_without_a_thread_is_its_own_row_with_no_tokens() {
    let jobs = summary(vec![job("boom", None, Some(Outcome::Failed), Some(4))]);
    // The live agent keeps the stage on the pane; the failed job is listed under it.
    let flow = flow(vec![codex_agent("01a1e59a-099e", None, true)]);
    let lines = rows(&live_lines(&flow, &jobs, 132));
    let row = lines.iter().find(|row| row.contains("boom")).unwrap();
    assert!(row.starts_with("  codex boom"), "{row}");
    assert!(row.contains("gpt-6.1-terra/xhigh"), "{row}");
    assert!(row.contains("failed 4s / 25m"), "{row}");
    assert!(row.ends_with('—'), "{row}");
    assert_eq!(row.chars().count(), 132);
}

#[test]
fn a_running_job_without_a_thread_keeps_its_stage_on_the_pane() {
    let jobs = summary(vec![job("boom", None, None, None)]);
    let flow = flow(vec![codex_agent("01a1e59a-099e", None, false)]);
    let lines = rows(&live_lines(&flow, &jobs, 132));
    assert!(
        lines.iter().any(|row| row.contains("codex boom")),
        "{lines:?}"
    );
}

#[test]
fn a_finished_job_without_a_thread_does_not_keep_its_stage_on_the_pane() {
    let jobs = summary(vec![job("boom", None, Some(Outcome::Failed), Some(4))]);
    let flow = flow(vec![codex_agent("01a1e59a-099e", None, false)]);
    let lines = rows(&live_lines(&flow, &jobs, 132));
    assert_eq!(lines.len(), 1, "{lines:?}");
    assert!(lines[0].contains("no active agent"));
}

#[test]
fn a_stage_without_jobs_or_live_agents_stays_off_the_pane() {
    let flow = flow(vec![codex_agent("01a1e59a-099e", None, false)]);
    let lines = rows(&live_lines(&flow, &summary(Vec::new()), 132));
    assert_eq!(lines.len(), 1);
    assert!(lines[0].contains("no active agent"));
}
