use super::*;

fn routing_workspace() -> TempDir {
    let workspace = setup_test_workspace();
    let stages = workspace.path().join(".loom/work/stages");
    fs::write(
        stages.join("01-core-architecture.md"),
        "---\nstatus: executing\n---\n",
    )
    .unwrap();
    fs::write(
        stages.join("02-math-core.md"),
        "---\nstatus: blocked\n---\n",
    )
    .unwrap();
    fs::write(
        stages.join("02-ui-framework.md"),
        "---\nstatus: merge-conflict\n---\n",
    )
    .unwrap();
    workspace
}

#[test]
fn routed_candidate_domains() {
    let workspace = routing_workspace();
    let root = workspace.path();
    type Case = (
        &'static str,
        &'static str,
        &'static str,
        &'static [&'static str],
        &'static [&'static str],
    );
    #[rustfmt::skip]
    let cases: &[Case] = &[
        ("init", "PLAN", "init", &["init"], &[]),
        ("check", "core", "check", &["check"], &["core-architecture"]),
        ("sessions kill", "session-00", "kill", &["sessions", "kill"], &["session-001", "session-002"]),
        ("stage complete", "", "complete", &["stage", "complete"], &["core-architecture"]),
        ("stage retry", "", "retry", &["stage", "retry"], &["math-core"]),
        ("stage merge", "", "merge", &["stage", "merge"], &["ui-framework"]),
        ("stage hold", "core", "hold", &["stage", "hold"], &["core-architecture"]),
        ("stage release", "math", "release", &["stage", "release"], &["math-core"]),
        ("stage skip", "ui", "skip", &["stage", "skip"], &["ui-framework"]),
        ("stage resume", "core", "resume", &["stage", "resume"], &["core-architecture"]),
        ("output set", "core", "set", &["stage", "output", "set"], &["core-architecture"]),
        ("output get", "math", "get", &["stage", "output", "get"], &["math-core"]),
        ("output list", "ui", "list", &["stage", "output", "list"], &["ui-framework"]),
        ("output remove", "ui", "remove", &["stage", "output", "remove"], &["ui-framework"]),
        ("worktree remove", "core", "remove", &["worktree", "remove"], &["core-architecture"]),
        ("stage flag", "core", "--stage", &["sessions", "kill"], &["core-architecture"]),
        ("memory stage flag", "core", "--stage", &["memory", "note"], &["core-architecture"]),
        ("entry type flag", "de", "--entry-type", &["memory", "list"], &["decision"]),
        ("backend flag", "t", "--backend", &["run"], &["tmux"]),
        ("effort flag", "xh", "--claude-effort", &["pressure"], &["xhigh"]),
        ("knowledge update", "pa", "update", &["knowledge", "update"], &["patterns"]),
        ("shell", "", "completions", &["completions"], &["bash", "fish", "zsh"]),
        ("flag", "--trigger", "handoff", &["handoff"], &["--trigger"]),
        ("trigger", "", "--trigger", &["handoff"], &["manual", "precompact", "session_end"]),
        ("top level", "init", "loom", &[], &["init"]),
        ("stage subcommand", "hold", "stage", &["stage"], &["hold"]),
        ("unknown", "", "status", &["status"], &[]),
    ];
    for (name, prefix, previous, path, expected) in cases {
        let cmdline = format!("loom {}", path.join(" "));
        let actual = route_completion(root, prefix, previous, path, &cmdline).unwrap();
        assert_eq!(
            actual,
            expected.iter().map(|s| s.to_string()).collect::<Vec<_>>(),
            "routing case: {name}"
        );
    }
}
