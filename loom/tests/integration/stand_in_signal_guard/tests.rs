//! The guard's own contract: what it rejects, what it allows, and where it reads.

use std::fs;

use super::regions::{squeeze, strip_comments, test_regions};
use super::*;

fn found_in(source: &str, scope: Scope) -> Vec<String> {
    findings(&squeeze(&strip_comments(source)), scope)
}

/// What `source` yields with rule 1 on (`raw_kills`) or with no rule but
/// those that apply everywhere.
fn found(source: &str, raw_kills: bool) -> Vec<String> {
    let scope = Scope {
        raw_kills,
        ..Scope::default()
    };
    found_in(source, scope)
}

const OLD_LIVE_TREE: &str = r#"
    impl Drop for LiveTree {
        fn drop(&mut self) {
            // SAFETY: the group was created for this child alone by process_group(0).
            unsafe {
                libc::killpg(self.0.id() as i32, libc::SIGKILL);
            }
            let _ = self.0.wait();
        }
    }
"#;

#[test]
fn the_guard_rejects_the_drop_that_hung_the_suite() {
    let in_src = found(OLD_LIVE_TREE, true);
    assert!(
        in_src
            .iter()
            .any(|f| f.starts_with("a kill outside the helper: killpg")),
        "{in_src:?}"
    );
    let anywhere = found(OLD_LIVE_TREE, false);
    assert_eq!(anywhere.len(), 1, "{anywhere:?}");
    assert!(anywhere[0].starts_with("an unchecked kill, then an unbounded wait"));
}

#[test]
fn an_unchecked_child_kill_before_a_wait_is_rejected_even_where_kills_are_allowed() {
    let reaped = "fn drop(&mut self) { let _ = self.0.kill();\nlet _ = self.0.wait(); }";
    assert_eq!(
        found(reaped, false),
        ["an unchecked kill, then an unbounded wait: kill()"]
    );
    let term = "fn f() { let _ = kill(pid, Signal::SIGTERM); child.wait().unwrap(); }";
    assert_eq!(found(term, false).len(), 1);
}

#[test]
fn a_checked_kill_or_a_bounded_wait_is_not_a_hang() {
    let checked = "fn f() { child.kill().unwrap(); child.wait().unwrap(); }";
    assert_eq!(found(checked, false), Vec::<String>::new());
    let bounded = "fn f() { let _ = child.kill(); reap_within(&mut child, REAP_DEADLINE); }";
    assert_eq!(found(bounded, false), Vec::<String>::new());
    let elsewhere = "fn a() { let _ = child.kill(); }\nfn b() { other.wait().unwrap(); }";
    assert_eq!(found(elsewhere, false), Vec::<String>::new());
}

#[test]
fn an_unbounded_wait_on_a_stand_in_is_rejected() {
    let waited = "let status = leader.child().wait().expect(\"reap\");";
    assert_eq!(
        found(waited, false),
        ["an unbounded wait on a stand-in: .child().wait()"]
    );
}

#[test]
fn the_guard_allows_probes_stimuli_prose_and_method_namesakes() {
    let allowed = r#"
        assert_eq!(killpg(group, None), Err(Errno::ESRCH));
        kill(Pid::from_raw(worker as i32), Signal::SIGSTOP).unwrap();
        killpg(Pid::from_raw(group), Signal::SIGHUP).unwrap();
        nix::sys::signal::kill(pid, None);
        backend.kill_session(session);
        fn kill(&self, session: &Session) {}
        // the orchestrator must kill(pid, SIGKILL) the agent, then child.wait()
        let url = "http://a // string"; kill(pid, SIGKILL);
    "#;
    assert_eq!(
        found(allowed, true),
        ["a kill outside the helper: kill(pid,SIGKILL)"]
    );
}

/// Each rule's finding for each place test code lives, keyed by its label.
const LAYOUT: &[(&str, &str, &str)] = &[
    (
        "src/prod_tests.rs",
        "fn a() { let _ = terminate(pid); }",
        "a kill outside the helper: terminate(pid)",
    ),
    (
        "src/support.rs",
        "mod deeper;\nfn b(signal: Signal) { kill(pid, signal).unwrap(); }",
        "a kill outside the helper: kill(pid,signal)",
    ),
    (
        "src/support/deeper.rs",
        "fn c(tree: &mut StandIn) { tree.child().wait().unwrap(); }",
        "an unbounded wait on a stand-in: .child().wait()",
    ),
    (
        "src/prod.rs (#[cfg(test)] item 1)",
        "mod tests { fn d() { let mut c = Command::new(\"sleep\").spawn().unwrap(); \
         c.wait().unwrap(); } }",
        "an unbounded wait on a raw child: c.wait()",
    ),
    (
        "src/one_line.rs",
        "fn e() { let _ = c.kill(); c.wait().unwrap(); }",
        "a kill outside the helper: kill(); an unchecked kill, then an unbounded wait: kill()",
    ),
    (
        "src/helpers.rs",
        "fn g() { let mut s = command.stdout(piped()).spawn().unwrap(); s.wait_with_output(); }",
        "an unbounded wait on a raw child: s.wait_with_output()",
    ),
    (
        "tests/contract.rs",
        "fn f() { kill(pid, SIGKILL).unwrap(); let mut c = Command::new(\"x\").spawn().unwrap(); \
         c.wait().unwrap(); let _ = killpg(group, SIGTERM); c.wait().unwrap(); }",
        "an unchecked kill, then an unbounded wait: killpg(group,SIGTERM)",
    ),
];

/// A crate laid out with test code in each place it can live: a `_tests.rs`
/// file, a file a `#[cfg(test)] mod` declares (on one line, on two, or under
/// an attribute spanning lines), a module nested in one, an inline
/// `#[cfg(test)]` item, and a file under `tests/`.
fn layout() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    let write = |path: &str, text: &str| {
        let path = root.path().join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    };
    let code = |label: &str| LAYOUT.iter().find(|(l, _, _)| *l == label).unwrap().1;
    let lib = "#[cfg(test)]\nmod support;\n#[cfg(test)] mod one_line;\nmod prod;\n\
               #[cfg(all(\n    test,\n    unix\n))]\nmod helpers;\n";
    write("src/lib.rs", lib);
    write("src/helpers.rs", code("src/helpers.rs"));
    write("src/support.rs", code("src/support.rs"));
    write("src/support/deeper.rs", code("src/support/deeper.rs"));
    write("src/one_line.rs", code("src/one_line.rs"));
    let inline = code("src/prod.rs (#[cfg(test)] item 1)");
    let live = "fn live() { let _ = kill(pid, SIGKILL); child.wait().unwrap(); }";
    write("src/prod.rs", &format!("{live}\n#[cfg(test)]\n{inline}\n"));
    write("src/prod_tests.rs", code("src/prod_tests.rs"));
    write("tests/contract.rs", code("tests/contract.rs"));
    write("tests/fixtures/sample.rs", code("src/one_line.rs"));
    root
}

#[test]
fn the_guard_reads_every_place_test_code_lives_and_no_fixture() {
    let root = layout();
    let regions = test_regions(root.path());
    let mut labels: Vec<&str> = regions.iter().map(|r| r.label.as_str()).collect();
    labels.sort_unstable();
    let mut expected: Vec<&str> = LAYOUT.iter().map(|(label, _, _)| *label).collect();
    expected.sort_unstable();
    assert_eq!(labels, expected);
    let mut offenders = offenders(&regions);
    offenders.sort();
    let mut wanted: Vec<String> = LAYOUT
        .iter()
        .map(|(label, _, finding)| format!("{label}: {finding}"))
        .collect();
    wanted.sort();
    assert_eq!(offenders, wanted);
}

#[test]
fn a_kill_with_a_signal_in_a_variable_or_through_terminate_is_a_kill() {
    let in_src = Scope {
        raw_kills: true,
        ..Scope::default()
    };
    let variable = "fn f(s: Signal) { kill(Pid::from_raw(p), s).unwrap(); killpg(g, Some(s)); }";
    assert_eq!(
        found_in(variable, in_src),
        [
            "a kill outside the helper: killpg(g,Some(s))",
            "a kill outside the helper: kill(Pid::from_raw(p),s)",
        ]
    );
    let stimuli = "fn f() { kill(p, Some(Signal::SIGCONT)); libc::kill(p, 0); killpg(g, None); }";
    assert_eq!(found_in(stimuli, in_src), Vec::<String>::new());
    let guard = "fn drop(&mut self) { let _ = crate::process::terminate(self.0); }";
    assert_eq!(
        found_in(guard, in_src),
        ["a kill outside the helper: terminate(self.0)"]
    );
    let namesake = "fn f() { registry::terminate(&jobs, SESSION); job.terminate(); }";
    assert_eq!(found_in(namesake, in_src), Vec::<String>::new());
    let hang = "fn f() { let _ = terminate(child.id()); child.wait().unwrap(); }";
    assert_eq!(
        found_in(hang, Scope::default()),
        ["an unchecked kill, then an unbounded wait: terminate(child.id())"]
    );
}

#[test]
fn a_wait_on_a_child_spawned_in_the_same_function_is_rejected_in_src() {
    let in_src = Scope {
        raw_waits: true,
        ..Scope::default()
    };
    let raw = "fn f() { let mut driver = std::process::Command::new(\"x\").arg(\"a;b\").spawn()\
               .unwrap(); kill(Pid::from_raw(driver.id() as i32), signal).unwrap(); \
               driver.wait().unwrap(); }";
    assert_eq!(
        found_in(raw, in_src),
        ["an unbounded wait on a raw child: driver.wait()"]
    );
    let output = "fn f() { let child: Child = Command::new(\"x\").spawn().unwrap(); \
                  child.wait_with_output().unwrap(); }";
    assert_eq!(
        found_in(output, in_src),
        ["an unbounded wait on a raw child: child.wait_with_output()"]
    );
    let allowed = Scope {
        allowed: &["driver.wait()"],
        ..in_src
    };
    assert_eq!(found_in(raw, allowed), Vec::<String>::new());
    let not_a_child = "fn f() { let barrier = Barrier::new(2); barrier.wait(); \
                       let mut held = StandIn::direct(Command::new(\"x\")); held.wait_within(B); \
                       let mut other = spawn_child(); other.wait().unwrap(); }";
    assert_eq!(found_in(not_a_child, in_src), Vec::<String>::new());
    let elsewhere = "fn a() { let c = Command::new(\"x\").spawn().unwrap(); }\n\
                     fn b(c: &mut Child) { c.wait().unwrap(); }";
    assert_eq!(found_in(elsewhere, in_src), Vec::<String>::new());
}

#[test]
fn a_command_built_in_a_variable_and_spawned_is_a_raw_child() {
    let in_src = Scope {
        raw_waits: true,
        ..Scope::default()
    };
    let built = "fn f() { let mut command = repo.confined_command(&script); \
                 let mut session = command.stdout(Stdio::piped()).spawn().unwrap(); \
                 let session = session.wait_with_output().unwrap(); }";
    assert_eq!(
        found_in(built, in_src),
        ["an unbounded wait on a raw child: session.wait_with_output()"]
    );
    for ending in [
        "?",
        ".expect(\"spawn\")",
        ".unwrap_or_else(|e| panic!(\"{e}\"))",
        "",
    ] {
        let spawned = format!("fn f() {{ let mut c = cmd.spawn(){ending}; c.wait(); }}");
        assert_eq!(
            found_in(&spawned, in_src),
            ["an unbounded wait on a raw child: c.wait()"]
        );
    }
    let helpers = "fn f() { let mut job = jobs.spawn(\"x\"); job.wait(); \
                   let mut held = cmd.spawn().map(StandIn::from).unwrap(); held.wait(); }";
    assert_eq!(found_in(helpers, in_src), Vec::<String>::new());
}

#[test]
fn kill_and_reap_is_a_kill_whose_result_says_the_child_ended() {
    let in_src = Scope {
        raw_kills: true,
        ..Scope::default()
    };
    let discarded = "fn f() { reap::kill_and_reap(&mut session); panic!(\"late\"); }";
    assert_eq!(
        found_in(discarded, in_src),
        ["a kill outside the helper: kill_and_reap(&mutsession)"]
    );
    let then_wait = "fn drop(&mut self) { kill_and_reap(&mut self.0); self.0.wait().ok(); }";
    assert_eq!(
        found(then_wait, false),
        ["an unchecked kill, then an unbounded wait: kill_and_reap(&mutself.0)"]
    );
    let checked = "fn f() { assert!(kill_and_reap(&mut c)); c.wait(); \
                   if !kill_and_reap(&mut d) { panic!() } d.wait(); }";
    assert_eq!(found(checked, false), Vec::<String>::new());
    let namesakes = "fn kill_and_reap(child: &mut Child) {} fn g() { x.kill_and_reap(c); }";
    assert_eq!(found(namesakes, true), Vec::<String>::new());
}

#[test]
fn every_call_allowance_still_names_a_finding() {
    let regions = test_regions(Path::new(env!("CARGO_MANIFEST_DIR")));
    for (path, call, why) in ALLOWED_CALLS {
        // A call may sit in a test file or in a `#[cfg(test)]` item of one.
        let in_file = regions
            .iter()
            .filter(|region| region.label.split(" (").next() == Some(path))
            .collect::<Vec<_>>();
        assert!(!in_file.is_empty(), "{path} ({why}) holds no test code");
        let everything = Scope {
            raw_kills: true,
            raw_waits: true,
            allowed: &[],
        };
        let findings: Vec<String> = in_file
            .iter()
            .flat_map(|region| findings(&region.code, everything))
            .collect();
        assert!(
            findings
                .iter()
                .any(|finding| finding.ends_with(&format!(": {call}"))),
            "{path} no longer has {call}; drop its allowance: {findings:?}"
        );
    }
}

#[test]
fn every_allowance_names_a_test_file_that_still_sends_a_kill() {
    let regions = test_regions(Path::new(env!("CARGO_MANIFEST_DIR")));
    for (path, why) in ALLOWED {
        let region = regions.iter().find(|region| region.label == *path);
        let region = region.unwrap_or_else(|| panic!("{path} ({why}) is not a test file"));
        assert!(
            !kills(&region.code).is_empty(),
            "{path} no longer sends a kill; drop its allowance"
        );
    }
}
