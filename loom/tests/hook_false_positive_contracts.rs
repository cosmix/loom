//! Contracts for stage hook-guards of PLAN-hook-false-positives: each guard keeps its block on
//! the hostile form and stops blocking the sound one, and every block lands in the stage's block
//! ledger, which `loom hook blocks` reads. They run copies of the real hook scripts; see
//! `hook_false_positive_contracts/support.rs` for the fixture.

#[path = "test_support/mod.rs"]
mod test_support;

#[path = "hook_false_positive_contracts/support.rs"]
mod support;

#[path = "hook_false_positive_contracts/ledger.rs"]
mod ledger;

use ledger::{assert_block_recorded, assert_report_line, blocks_report, BlockCase};
use serde_json::{json, Value};
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use support::{bash_payload, text, Agent, Fixture, Run};

const SUBAGENT: Agent<'static> = Agent::Subagent("loom-software-engineer");
const NO_OUTLINE: &str = "no outline is available";

/// One completion attempt through loom-control-complete.sh in test mode, from the worktree, with
/// the model's `timeout` of 300000 in the tool input.
fn completion(fx: &Fixture, agent: Agent, command: &str) -> Run {
    let payload = json!({
        "tool_name": "Bash",
        "tool_input": {"command": command, "timeout": 300000},
        "cwd": fx.wt,
    });
    let env = [
        ("LOOM_STAGE_ID", fx.stage.clone()),
        ("LOOM_SESSION_ID", "session-123".to_string()),
        ("LOOM_WORKTREE_PATH", text(&fx.wt)),
        ("LOOM_WORK_DIR", text(&fx.repo.join(".loom/work"))),
        ("LOOM_CONTROL_TESTING", "1".to_string()),
        ("LOOM_CONTROL_TEST_BIN", text(&fx.loom_bin())),
    ];
    fx.run("loom-control-complete.sh", agent, payload, &fx.wt, &env)
}

#[track_caller]
fn assert_rewritten(run: &Run, pinned: &str, what: &str) {
    run.assert_code(0, what);
    let out = run.hook_output(what);
    assert_eq!(out["hookEventName"], "PreToolUse", "{what}: {out}");
    assert_eq!(out["permissionDecision"], "allow", "{what}: {out}");
    assert_eq!(out["updatedInput"]["command"], pinned, "{what}: {out}");
    assert_eq!(out["updatedInput"]["timeout"], 300000, "{what}: {out}");
}

#[test]
fn completion_bare_command_is_rewritten_to_pinned() {
    let fx = Fixture::new("build-api");
    let pinned = format!("{} stage complete build-api", fx.loom_bin().display());

    let main = completion(&fx, Agent::Plain, "loom stage complete build-api");
    assert_rewritten(&main, &pinned, "bare completion from the main agent");

    let sub = completion(&fx, SUBAGENT, "loom stage complete build-api");
    sub.assert_code(2, "bare completion from a subagent");
    assert!(
        !sub.stdout.contains("updatedInput"),
        "a subagent's completion was rewritten: {}",
        sub.describe()
    );
}

#[test]
fn completion_tail_suffix_is_rewritten_to_pinned() {
    let fx = Fixture::new("build-api");
    let pinned = format!("{} stage complete build-api", fx.loom_bin().display());

    let tail = completion(
        &fx,
        Agent::Plain,
        "loom stage complete build-api 2>&1 | tail -30",
    );
    assert_rewritten(&tail, &pinned, "completion with a 2>&1 | tail -30 suffix");

    let piped = completion(
        &fx,
        Agent::Plain,
        "loom stage complete build-api | tail -30 | sh",
    );
    piped.assert_code(2, "completion piped through tail and then sh");
    assert!(
        piped.stderr.contains("LOOM_CONTROL_ERROR"),
        "the second pipe was not rejected with a reason: {}",
        piped.describe()
    );
    assert!(
        !piped.stdout.contains("updatedInput"),
        "a second pipe was rewritten: {}",
        piped.describe()
    );
}

fn write_file(path: &Path, body: &str) {
    fs::create_dir_all(path.parent().expect("file has a parent")).expect("create parents");
    fs::write(path, body).expect("write fixture file");
}

#[test]
fn traversal_resolving_inside_worktree_is_allowed() {
    let fx = Fixture::new("st");
    let cwd = fx.wt.join("web/src");
    fs::create_dir_all(&cwd).expect("create web/src");
    write_file(&fx.wt.join("doc/x.md"), "inside\n");
    write_file(&fx.repo.join("doc/x.md"), "main checkout\n");
    let isolation = |command: &str| {
        let payload = bash_payload(command, &cwd);
        fx.run("worktree-isolation.sh", Agent::Plain, payload, &cwd, &[])
    };

    isolation("sed -n 1p ../../doc/x.md")
        .assert_code(0, "../../doc/x.md resolves to <wt>/doc/x.md");
    isolation("sed -n 1p ../../../../doc/x.md")
        .assert_code(2, "../../../../doc/x.md resolves to <repo>/doc/x.md");
}

/// `<projects>/<session>/tool-results/<name>`, a regular file.
fn results_file(projects: &Path, session: &str, name: &str) -> PathBuf {
    let path = projects.join(session).join("tool-results").join(name);
    write_file(&path, "tool output\n");
    path
}

#[test]
fn own_tool_results_read_is_allowed() {
    let fx = Fixture::new("st");
    let projects = fx.home.join(".claude/projects/p");
    let own = results_file(&projects, "S", "b1.txt");
    let other = results_file(&projects, "T", "b1.txt");
    let link = projects.join("S/tool-results/link.txt");
    symlink("/etc/hostname", &link).expect("link a tool-results entry");
    let read = |path: &Path| {
        let payload = json!({
            "tool_name": "Read",
            "tool_input": {"file_path": path},
            "session_id": "S",
            "cwd": fx.wt,
        });
        fx.run("worktree-file-guard.sh", Agent::Plain, payload, &fx.wt, &[])
    };

    read(&own).assert_code(0, "Read of this session's own tool-results file");
    read(&other).assert_code(2, "Read of another session's tool-results file");
    read(&link).assert_code(2, "Read of a tool-results entry symlinked to /etc/hostname");
}

#[test]
fn git_c_into_scratch_clone_is_allowed() {
    let fx = Fixture::new("st");
    let scratch = fx.scratch();
    let clone = scratch.join("clone");
    fs::create_dir_all(&clone).expect("create clone");
    test_support::git_checked(&clone, &["init", "-q"]);
    let env = [("LOOM_SCRATCH_DIR", text(&scratch))];
    let isolation = |command: &str| {
        let payload = bash_payload(command, &fx.wt);
        fx.run("worktree-isolation.sh", Agent::Plain, payload, &fx.wt, &env)
    };

    let literal = format!("git -C {} status", clone.display());
    isolation(&literal).assert_code(0, "git -C into the scratch clone, written literally");
    isolation("git -C \"$LOOM_SCRATCH_DIR/clone\" status")
        .assert_code(0, "git -C into the scratch clone through $LOOM_SCRATCH_DIR");

    let main = format!("git -C {} status", fx.repo.display());
    isolation(&main).assert_code(2, "git -C into the main checkout");
    let state = format!("git -C {}/.loom/work log", fx.wt.display());
    isolation(&state).assert_code(2, "git -C through the worktree's .loom/work symlink");
}

#[test]
fn subagent_cargo_check_is_allowed() {
    let fx = Fixture::new("st");
    let env = [
        ("LOOM_STAGE_ID", fx.stage.clone()),
        ("LOOM_SESSION_ID", "x".to_string()),
        ("LOOM_WORK_DIR", text(&fx.work_dir())),
    ];
    let guard = |command: &str| {
        let payload = bash_payload(command, &fx.wt);
        fx.run("subagent-verify-guard.sh", SUBAGENT, payload, &fx.wt, &env)
    };

    guard("cargo check --lib --tests").assert_code(0, "subagent cargo check --lib --tests");
    guard("cargo fmt -- src/a.rs").assert_code(2, "subagent cargo fmt -- src/a.rs");
    guard("cargo clippy").assert_code(2, "subagent cargo clippy");
}

#[test]
fn scratch_repo_commit_is_allowed_for_subagent() {
    let fx = Fixture::new("s");
    let scratch = fx.scratch();
    write_file(&scratch.join("fx/doc/a.md"), "a\n");
    let gitdir = fx.repo.join(".git/worktrees/s");
    assert!(
        gitdir.is_dir(),
        "fixture worktree git dir {}",
        gitdir.display()
    );
    write_file(
        &scratch.join("gf/.git"),
        &format!("gitdir: {}\n", gitdir.display()),
    );
    let env = [
        ("LOOM_STAGE_ID", "s".to_string()),
        ("LOOM_SESSION_ID", "x".to_string()),
        ("LOOM_SCRATCH_DIR", text(&scratch)),
    ];
    let filter = |command: &str| {
        let payload = bash_payload(command, &fx.wt);
        fx.run("commit-filter.sh", SUBAGENT, payload, &fx.wt, &env)
    };
    let s = scratch.display();

    let allowed = format!("cd {s}/fx && git init -q && git add doc && git commit -qm x");
    filter(&allowed).assert_code(0, &allowed);
    for blocked in [
        format!("cd {} && git commit -qm x", fx.wt.display()),
        format!("(cd {s}/fx && :); git commit -qm x"),
        format!("cd {s}/gf && git commit -qm x"),
        format!("cd {s}/fx && loom stage complete s"),
    ] {
        filter(&blocked).assert_code(2, &blocked);
    }
}

/// A 600-line file read whole through read-guard.sh in a live session: `LOOM_STAGE_ID` set,
/// `LOOM_MAIN_AGENT_PID` this test process, `LOOM_BIN` missing and no `config.toml`.
fn whole_read(fx: &Fixture, work: &Path, file: &Path) -> Run {
    let payload = json!({
        "tool_name": "Read",
        "tool_input": {"file_path": file},
        "agent_id": "agent-a",
        "session_id": "sess-a",
        "cwd": fx.root,
    });
    let env = [
        ("LOOM_STAGE_ID", "stage-1".to_string()),
        ("LOOM_MAIN_AGENT_PID", std::process::id().to_string()),
        ("LOOM_SESSION_ID", "sess-a".to_string()),
        ("LOOM_WORK_DIR", text(work)),
        ("LOOM_HOME", text(&fx.root.join("loom-home"))),
        ("LOOM_BIN", text(&fx.root.join("no-such-loom"))),
    ];
    fx.run("read-guard.sh", Agent::Plain, payload, &fx.root, &env)
}

#[track_caller]
fn assert_no_outline_warning(run: &Run, file: &Path, what: &str) {
    run.assert_code(0, what);
    let context = run.hook_output(what)["additionalContext"]
        .as_str()
        .unwrap_or_default()
        .to_string();
    assert!(
        context.contains(NO_OUTLINE) && context.contains(&*file.to_string_lossy()),
        "{what}: additionalContext lacks the no-outline guidance: {}",
        run.describe()
    );
}

#[test]
fn first_large_unoutlined_read_warns_instead_of_blocking() {
    let fx = Fixture::new("st");
    let work = fx.root.join("work");
    fs::create_dir_all(&work).expect("create work dir");
    let home = fx.root.join("loom-home");
    write_file(&home.join("config.toml"), "[update]\ncheck = false\n");
    let file = fx.root.join("files/big.txt");
    write_file(&file, &"line\n".repeat(600));

    let first = whole_read(&fx, &work, &file);
    assert_no_outline_warning(&first, &file, "first whole read of an unoutlined file");
    let repeat = whole_read(&fx, &work, &file);
    assert_no_outline_warning(&repeat, &file, "identical repeat of that read");
}

/// (guard, rule, input): one blocked call per guard. The input is the Bash command, except for
/// worktree-file-guard, where it is the path of a Read.
const LEDGER_CASES: [(&str, &str, &str); 6] = [
    (
        "worktree-isolation",
        "path-traversal",
        "cat ../../../outside",
    ),
    ("worktree-file-guard", "outside-worktree", "/etc/hostname"),
    ("subagent-verify-guard", "subagent-verify", "cargo clippy"),
    ("commit-filter", "no-git-commit", "git commit -qm x"),
    ("git-add-guard", "dangerous-add", "git add -A"),
    (
        "loom-control-complete",
        "not-pinned",
        "loom stage complete s; true",
    ),
];

/// [`LEDGER_CASES`] as calls from the worktree of stage `s`: subagent-verify-guard runs as a
/// subagent, every other guard as the main agent.
fn ledger_cases(fx: &Fixture) -> Vec<BlockCase<'static>> {
    let wt = &fx.wt;
    let completion_env = vec![
        ("LOOM_WORKTREE_PATH", text(wt)),
        ("LOOM_CONTROL_TESTING", "1".to_string()),
        ("LOOM_CONTROL_TEST_BIN", text(&fx.loom_bin())),
    ];
    let to_case = |(guard, rule, input): (&'static str, &'static str, &str)| {
        let payload: Value = match guard {
            "worktree-file-guard" => json!({"tool_name": "Read",
                "tool_input": {"file_path": input}, "session_id": ledger::SESSION, "cwd": wt}),
            _ => bash_payload(input, wt),
        };
        let agent = match guard {
            "subagent-verify-guard" => SUBAGENT,
            _ => Agent::Plain,
        };
        let env = match guard {
            "loom-control-complete" => completion_env.clone(),
            _ => Vec::new(),
        };
        BlockCase {
            guard,
            rule,
            agent,
            payload,
            env,
        }
    };
    LEDGER_CASES.into_iter().map(to_case).collect()
}

#[test]
fn every_guard_block_is_recorded_in_stage_ledger() {
    let fx = Fixture::new(ledger::STAGE);
    let work = fx.work_dir();
    let ledger_path = work.join("heartbeat/s.blocks.jsonl");
    for mut case in ledger_cases(&fx) {
        case.env.extend([
            ("LOOM_WORK_DIR", text(&work)),
            ("LOOM_STAGE_ID", ledger::STAGE.to_string()),
            ("LOOM_SESSION_ID", ledger::SESSION.to_string()),
        ]);
        assert_block_recorded(&fx, case, &ledger_path);
    }

    let report = blocks_report(&fx, &work);
    assert_report_line(
        &report,
        "1 worktree-isolation/path-traversal main=1 subagent=0",
    );
    assert_report_line(
        &report,
        "1 subagent-verify-guard/subagent-verify main=0 subagent=1",
    );
}
