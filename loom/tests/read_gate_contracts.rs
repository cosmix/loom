//! Frozen contracts for the deny-once read gate and the Markdown outline. They run the real hook
//! scripts and the real binary. Rule 1 warns with exit 0 before the gate exists, so each contract
//! about an allowed read also asserts a denial: a contract of exit 0 alone would already pass.

#[path = "integration/helpers.rs"]
#[allow(dead_code)]
mod helpers;

#[path = "read_gate_contracts/support.rs"]
mod support;

use std::fs;
use std::os::unix::fs::symlink;
use support::{big_file, map_outline, outline_rows, Gate, Liveness};
use tempfile::TempDir;

const AGENT: &str = "agent-a";

fn files_dir() -> TempDir {
    TempDir::new().expect("files dir")
}

#[test]
fn first_whole_read_denied_then_repeat_allowed() {
    let gate = Gate::live();
    let files = files_dir();
    let file = big_file(files.path(), "big.txt");

    gate.read(AGENT, &file).assert_denied("first whole read");
    gate.read(AGENT, &file)
        .assert_silently_allowed("repeat of a denied read");
}

#[test]
fn outside_a_live_stage_only_warns() {
    let mut gate = Gate::live();
    let files = files_dir();
    let live = big_file(files.path(), "live.txt");
    let dead = big_file(files.path(), "dead.txt");
    let plain = big_file(files.path(), "plain.txt");

    gate.read(AGENT, &live).assert_denied("live main agent");

    gate.liveness = Liveness::DeadMainAgent;
    gate.read(AGENT, &dead)
        .assert_warned(&dead, "stage id with a dead main agent");

    gate.liveness = Liveness::NoStage;
    gate.read(AGENT, &plain)
        .assert_warned(&plain, "no stage id and no main agent");
}

#[test]
fn unrecordable_denial_fails_open() {
    let mut gate = Gate::live();
    let files = files_dir();
    let file = big_file(files.path(), "big.txt");

    gate.session = "sess-a";
    gate.read(AGENT, &file).assert_denied("recordable denial");

    let outside = TempDir::new().expect("symlink target");
    let root = gate.work_dir().join("hooks/read-denials");
    fs::create_dir_all(&root).expect("denials root");
    symlink(outside.path(), root.join("sess-b")).expect("symlink the session ledger directory");

    gate.session = "sess-b";
    gate.read(AGENT, &file)
        .assert_warned(&file, "first read with an unrecordable denial");
    gate.read(AGENT, &file)
        .assert_warned(&file, "second read with an unrecordable denial");

    let written: Vec<_> = fs::read_dir(outside.path())
        .expect("list symlink target")
        .collect();
    assert!(
        written.is_empty(),
        "written through the symlink: {written:?}"
    );
}

#[test]
fn control_inputs_are_exempt() {
    let gate = Gate::live();
    let root = files_dir();
    for relative in [
        ".loom/work/signals/session-1.md",
        "doc/plans/briefs/worker.md",
        "doc/plans/feature/briefs/worker.md",
        "doc/plans/PLAN-x.md",
    ] {
        let file = big_file(root.path(), relative);
        gate.read(AGENT, &file).assert_silently_allowed(relative);
    }

    let ordinary = big_file(root.path(), "doc/guides/ordinary.md");
    gate.read(AGENT, &ordinary)
        .assert_denied("a file outside the exempt directories");
}

#[test]
fn read_gate_false_only_warns() {
    let gate = Gate::live();
    let files = files_dir();
    let file = big_file(files.path(), "big.txt");

    gate.write_config("[hooks]\nread_gate = false\n");
    gate.read(AGENT, &file)
        .assert_warned(&file, "read_gate = false");

    gate.write_config("[hooks]\nread_gate = true\n");
    gate.read(AGENT, &file).assert_denied("read_gate = true");
}

#[test]
fn fenced_headings_are_not_outlined() {
    let repo = helpers::init_test_repo();
    let body = concat!(
        "# Real heading\n\nprose\n\n",
        "```bash\n# comment\n```\n\n",
        "~~~\n## also code\n~~~\n\n",
        "## Tinted \u{1b}[31mred\u{1b}[0m words\n\nprose\n",
    );
    fs::write(repo.path().join("notes.md"), body).expect("write notes.md");

    let out = map_outline(repo.path(), "notes.md");
    let rows = outline_rows(&out.stdout);

    assert!(
        rows.iter().any(|row| row.contains("Real heading")),
        "no row for the real heading: exit {} stdout: {} stderr: {}",
        out.code,
        out.stdout,
        out.stderr
    );
    assert!(
        rows.iter().any(|row| row.contains("Tinted")),
        "no row for the heading with an escape: {rows:?}"
    );
    assert!(
        !rows
            .iter()
            .any(|row| row.contains("comment") || row.contains("also code")),
        "a fenced line became a row: {rows:?}"
    );
    assert!(
        !out.stdout.contains('\u{1b}'),
        "stdout holds an ESC byte: {:?}",
        out.stdout
    );
}

#[test]
fn symlinked_markdown_gets_no_rows() {
    let repo = helpers::init_test_repo();
    fs::write(repo.path().join("real.md"), "# Real heading\n\nprose\n").expect("write real.md");
    let real = map_outline(repo.path(), "real.md");
    assert!(
        outline_rows(&real.stdout)
            .iter()
            .any(|row| row.contains("Real heading")),
        "no row for real.md: exit {} stdout: {} stderr: {}",
        real.code,
        real.stdout,
        real.stderr
    );

    let outside = TempDir::new().expect("outside dir");
    let target = outside.path().join("target.md");
    fs::write(&target, "# Outside secret\n\nprose\n").expect("write outside file");
    symlink(&target, repo.path().join("link.md")).expect("symlink link.md");

    let linked = map_outline(repo.path(), "link.md");
    assert!(
        linked.stdout.contains("Outline:"),
        "no header: {}",
        linked.stdout
    );
    assert!(
        linked.stdout.contains("coverage: markdown unavailable"),
        "no unavailable line: {}",
        linked.stdout
    );
    assert!(
        outline_rows(&linked.stdout).is_empty(),
        "rows through a symlink: {}",
        linked.stdout
    );
    assert!(
        !linked.stdout.contains("Outside secret"),
        "outside heading text printed: {}",
        linked.stdout
    );
}

#[test]
fn markdown_outline_reaches_the_denial() {
    let repo = helpers::init_test_repo();
    let body = format!(
        "# Alpha section\n{}## Beta section\n{}",
        "line\n".repeat(248),
        "line\n".repeat(250)
    );
    let file = repo.path().join("guide.md");
    fs::write(&file, body).expect("write guide.md");

    let mut gate = Gate::live();
    gate.cwd = Some(repo.path().to_path_buf());
    gate.loom_bin = Some(helpers::loom_bin_path());

    let out = gate.read(AGENT, &file);
    out.assert_denied("whole read of a large Markdown file");
    for (row_start, title) in [("  L1-L", "Alpha section"), ("  L250-L", "Beta section")] {
        assert!(
            out.stderr
                .lines()
                .any(|line| line.contains(row_start) && line.contains(title)),
            "no {title} row in the denial: {}",
            out.stderr
        );
    }
}

#[test]
fn second_agent_gets_its_own_denial() {
    let gate = Gate::live();
    let files = files_dir();
    let file = big_file(files.path(), "big.txt");

    gate.read("agent-a", &file).assert_denied("agent-a first");
    gate.read("agent-a", &file).assert_allowed("agent-a repeat");
    gate.read("agent-b", &file).assert_denied("agent-b first");
}

#[test]
fn bash_cat_and_read_share_one_denial() {
    let root = files_dir();
    let cwd = root.path().join("cwd");
    fs::create_dir_all(&cwd).expect("create cwd");
    let big = big_file(root.path(), "files/big.md");
    let other = big_file(root.path(), "files/other.md");

    let mut gate = Gate::live();
    gate.cwd = Some(cwd);

    gate.bash(AGENT, "cat ../files/big.md")
        .assert_denied("cat first");
    gate.read(AGENT, &big).assert_allowed("Read after cat");

    gate.read(AGENT, &other).assert_denied("Read first");
    gate.bash(AGENT, "cat ../files/other.md")
        .assert_allowed("cat after Read");
}

#[test]
fn denial_ledger_at_capacity_warns() {
    let gate = Gate::live();
    let files = files_dir();
    let paths: Vec<_> = (0..301)
        .map(|index| big_file(files.path(), &format!("file-{index}.txt")))
        .collect();

    for (index, path) in paths[..300].iter().enumerate() {
        gate.read(AGENT, path)
            .assert_denied(&format!("first read {index}"));
    }
    let last = &paths[300];
    gate.read(AGENT, last)
        .assert_warned(last, "first read with a full ledger");

    gate.read(AGENT, &paths[0])
        .assert_silently_allowed("repeat of the oldest denial");
    gate.read(AGENT, last)
        .assert_allowed("repeat of the read the full ledger waved through");
}
