//! The eval command's accuracy bar and report lines.

use super::*;

#[test]
#[serial]
fn eval_bar_is_ninety_percent() {
    let scene = Scene::new();
    scene.write("doc.md", "text\n");
    scene.serve_claude();
    write_cases(&scene, "cases.json", &cases_satisfying(10, 9), "doc.md");
    let pass = scene.eval("cases.json");
    pass.assert_exit(0);
    pass.assert_stdout_has("accuracy: 0.90");
    write_cases(&scene, "cases.json", &cases_satisfying(10, 8), "doc.md");
    let fail = scene.eval("cases.json");
    fail.assert_exit(1);
    fail.assert_stdout_has("accuracy: 0.80");
}

#[test]
#[serial]
fn eval_counts_forbidden_strings() {
    let scene = Scene::new();
    scene.write("doc.md", "text\n");
    scene.serve_claude();
    let rows = [
        row("forbidden", &["ok"], &["OK"]),
        row("clean", &["OK"], &["absent"]),
    ];
    write_cases(&scene, "cases.json", &rows, "doc.md");
    let done = scene.eval("cases.json");
    done.assert_exit(1);
    done.assert_stdout_has("accuracy: 0.50");
}

#[test]
#[serial]
fn eval_report_carries_token_and_latency_lines() {
    let scene = Scene::new();
    scene.write("doc.md", "é".repeat(4000));
    scene.serve_claude();
    write_cases(&scene, "cases.json", &[row("one", &["ok"], &[])], "doc.md");
    let done = scene.eval("cases.json");
    done.assert_exit(0);
    let lines: Vec<&str> = done.stdout.lines().map(str::trim).collect();
    assert!(lines.contains(&CLAUDE_TOKENS), "{}", done.stdout);
    assert!(
        lines.contains(&"direct tokens estimate: 1000"),
        "{}",
        done.stdout
    );
    let latency = lines.iter().find(|line| line.starts_with("latency p50:"));
    assert!(
        latency.is_some_and(|line| line.contains("p90:")),
        "{}",
        done.stdout
    );
}
