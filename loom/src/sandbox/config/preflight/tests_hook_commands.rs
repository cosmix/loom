//! Hook command preflight cases over injected registrations and temporary files.

use super::hook_commands::HookDocument;
use super::*;
use serde_json::{json, Map};
use std::os::unix::fs::{symlink, PermissionsExt};
use tempfile::TempDir;

#[path = "tests_hook_arguments.rs"]
mod arguments;

#[path = "tests_hook_registrations.rs"]
mod registrations;

fn executable(path: &Path) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, "#!/bin/sh\n").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755)).unwrap();
}

/// A repository and a writable root (both session-writable), a directory
/// outside every root, and a PATH directory, all in one temp dir.
struct Scene {
    temp: TempDir,
    repo: PathBuf,
    writable: PathBuf,
    outside: PathBuf,
    bin: PathBuf,
}

fn scene() -> Scene {
    let temp = TempDir::new().unwrap();
    let [repo, writable, outside, bin] =
        ["repo", "writable", "outside", "bin"].map(|name| temp.path().join(name));
    for dir in [&repo, &writable, &outside, &bin] {
        std::fs::create_dir_all(dir).unwrap();
    }
    Scene {
        temp,
        repo,
        writable,
        outside,
        bin,
    }
}

fn document(event: &str, command: &str) -> HookDocument {
    let entry = json!([{ "matcher": "*", "hooks": [{ "type": "command", "command": command }] }]);
    let mut hooks = Map::new();
    hooks.insert(event.to_string(), entry);
    HookDocument {
        label: "settings.json".to_string(),
        settings: json!({ "hooks": hooks }),
    }
}

fn scan(scene: &Scene, documents: &[HookDocument]) -> HookFindings {
    scan_hook_registrations(&HookScan {
        documents,
        repo_root: &scene.repo,
        home: Some(scene.temp.path()),
        path: &[
            scene.bin.clone(),
            PathBuf::from("/bin"),
            PathBuf::from("/usr/bin"),
        ],
        writable_roots: &[scene.repo.clone(), scene.writable.clone()],
    })
}

#[test]
fn check_5_refuses_a_user_hook_that_runs_a_file_under_a_writable_root() {
    let scene = scene();
    for dir in [&scene.writable, &scene.outside] {
        executable(&dir.join("notify.sh"));
        executable(&dir.join("status"));
    }
    symlink(scene.writable.join("status"), scene.bin.join("status")).unwrap();
    let (writable, outside) = (scene.writable.display(), scene.outside.display());

    for command in [
        format!("{writable}/notify.sh"),
        format!("FOO=1 bash -x {writable}/notify.sh"),
        "status --line".to_string(),
        "$CLAUDE_PROJECT_DIR/.claude/hooks/guard.sh".to_string(),
    ] {
        let findings = scan(&scene, &[document("Stop", &command)]);
        assert_eq!(findings.refusals.len(), 1, "{command}: {findings:?}");
        let refusal = &findings.refusals[0];
        assert!(
            refusal.contains("under the session-writable root"),
            "{refusal}"
        );
    }

    for command in [
        format!("{outside}/notify.sh"),
        format!("bash -c 'echo {writable}/notify.sh'"),
    ] {
        let findings = scan(&scene, &[document("Stop", &command)]);
        assert_eq!(findings, HookFindings::default(), "{command}");
    }
}

#[test]
fn hook_commands_handle_quoted_paths_wrappers_and_lists() {
    let scene = scene();
    let script = scene.writable.join("hook script.sh");
    executable(&script);
    let path = script.display();
    for command in [
        format!("\"{path}\""),
        format!("/bin/bash \"{path}\""),
        format!("/bin/bash {}", path.to_string().replace(' ', "\\ ")),
        format!("FOO='some value' env BAR='other value' /bin/bash \"{path}\""),
        format!("/bin/true; /bin/bash \"{path}\""),
    ] {
        let findings = scan(&scene, &[document("Stop", &command)]);
        assert!(
            findings
                .refusals
                .iter()
                .any(|r| r.contains("session-writable root")),
            "{command}: {findings:?}"
        );
    }
    for command in [
        "not-on-path --flag",
        "env PATH=/tmp tool",
        "echo $(tool)",
        "bash -o option script",
        "/bin/true > output",
    ] {
        let findings = scan(&scene, &[document("Stop", command)]);
        assert!(
            findings
                .refusals
                .iter()
                .any(|r| r.contains("cannot be verified")),
            "{command}: {findings:?}"
        );
    }
}

/// `scene` with every program the wrapper cases name on its PATH, so each case
/// depends on the grammar under test and not on what this host installs.
fn scene_with_programs() -> Scene {
    let scene = scene();
    for name in [
        "timeout",
        "nohup",
        "nice",
        "stdbuf",
        "setsid",
        "flock",
        "xargs",
        "env",
        "uv",
        "npx",
        "busybox",
        "watch",
        "find",
        "make",
        "python3.12",
        "python3",
        "node20",
        "perl5.36",
        "bash-5.2",
        "pypy3",
        "bash",
        "sudo",
        "ruby",
    ] {
        executable(&scene.bin.join(name));
    }
    scene
}

fn refusal_text(scene: &Scene, command: &str) -> String {
    scan(scene, &[document("Stop", command)])
        .refusals
        .join("\n")
}

#[test]
fn exec_wrappers_resolve_to_the_program_they_run() {
    let scene = scene_with_programs();
    let script = scene.writable.join("notify.sh");
    let clean = scene.outside.join("notify.sh");
    executable(&script);
    executable(&clean);
    let cases = [
        "timeout 5 {}",
        "timeout -k 1 5 {}",
        "timeout --signal=TERM --preserve-status 5 {}",
        "nohup {}",
        "nice {}",
        "nice -n 5 {}",
        "nice --adjustment=5 {}",
        "stdbuf -oL {}",
        "stdbuf -o L -e0 {}",
        "setsid -w {}",
        "flock /tmp/lock {}",
        "flock -n -w 5 /tmp/lock {}",
        "xargs -0 {}",
        "xargs -n 1 -r {}",
        "env {}",
        "env -i A=1 {}",
        "/usr/bin/env A=1 timeout 5 nohup {}",
        "exec timeout 5 {}",
        "A=1 timeout 5 python3.12 {}",
        "timeout 5 bash -x {}",
    ];
    for case in cases {
        let refused = refusal_text(&scene, &case.replace("{}", &script.display().to_string()));
        assert!(
            refused.contains("session-writable root"),
            "{case}: {refused}"
        );
        let command = case.replace("{}", &clean.display().to_string());
        assert_eq!(refusal_text(&scene, &command), "", "{command}");
    }
}

#[test]
fn versioned_interpreters_resolve_to_their_script() {
    let scene = scene_with_programs();
    let script = scene.writable.join("hook.py");
    let clean = scene.outside.join("hook.py");
    executable(&script);
    executable(&clean);
    for program in ["python3.12", "node20", "perl5.36", "bash-5.2", "pypy3"] {
        let refused = refusal_text(&scene, &format!("{program} {}", script.display()));
        assert!(
            refused.contains("session-writable root"),
            "{program}: {refused}"
        );
        let command = format!("{program} {}", clean.display());
        assert_eq!(refusal_text(&scene, &command), "", "{command}");
    }
}

#[test]
fn a_command_with_no_followable_target_is_refused_with_a_diagnostic() {
    let scene = scene_with_programs();
    for (command, reason) in [
        ("timeout 5", "names no command"),
        ("nohup", "names no command"),
        ("xargs", "names no command"),
        ("timeout --bogus 5 /bin/true", "--bogus"),
        ("xargs -I{} {}", "-I{}"),
        ("xargs --replace=x x", "--replace=x"),
        ("flock -c /bin/true /tmp/lock", "-c"),
        ("flock /tmp/lock -c /bin/true", "option"),
        ("env -S /bin/true", "option"),
        ("uv run tool", "`uv`"),
        ("uv run --with x tool", "`uv`"),
        ("npx tool", "`npx`"),
        ("busybox sh script", "`busybox`"),
        ("watch /bin/true", "`watch`"),
        ("find . -exec /bin/true ;", "`find`"),
        ("make hook", "`make`"),
        ("sudo /bin/true", "unsupported"),
        ("time /bin/true", "unsupported"),
        ("bash", "names no script"),
        ("bash -x", "names no script"),
        ("python3", "names no script"),
        ("xargs bash", "names no script"),
        ("python3 -c pass", "inline"),
        ("python3.12 -c pass", "inline"),
        ("ruby -e 0", "inline"),
    ] {
        let refused = refusal_text(&scene, command);
        assert!(
            refused.contains("cannot be verified"),
            "{command}: {refused:?}"
        );
        assert!(refused.contains(reason), "{command}: {refused:?}");
    }
}

#[test]
fn inline_shell_code_is_checked_as_the_commands_it_runs() {
    let scene = scene_with_programs();
    let script = scene.writable.join("notify.sh");
    executable(&script);
    let path = script.display();
    for command in [
        format!("bash -c '{path}'"),
        format!("bash -c '/bin/true; bash {path}'"),
        format!("timeout 5 bash -c 'nohup {path}'"),
        format!("bash -c \"bash -c '{path}'\""),
    ] {
        let refused = refusal_text(&scene, &command);
        assert!(
            refused.contains("session-writable root"),
            "{command}: {refused}"
        );
    }
    for (command, reason) in [
        ("bash -c 'cd /tmp; ./x'", "shell builtin `cd`"),
        ("bash -c '$X'", "dynamic"),
        ("bash -c", "names no inline code"),
    ] {
        let refused = refusal_text(&scene, command);
        assert!(refused.contains(reason), "{command}: {refused}");
    }
}

#[test]
fn shell_builtins_are_named_as_builtins_and_never_as_missing_programs() {
    let scene = scene_with_programs();
    for command in [
        "exit 0",
        ":",
        "true",
        "false",
        "echo done",
        "printf done",
        "test -f x",
        "pwd",
    ] {
        assert_eq!(
            scan(&scene, &[document("Stop", command)]),
            HookFindings::default(),
            "{command}"
        );
    }
    for (command, reason) in [
        ("cd /tmp && /bin/true", "shell builtin `cd`"),
        ("export A=1", "shell builtin `export`"),
        ("unset A", "shell builtin `unset`"),
        ("trap x EXIT", "shell builtin `trap`"),
        ("eval /bin/true", "unsupported"),
        ("source /bin/true", "unsupported"),
        (". /bin/true", "unsupported"),
        ("command /bin/true", "unsupported"),
        ("if /bin/true; then /bin/true; fi", "unsupported"),
    ] {
        let refused = refusal_text(&scene, command);
        assert!(refused.contains(reason), "{command}: {refused}");
        assert!(
            !refused.contains("not found on PATH"),
            "{command}: {refused}"
        );
    }
    let refused = refusal_text(&scene, "not-on-path");
    assert!(refused.contains("not found on PATH"), "{refused}");
}

#[test]
fn a_builtin_name_run_through_exec_or_a_wrapper_is_the_program_on_path() {
    let scene = scene_with_programs();
    symlink(scene.writable.join("true"), scene.bin.join("true")).unwrap();
    executable(&scene.writable.join("true"));
    for command in ["exec true", "timeout 5 true", "env true"] {
        let refused = refusal_text(&scene, command);
        assert!(
            refused.contains("session-writable root"),
            "{command}: {refused}"
        );
    }
    assert_eq!(refusal_text(&scene, "true"), "", "the shell runs its own");
}
