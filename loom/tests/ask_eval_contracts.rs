//! `loom ask` behaviour the frozen `ask_contracts.rs` leaves open: where a cases
//! file's paths resolve, what a signal does to an eval run and a worker's process
//! group, and the credential paths loom's own sandbox denies.

#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;
#[path = "ask_contracts/support.rs"]
#[allow(dead_code)]
mod support;

use std::time::Duration;

use nix::sys::signal::{kill, Signal};
use nix::unistd::Pid;
use serial_test::serial;

use support::{
    ask_args, assert_gone, read_pid, row, wait_for_pid_file, write_cases, Running, Scene, DEADLINE,
};

fn signal(running: &Running, signal: Signal) {
    let pid = Pid::from_raw(i32::try_from(running.pid()).expect("pid fits i32"));
    kill(pid, signal).expect("send signal");
}

#[test]
#[serial]
fn a_case_path_resolves_against_a_cases_file_in_a_subdirectory() {
    let scene = Scene::new();
    scene.write("evals/docs/doc.md", "DOCUMENT-BODY\n");
    scene.write("docs/doc.md", "WRONG-BODY\n");
    scene.serve_claude();
    let one = [row("one", &["ok"], &[])];
    write_cases(&scene, "evals/cases.json", &one, "docs/doc.md");

    let done = scene.eval("evals/cases.json");

    done.assert_exit(0);
    done.assert_stdout_has("one: pass");
    let payload = scene.stdin();
    assert!(payload.contains("DOCUMENT-BODY"), "{payload}");
    assert!(!payload.contains("WRONG-BODY"), "{payload}");
    assert!(
        payload.contains("<file path=\"evals/docs/doc.md\">"),
        "{payload}"
    );
}

#[test]
#[serial]
fn sigterm_kills_the_workers_process_group() {
    let scene = Scene::new();
    scene.write("doc.md", "text\n");
    let (pid, child) = (scene.sh("pid"), scene.sh("child"));
    scene.fake(
        "claude",
        &format!("sleep 60 &\necho $! > {child}\necho $$ > {pid}\nsleep 60"),
    );
    let args = ask_args(&["--timeout-secs", "120"], &["doc.md"]);
    let mut running = scene.start(scene.loom(&args));
    wait_for_pid_file(&mut running, &scene.rec("pid"));
    let pids = [read_pid(&scene.rec("pid")), read_pid(&scene.rec("child"))];

    signal(&running, Signal::SIGTERM);

    running.wait_within(Duration::from_secs(5)).assert_exit(130);
    pids.into_iter().for_each(assert_gone);
}

#[test]
#[serial]
fn an_interrupted_eval_starts_no_further_case() {
    let scene = Scene::new();
    scene.write("doc.md", "text\n");
    scene.fake(
        "claude",
        &format!("echo $$ > {}\nsleep 60", scene.sh("pid")),
    );
    let rows = [row("first", &["ok"], &[]), row("second", &["ok"], &[])];
    write_cases(&scene, "cases.json", &rows, "doc.md");
    let args = ["ask", "--eval", "cases.json", "--timeout-secs", "120"];
    let mut running = scene.start(scene.loom(&args));
    wait_for_pid_file(&mut running, &scene.rec("pid"));
    let worker = read_pid(&scene.rec("pid"));

    signal(&running, Signal::SIGTERM);

    let done = running.wait_within(Duration::from_secs(5));
    done.assert_exit(130);
    assert_gone(worker);
    assert_eq!(scene.runs(), 1, "a further case started");
    assert!(!done.stdout.contains("second"), "{}", done.stdout);
}

#[test]
#[serial]
fn credentials_the_sandbox_denies_are_refused_from_the_home_directory() {
    let scene = Scene::new();
    scene.serve_claude();
    let denied = [
        ".claude/.credentials.json",
        ".kube/config",
        ".docker/config.json",
        ".config/gh/hosts.yml",
        ".pypirc",
        ".ssh/config",
        ".config/gcloud/application_default_credentials.json",
        ".codex/auth.json",
    ];
    for path in denied {
        scene.write(path, "fake credential content\n");
        let mut command = scene.loom(&ask_args(&[], &[path]));
        command.env("HOME", scene.project());
        let done = scene.start(command).wait_within(DEADLINE);
        done.assert_refused("credential");
        assert_eq!(scene.runs(), 0, "{path} reached the worker");
    }
}

#[test]
#[serial]
fn a_refused_path_reads_as_refused_and_counts_against_accuracy() {
    let scene = Scene::new();
    scene.write("secret.pem", "fake credential content\n");
    scene.serve_claude();
    let refused = [row("refused-case", &["ok"], &[])];
    write_cases(&scene, "cases.json", &refused, "secret.pem");

    let done = scene.eval("cases.json");

    done.assert_exit(1);
    done.assert_stdout_has("refused-case: refused (");
    done.assert_stdout_has("credential");
    done.assert_stdout_has("accuracy: 0.00");
    assert_eq!(scene.runs(), 0, "a refused case started a worker");
}
