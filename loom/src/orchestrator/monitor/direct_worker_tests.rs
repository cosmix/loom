//! Authenticating a direct worker: synthetic process tables for each rule.

use super::*;

const AGENT: u32 = 100;
const INVOCATION: &str = "inv-0123456789abcdef0123456789abcdef";

fn row(pid: u32, ppid: u32, name: &str) -> ProcessRow {
    ProcessRow {
        pid,
        ppid,
        zombie: false,
        name: name.to_string(),
    }
}

fn authentic_argv(pid: u32) -> Option<String> {
    Some(match pid {
        101 => format!("zsh\0-c\0codex-forward.sh task 'x' --invocation-id {INVOCATION}\0"),
        103 => "python3\0/hooks/_codex-direct.py\0--timeout-ms\0".to_string(),
        _ => "codex\0exec\0--json\0".to_string(),
    })
}

fn authentic_tree() -> Vec<ProcessRow> {
    vec![
        row(101, AGENT, "zsh"),
        row(102, 101, "bash"),
        row(103, 102, "python3"),
        row(104, 103, "codex"),
    ]
}

#[test]
fn the_authorized_launch_names_its_supervisor() {
    let rows = authentic_tree();
    assert_eq!(
        authorized_supervisor(AGENT, &rows, INVOCATION, authentic_argv),
        Some(103)
    );
}

#[test]
fn a_descendant_cannot_supply_the_launchers_invocation() {
    // The sandboxed shell quotes the id in its own argv, and a process it starts
    // is named like the supervisor. The direct child's argv does not hold it.
    let rows = authentic_tree();
    let forged = |pid| {
        Some(match pid {
            101 => "zsh\0-c\0bwrap -- sh forge.sh\0".to_string(),
            102 => format!("bash\0--invocation-id {INVOCATION}\0"),
            _ => "python3\0_codex-direct.py\0".to_string(),
        })
    };
    assert_eq!(
        authorized_supervisor(AGENT, &rows, INVOCATION, forged),
        None
    );
}

#[test]
fn only_a_direct_launcher_child_of_the_agent_counts() {
    let mut other_parent = authentic_tree();
    other_parent[0].ppid = 7;
    assert_eq!(
        authorized_supervisor(AGENT, &other_parent, INVOCATION, authentic_argv),
        None
    );
    let mut other_name = authentic_tree();
    other_name[0].name = "node".to_string();
    assert_eq!(
        authorized_supervisor(AGENT, &other_name, INVOCATION, authentic_argv),
        None
    );
    let mut finished = authentic_tree();
    finished[0].zombie = true;
    assert_eq!(
        authorized_supervisor(AGENT, &finished, INVOCATION, authentic_argv),
        None
    );
}

#[test]
fn two_launchers_with_one_invocation_are_refused() {
    let mut rows = authentic_tree();
    rows.push(row(201, AGENT, "zsh"));
    let argv = |pid| authentic_argv(if pid == 201 { 101 } else { pid });
    assert_eq!(authorized_supervisor(AGENT, &rows, INVOCATION, argv), None);
}

#[test]
fn a_launch_without_a_supervisor_has_no_worker() {
    let rows = vec![row(101, AGENT, "zsh"), row(102, 101, "bash")];
    assert_eq!(
        authorized_supervisor(AGENT, &rows, INVOCATION, authentic_argv),
        None
    );
}

#[test]
fn supervisor_polling_is_not_worker_progress() {
    let rows = authentic_tree();
    let polling_only = |pid| Some(if pid == 103 { 9_000 } else { 5 });
    assert_eq!(worker_cpu_ticks(103, &rows, polling_only), Some(5));
    let no_worker = vec![row(103, 102, "python3")];
    assert_eq!(worker_cpu_ticks(103, &no_worker, polling_only), None);
    let mut dead_worker = authentic_tree();
    dead_worker[3].zombie = true;
    assert_eq!(worker_cpu_ticks(103, &dead_worker, polling_only), None);
}

#[test]
fn a_descendant_exiting_mid_sample_does_not_void_the_sample() {
    let mut rows = authentic_tree();
    rows.push(row(105, 104, "cargo"));
    let exited = |pid| (pid != 105).then_some(5);
    assert_eq!(worker_cpu_ticks(103, &rows, exited), Some(5));
    let root_gone = |pid| (pid != 104).then_some(5);
    assert_eq!(worker_cpu_ticks(103, &rows, root_gone), None);
}

#[test]
fn a_prompt_naming_the_supervisor_does_not_make_the_wrapper_one() {
    // The prompt is stage text in the wrapper's argv. Taking the wrapper for the
    // supervisor would count the real supervisor's polling as worker CPU.
    let rows = authentic_tree();
    let quoting = |pid| {
        (pid == 102)
            .then(|| format!("bash\0/hooks/codex-forward.sh\0task\0see {SUPERVISOR_SCRIPT}\0"))
            .or_else(|| authentic_argv(pid))
    };
    assert_eq!(
        authorized_supervisor(AGENT, &rows, INVOCATION, quoting),
        Some(103)
    );
}

fn companion_tree() -> Vec<ProcessRow> {
    vec![
        row(101, AGENT, "zsh"),
        row(102, 101, "bash"),
        row(103, 102, "node"),
    ]
}

fn companion_argv(pid: u32) -> Option<String> {
    Some(match pid {
        101 => format!("zsh\0-c\0codex-forward.sh task 'x' --invocation-id {INVOCATION}\0"),
        102 => "bash\0/hooks/codex-forward.sh\0task\0x\0".to_string(),
        103 => format!("node\0/a b/{COMPANION_SCRIPT}\0status\0task-one\0--wait\0"),
        _ => return None,
    })
}

#[test]
fn only_the_authorized_launch_vouches_for_its_companion_job() {
    let rows = companion_tree();
    let awaits = |rows: &[ProcessRow], invocation: &str, job: &str| {
        awaits_job(AGENT, rows, invocation, job, companion_argv)
    };
    assert!(awaits(&rows, INVOCATION, "task-one"));
    assert!(!awaits(&rows, INVOCATION, "task-two"), "another job");
    assert!(!awaits(&rows, "inv-other", "task-one"), "another launch");

    let mut orphaned = companion_tree();
    orphaned[1].ppid = 1;
    assert!(!awaits(&orphaned, INVOCATION, "task-one"), "not under it");
}

#[test]
fn a_companion_wait_is_not_a_prompt_and_not_below_a_direct_supervisor() {
    let rows = companion_tree();
    // The launch job's own prompt can spell out a wait for a planted job.
    let launching = |pid| {
        (pid == 103)
            .then(|| {
                format!(
                    "node\0/a/{COMPANION_SCRIPT}\0task\0/a/{COMPANION_SCRIPT} status task-one\0"
                )
            })
            .or_else(|| companion_argv(pid))
    };
    assert!(!awaits_job(AGENT, &rows, INVOCATION, "task-one", launching));

    // A Codex tool under a direct supervisor can run anything: the supervisor
    // is shallower, so the lane is direct and no job is vouched for.
    let mut direct = authentic_tree();
    direct.push(row(105, 104, "node"));
    let tool = |pid| {
        (pid == 105)
            .then(|| format!("node\0/a/{COMPANION_SCRIPT}\0status\0task-one\0"))
            .or_else(|| authentic_argv(pid))
    };
    assert!(!awaits_job(AGENT, &direct, INVOCATION, "task-one", tool));
}
