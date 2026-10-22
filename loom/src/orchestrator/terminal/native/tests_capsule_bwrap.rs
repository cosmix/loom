//! The bubblewrap probe of the emitted capsule. The capsule's `denyWrite` and
//! `denyRead` lists are translated into bubblewrap binds the way Claude Code's
//! Linux sandbox applies them, and shell probes run inside. Every refusal probe
//! has a matched control that succeeds, so a probe that never ran reads as a
//! failure, not as a deny. Skips when bubblewrap cannot start; fails instead
//! under `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`.

use super::{kinds_with_cwd, register_worktrees};
use crate::fs::permissions::state_root::{
    CODEX_AUTH_DENY_READ_PATH, CREDENTIAL_FILE_DENY_READ_PATHS,
};
use crate::models::session::SessionType;
use crate::models::stage::Implementer;
use crate::orchestrator::terminal::native::tests_session_settings::{
    checkout, read_capsule, sandbox_with, write_capsule, Checkout,
};
use crate::process::sandbox_probe::{confined_commands_runnable, skip_unless};
use serde_json::Value;
use std::path::{Path, PathBuf};
use std::process::Command;

fn skip(test: &str) -> bool {
    skip_unless(
        confined_commands_runnable(),
        test,
        "bubblewrap cannot start here",
    )
}

fn texts<'a>(capsule: &'a Value, pointer: &str) -> Vec<&'a str> {
    let array = capsule.pointer(pointer).and_then(Value::as_array);
    array
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect()
}

/// A capsule entry as a host path inside the fixture root, or `None`: globs
/// expand to one mount per match and relative entries are resolved by Claude
/// Code against its cwd, and the probe never binds a path outside the fixture.
fn resolve(entry: &str, home: &Path, root: &Path) -> Option<PathBuf> {
    if entry.contains(['*', '?', '[']) {
        return None;
    }
    let path = match entry.strip_prefix("~/") {
        Some(rest) => home.join(rest),
        None => PathBuf::from(entry),
    };
    (path.is_absolute() && path.starts_with(root)).then_some(path)
}

fn push(args: &mut Vec<String>, flag: &str, source: &str, target: &Path) {
    args.extend([flag.into(), source.into(), target.to_str().unwrap().into()]);
}

/// The bubblewrap arguments for one capsule, in Claude Code's order:
/// - `--ro-bind / /` and `--dev /dev`, then a writable bind of the fixture
///   root (the write grants for the cwd and the git common directory);
/// - each `denyWrite` entry in capsule order: an existing path is bound
///   read-only onto itself, an absent path whose parent exists gets
///   `--ro-bind /dev/null`, any other is skipped (a mount point beneath a
///   read-only bind makes bubblewrap abort, which fails the test);
/// - each `denyRead` entry that exists: a file is bound from `/dev/null` with
///   `--ro-bind`, never `--dev-bind`, so opening it fails with EACCES as it
///   does under Claude Code; a directory gets `--tmpfs`.
fn bwrap_args(capsule: &Value, checkout: &Checkout) -> Vec<String> {
    let root = checkout.repo.parent().unwrap();
    let mut args: Vec<String> = [
        "--die-with-parent",
        "--unshare-all",
        "--new-session",
        "--ro-bind",
        "/",
        "/",
        "--dev",
        "/dev",
        "--bind",
    ]
    .map(String::from)
    .into();
    args.extend([root.to_str().unwrap().into(), root.to_str().unwrap().into()]);
    for entry in texts(capsule, "/sandbox/filesystem/denyWrite") {
        let Some(path) = resolve(entry, &checkout.home, root) else {
            continue;
        };
        let parent_exists = path.parent().is_some_and(Path::exists);
        if path.exists() {
            push(&mut args, "--ro-bind", path.to_str().unwrap(), &path);
        } else if parent_exists {
            push(&mut args, "--ro-bind", "/dev/null", &path);
        }
    }
    for entry in texts(capsule, "/sandbox/filesystem/denyRead") {
        match resolve(entry, &checkout.home, root) {
            Some(path) if path.is_dir() => {
                args.extend(["--tmpfs".into(), path.to_str().unwrap().into()]);
            }
            Some(path) if path.is_file() => push(&mut args, "--ro-bind", "/dev/null", &path),
            _ => {}
        }
    }
    args
}

/// Run `script` in bubblewrap and return its probe lines. Fails unless
/// bubblewrap exited 0 and the script printed `ALIVE` first.
fn run_probe(args: &[String], script: &str, at: &str) -> Vec<String> {
    let output = Command::new("bwrap")
        .args(args)
        .args(["sh", "-c", script])
        .output()
        .unwrap();
    let stdout = String::from_utf8_lossy(&output.stdout).into_owned();
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(output.status.success(), "{at}: bwrap {args:?}: {stderr}");
    let mut lines = stdout.lines().map(str::to_owned);
    assert_eq!(lines.next().as_deref(), Some("ALIVE"), "{at}: {stderr}");
    lines.collect()
}

/// The integer after `key=` on the line starting with `name `.
fn field(lines: &[String], name: &str, key: &str) -> i64 {
    let line = lines
        .iter()
        .find(|line| line.split(' ').next() == Some(name))
        .unwrap_or_else(|| panic!("no probe line for {name}: {lines:?}"));
    let token = line
        .split(' ')
        .find_map(|part| part.strip_prefix(&format!("{key}=")));
    token.unwrap().parse().unwrap()
}

fn write_probe(name: &str, dir: &Path) -> String {
    format!(
        "touch {0}/probe >/dev/null 2>&1; echo \"{name} rc=$?\"\n",
        dir.display()
    )
}

fn read_probe(name: &str, path: &Path) -> String {
    let p = path.display();
    format!(
        "echo \"{name} rc=$(cat {p} >/dev/null 2>&1; echo $?) bytes=$(cat {p} 2>/dev/null | wc -c | tr -d ' ')\"\n"
    )
}

fn capsule_for(checkout: &Checkout, id: &str, kind: SessionType, cwd: &Path) -> Value {
    let sandbox = sandbox_with(vec![Implementer::Claude]);
    let path = write_capsule(checkout, id, kind, cwd, &sandbox, Some(&checkout.hooks_dir));
    read_capsule(&path.unwrap())
}

/// A sibling's administrative directory refuses writes, and the session's own
/// directory takes them. The worktree takes a write only for a session rooted
/// in it: a checkout-rooted knowledge session is refused it, since its capsule
/// denies `.worktrees`. The object store is the control every row shares.
#[test]
fn sibling_admin_dirs_refuse_writes_and_the_own_dir_takes_them() {
    let test = "sibling_admin_dirs_refuse_writes_and_the_own_dir_takes_them";
    if skip(test) {
        return;
    }
    let checkout = checkout();
    register_worktrees(&checkout.repo);
    let common = checkout.repo.join(".git");
    let (s1, s2) = (common.join("worktrees/s1"), common.join("worktrees/s2"));
    let mut script = String::from("echo ALIVE\n");
    for (name, dir) in [
        ("s1", &s1),
        ("s2", &s2),
        ("objects", &common.join("objects")),
        ("tree", &checkout.worktree),
    ] {
        script.push_str(&write_probe(name, dir));
    }
    let rows = kinds_with_cwd(&checkout.repo, &checkout.worktree);
    for (kind, cwd) in rows.into_iter().filter(|(kind, _)| {
        matches!(
            kind,
            SessionType::Stage | SessionType::Merge | SessionType::Knowledge
        )
    }) {
        let in_worktree = cwd == checkout.worktree;
        let capsule = capsule_for(&checkout, "session-bw1", kind, &cwd);
        let at = format!("{kind} in {}", cwd.display());
        let lines = run_probe(&bwrap_args(&capsule, &checkout), &script, &at);
        assert_ne!(field(&lines, "s2", "rc"), 0, "{at}: {lines:?}");
        assert_eq!(
            field(&lines, "s1", "rc") == 0,
            in_worktree,
            "{at}: {lines:?}"
        );
        assert_eq!(field(&lines, "objects", "rc"), 0, "{at}: {lines:?}");
        assert_eq!(
            field(&lines, "tree", "rc") == 0,
            in_worktree,
            "{at}: {lines:?}"
        );
    }
}

/// Create every credential file in the fake home holding `canary`, plus an
/// unlisted file beside them; returns the credential paths and the control.
fn plant_credentials(home: &Path) -> (Vec<PathBuf>, PathBuf) {
    let entries = CREDENTIAL_FILE_DENY_READ_PATHS
        .iter()
        .chain([&CODEX_AUTH_DENY_READ_PATH]);
    let paths: Vec<PathBuf> = entries
        .map(|entry| home.join(entry.strip_prefix("~/").unwrap()))
        .collect();
    let control = home.join(".not-a-credential");
    for path in paths.iter().chain([&control]) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, "canary").unwrap();
    }
    (paths, control)
}

fn read_script(paths: &[PathBuf], control: &Path) -> String {
    let mut script = String::from("echo ALIVE\n");
    for (index, path) in paths.iter().enumerate() {
        script.push_str(&read_probe(&format!("c{index}"), path));
    }
    script.push_str(&read_probe("control", control));
    script
}

/// Run the read probes in a stage capsule licensed for `lanes`.
fn stage_reads(
    checkout: &Checkout,
    lanes: Vec<Implementer>,
    script: &str,
    at: &str,
) -> Vec<String> {
    let sandbox = sandbox_with(lanes);
    let hooks = Some(checkout.hooks_dir.as_path());
    let path = write_capsule(
        checkout,
        "session-bw2",
        SessionType::Stage,
        &checkout.worktree,
        &sandbox,
        hooks,
    );
    run_probe(
        &bwrap_args(&read_capsule(&path.unwrap()), checkout),
        script,
        at,
    )
}

/// Every credential file refuses reads (a non-zero `rc`, no bytes) while the
/// unlisted file beside them reads its 6 bytes; a codex-licensed stage reads
/// the codex credential.
#[test]
fn credential_files_refuse_reads_and_an_unlisted_file_does_not() {
    let test = "credential_files_refuse_reads_and_an_unlisted_file_does_not";
    if skip(test) {
        return;
    }
    let checkout = checkout();
    register_worktrees(&checkout.repo);
    let (paths, control) = plant_credentials(&checkout.home);
    let script = read_script(&paths, &control);

    let claude_only = format!("{test} claude");
    let lines = stage_reads(&checkout, vec![Implementer::Claude], &script, &claude_only);
    for (index, path) in paths.iter().enumerate() {
        let name = format!("c{index}");
        assert_ne!(
            field(&lines, &name, "rc"),
            0,
            "{}: {lines:?}",
            path.display()
        );
        assert_eq!(
            field(&lines, &name, "bytes"),
            0,
            "{}: {lines:?}",
            path.display()
        );
    }
    assert_eq!(field(&lines, "control", "rc"), 0, "{lines:?}");
    assert_eq!(field(&lines, "control", "bytes"), 6, "{lines:?}");

    let with_codex = format!("{test} claude+codex");
    let lines = stage_reads(
        &checkout,
        vec![Implementer::Claude, Implementer::Codex],
        &script,
        &with_codex,
    );
    let codex = format!("c{}", paths.len() - 1);
    assert_eq!(field(&lines, &codex, "rc"), 0, "{lines:?}");
    assert_eq!(field(&lines, &codex, "bytes"), 6, "{lines:?}");
}
