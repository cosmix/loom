use super::*;
use crate::tokens::tests_fixture::{append, body, claude_line, unix_now, usage, Fixture};
use crate::tokens::tracker::CHECKPOINT_FILE;
use std::fs;
use tempfile::TempDir;

const MAIN: &str =
    include_str!("../../tests/fixtures/plan_session/transcripts/subagent-main.jsonl");
const AGENT: &str =
    include_str!("../../tests/fixtures/plan_session/transcripts/subagent-agent.jsonl");

struct Planner {
    _tmp: TempDir,
    tokens: PathBuf,
    group: PlannerGroup,
}

fn planner() -> Planner {
    let tmp = TempDir::new().unwrap();
    let main = tmp.path().join("s1.jsonl");
    let agent = tmp.path().join("s1/subagents/agent-a7f3.jsonl");
    fs::create_dir_all(agent.parent().unwrap()).unwrap();
    fs::write(&main, MAIN).unwrap();
    fs::write(&agent, AGENT).unwrap();
    let subagents = vec![("a7f3".to_owned(), agent, Some("Explore".to_owned()))];
    Planner {
        tokens: tmp.path().join("planner-tokens"),
        group: PlannerGroup {
            id: "s1".to_owned(),
            main,
            subagents,
        },
        _tmp: tmp,
    }
}

fn rows(flows: &[(String, Vec<AgentFlow>)], group: &str) -> Vec<AgentFlow> {
    flows
        .iter()
        .find(|(id, _)| id == group)
        .map(|(_, rows)| rows.clone())
        .unwrap_or_default()
}

#[test]
fn one_group_yields_a_row_for_main_and_one_for_the_subagent() {
    let p = planner();
    let flows = planner_pass(&p.tokens, std::slice::from_ref(&p.group), unix_now()).unwrap();
    let rows = rows(&flows, "s1");
    assert_eq!(rows.len(), 2, "{rows:?}");
    let main = rows.iter().find(|r| r.id != "a7f3").expect("main row");
    assert!(main.model.is_some() && main.total.fresh() > 0, "{main:?}");
    let agent = rows.iter().find(|r| r.id == "a7f3").expect("a7f3 row");
    assert_eq!(agent.agent_type.as_deref(), Some("Explore"));
    assert!(
        agent.model.is_some() && agent.total.fresh() > 0,
        "{agent:?}"
    );
}

#[test]
fn a_second_pass_over_unchanged_files_counts_nothing_again() {
    let p = planner();
    let groups = [p.group.clone()];
    let now = unix_now();
    let first = planner_pass(&p.tokens, &groups, now).unwrap();
    let saved = fs::read(p.tokens.join(CHECKPOINT_FILE)).unwrap();
    let second = planner_pass(&p.tokens, &groups, now).unwrap();
    assert_eq!(rows(&first, "s1"), rows(&second, "s1"));
    assert_eq!(fs::read(p.tokens.join(CHECKPOINT_FILE)).unwrap(), saved);
}

#[test]
fn a_pass_reads_only_what_a_file_gained() {
    let p = planner();
    let groups = [p.group.clone()];
    let now = unix_now();
    let before = planner_pass(&p.tokens, &groups, now).unwrap();
    let line = claude_line("msg-new", now, "claude-haiku-5", &usage(10, 0, 0, 5));
    append(&p.group.subagents[0].1, &body(&[line]));
    let after = planner_pass(&p.tokens, &groups, now).unwrap();
    let total = |flows: &[(String, Vec<AgentFlow>)]| {
        rows(flows, "s1")
            .iter()
            .find(|r| r.id == "a7f3")
            .unwrap()
            .total
    };
    assert_eq!(total(&after).fresh(), total(&before).fresh() + 15);
}

#[test]
fn a_pass_without_the_group_removes_it_from_the_checkpoint() {
    let p = planner();
    let now = unix_now();
    planner_pass(&p.tokens, std::slice::from_ref(&p.group), now).unwrap();
    let flows = planner_pass(&p.tokens, &[], now).unwrap();
    assert!(flows.is_empty());
    let saved = fs::read_to_string(p.tokens.join(CHECKPOINT_FILE)).unwrap();
    let checkpoint: serde_json::Value = serde_json::from_str(&saved).unwrap();
    assert!(checkpoint["stages"].get("s1").is_none(), "{saved}");
}

#[test]
fn an_unsafe_subagent_id_is_not_read() {
    let mut p = planner();
    p.group.subagents[0].0 = "../a7f3".to_owned();
    let flows = planner_pass(&p.tokens, std::slice::from_ref(&p.group), unix_now()).unwrap();
    assert_eq!(rows(&flows, "s1").len(), 1);
}

#[test]
fn planner_passes_never_touch_the_daemons_checkpoint() {
    let fixture = Fixture::new();
    let p = planner();
    let now = unix_now();
    planner_pass(&p.tokens, std::slice::from_ref(&p.group), now).unwrap();
    assert!(!fixture.work_root.join("tokens").exists());

    fixture.add_session(
        "stage-1",
        "sess-1",
        &body(&[claude_line("m1", now, "m", &usage(1, 0, 0, 1))]),
    );
    fixture.scan(now);
    let daemon = fixture.work_root.join("tokens").join(CHECKPOINT_FILE);
    let before = fs::read(&daemon).unwrap();
    planner_pass(&p.tokens, std::slice::from_ref(&p.group), now + 5).unwrap();
    planner_pass(&p.tokens, &[], now + 10).unwrap();
    assert_eq!(fs::read(&daemon).unwrap(), before);
}

#[test]
fn the_thread_passes_the_rows_to_the_sink_and_stops_on_the_flag() {
    let p = planner();
    let (tx, rx) = std::sync::mpsc::channel();
    let group = p.group.clone();
    let stop = Arc::new(AtomicBool::new(false));
    let handle = spawn_planner_tokens(
        p.tokens.clone(),
        Box::new(move || vec![group.clone()]),
        Box::new(move |flows| {
            let _ = tx.send(flows);
        }),
        Arc::clone(&stop),
    )
    .unwrap();
    let flows = rx
        .recv_timeout(Duration::from_secs(10))
        .expect("a sink call");
    assert_eq!(rows(&flows, "s1").len(), 2);
    stop.store(true, Ordering::SeqCst);
    handle.join().expect("the thread ended cleanly");
}
