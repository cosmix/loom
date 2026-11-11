//! The shell script a planning session runs inside its tmux pane: environment, flags and the claude command line.

use std::ffi::OsString;
use std::path::Path;

use shell_escape::escape;

use crate::plan_session::bridge::host::LaunchSpec;
use crate::plan_session::sandbox::{hook_command, permission_mode};
use crate::process::agent_session_environment_from;

/// Build the fresh session's launch line, with an argv opening only when explicitly supplied.
pub fn launch_script(spec: &LaunchSpec, settings: &Path, env: &[(OsString, OsString)]) -> String {
    let mut words = vec!["exec".to_owned(), "env".to_owned(), "-i".to_owned()];
    words.extend(claude_env(spec, env).into_iter().map(|(name, value)| {
        let mut assignment = name;
        assignment.push("=");
        assignment.push(value);
        escape(assignment.to_string_lossy()).into_owned()
    }));
    words.extend(
        claude_argv(spec, settings)
            .iter()
            .map(|word| escape(word.to_string_lossy()).into_owned()),
    );
    if let Some(opening) = &spec.opening_argv {
        words.push(escape(opening.as_str().into()).into_owned());
    }
    format!(
        "#!/bin/sh\ncd {} || exit 1\n{} </dev/null >/dev/null 2>&1 &\n{}\n",
        escape(spec.project_root.to_string_lossy()),
        hook_command(&spec.loom_bin, "OwnerWatch", spec.dir.path()),
        words.join(" ")
    )
}

/// The claude command line of a session, from the binary through
/// `--append-system-prompt-file <brief>`: never an opening, which `launch_script` appends itself
/// and a headless reuse of these words must not receive (with `-p` it is a user message).
pub fn claude_argv(spec: &LaunchSpec, settings: &Path) -> Vec<OsString> {
    let guidance = &spec.guidance;
    let mut words = vec![spec.claude_bin.as_os_str().to_owned()];
    let pairs: [(&str, &std::ffi::OsStr); 8] = [
        ("--session-id", spec.id.as_ref()),
        ("--name", guidance.display_name.as_ref()),
        ("--settings", settings.as_os_str()),
        ("--setting-sources", "user,project".as_ref()),
        ("--model", guidance.model.as_ref()),
        ("--effort", guidance.effort.as_ref()),
        ("--permission-mode", permission_mode(&spec.kind).as_ref()),
        ("--plugin-dir", guidance.plugin_dir.as_os_str()),
    ];
    for (flag, value) in pairs {
        words.push(flag.into());
        words.push(value.to_owned());
    }
    words.push("--strict-mcp-config".into());
    words.push("--append-system-prompt-file".into());
    words.push(guidance.brief_path.as_os_str().to_owned());
    words
}

/// What a session's environment starts from, before [`claude_env`] filters and pins it: the
/// agent allowlist of this process's environment (`PATH` included), then the start request's
/// extras. The live session (`launch`) and its side process (the catalogue owner) share it.
pub fn inherited_env(spec: &LaunchSpec) -> Vec<(OsString, OsString)> {
    let mut env = agent_session_environment_from(std::env::vars_os());
    let extra = spec.extra_env.iter();
    env.extend(extra.map(|(name, value)| (name.into(), value.into())));
    env
}

/// The environment `env -i` gives a session: `env` without the stage identity, the parent tmux
/// variables and any `HOME` or planning identity, then the pinned `HOME`, `LOOM_PLAN_STATE` and
/// `LOOM_PLAN_SESSION`.
pub fn claude_env(spec: &LaunchSpec, env: &[(OsString, OsString)]) -> Vec<(OsString, OsString)> {
    // Pin planning identity here too: callers cannot replace it with a stage's identity, and
    // HOME is `spec.home`, the home the trust entry and the sandbox use, so no inherited or
    // `extra_env` HOME can split them from the one claude runs under.
    // The server's own TMUX, TMUX_PANE and TMUX_TMPDIR name the user's outer tmux server
    // (when `loom plan` runs inside one); the session reaches no tmux server, so they go too.
    // The private pane's own values never arrive: `env -i` clears them.
    let planning = [
        (OsString::from("HOME"), spec.home.as_os_str().to_owned()),
        (
            OsString::from("LOOM_PLAN_STATE"),
            spec.runtime.dir.as_os_str().to_owned(),
        ),
        (
            OsString::from("LOOM_PLAN_SESSION"),
            OsString::from(&spec.id),
        ),
    ];
    env.iter()
        .filter(|(name, _)| {
            !matches!(
                name.to_str(),
                Some(
                    "HOME"
                        | "LOOM_STAGE_ID"
                        | "LOOM_SESSION_ID"
                        | "LOOM_WORK_DIR"
                        | "LOOM_PLAN_STATE"
                        | "LOOM_PLAN_SESSION"
                        | "TMUX"
                        | "TMUX_PANE"
                        | "TMUX_TMPDIR"
                )
            )
        })
        .chain(planning.iter())
        .cloned()
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::os::unix::fs::PermissionsExt;
    use std::process::{Command, Output};
    use std::time::Duration;

    use crate::plan_session::bridge::LaunchGuidance;
    use crate::plan_session::kind::{SessionDir, SessionKind};
    use crate::planning::runtime::runtime_paths_under;
    use crate::process::run_bounded_output;

    const OPENING: &str = "OPENING-7f3a: private document contents";
    const NAME: &str = "-it's $(touch pwned)\nsecond line";
    const FAKE_CLAUDE: &str = "#!/bin/sh\nprintf '%s\\0' \"$PWD\" \
        \"${LOOM_PLAN_SESSION-}\" \"${LOOM_PLAN_STATE-}\" \
        \"${LOOM_STAGE_ID+set}\" \"${LOOM_SESSION_ID+set}\" \
        \"${LOOM_WORK_DIR+set}\" \"${CUSTOM-}\" \"$@\"\n";

    fn spec(base: &Path) -> LaunchSpec {
        let root = base.join("project '$\nspace");
        fs::create_dir(&root).unwrap();
        let runtime = runtime_paths_under(&base.join("loom-home"), &root).unwrap();
        let dir = SessionDir::new(runtime.sessions.join("session-test"));
        let claude_bin = base.join("claude '$\nspace");
        fs::write(&claude_bin, FAKE_CLAUDE).unwrap();
        fs::set_permissions(&claude_bin, fs::Permissions::from_mode(0o700)).unwrap();
        LaunchSpec {
            id: "session-test".into(),
            kind: SessionKind::Authoring,
            socket: "loom-plan-test".into(),
            tmux_session: "plan-authoring".into(),
            guidance: LaunchGuidance {
                brief_path: dir.brief(),
                plugin_dir: dir.plugin(),
                opening_message: OPENING.into(),
                model: "opus".into(),
                effort: "medium".into(),
                display_name: NAME.into(),
                opening_on_argv: false,
            },
            dir,
            extra_env: Vec::new(),
            opening_argv: None,
            project_root: root,
            runtime,
            loom_bin: base.join("loom"),
            claude_bin,
            home: base.join("home"),
        }
    }

    fn run(script: &str, base: &Path, syntax_only: bool) -> Output {
        let path = base.join("launch.sh");
        fs::write(&path, script).unwrap();
        let mut command = Command::new("sh");
        if syntax_only {
            command.arg("-n");
        }
        command.arg(path).current_dir(base);
        for name in ["LOOM_STAGE_ID", "LOOM_SESSION_ID", "LOOM_WORK_DIR"] {
            command.env(name, "inherited-stage");
        }
        let output =
            run_bounded_output(&mut command, Duration::from_secs(5), "launch script").unwrap();
        assert!(output.status.success(), "{output:?}\n{script}");
        output
    }

    fn fields(output: Output) -> Vec<String> {
        String::from_utf8(output.stdout)
            .unwrap()
            .split_terminator('\0')
            .map(str::to_owned)
            .collect()
    }

    #[test]
    fn flags_are_present_once_with_no_positional_prompt_or_resume() {
        let tmp = tempfile::tempdir().unwrap();
        let spec = spec(tmp.path());
        let settings = spec.dir.settings();

        let script = launch_script(&spec, &settings, &[]);
        let actual = fields(run(&script, tmp.path(), false));

        assert_eq!(
            &actual[7..],
            [
                "--session-id",
                spec.id.as_str(),
                "--name",
                NAME,
                "--settings",
                settings.to_str().unwrap(),
                "--setting-sources",
                "user,project",
                "--model",
                "opus",
                "--effort",
                "medium",
                "--permission-mode",
                "auto",
                "--plugin-dir",
                spec.guidance.plugin_dir.to_str().unwrap(),
                "--strict-mcp-config",
                "--append-system-prompt-file",
                spec.guidance.brief_path.to_str().unwrap(),
            ]
        );
        assert!(!script.contains(OPENING));
        assert!(!script.contains("--resume"));
        assert!(!script.contains("--continue"));
        assert!(!tmp.path().join("pwned").exists());
        assert!(!spec.project_root.join("pwned").exists());
    }

    #[test]
    fn planning_environment_is_pinned_and_stage_variables_are_dropped() {
        let tmp = tempfile::tempdir().unwrap();
        let spec = spec(tmp.path());
        let custom = "it's $HOME $(touch pwned)\nsecond line";
        let env: Vec<_> = [
            ("LOOM_STAGE_ID", "stage"),
            ("LOOM_SESSION_ID", "parent"),
            ("LOOM_WORK_DIR", "/parent"),
            ("LOOM_PLAN_STATE", "/wrong"),
            ("LOOM_PLAN_SESSION", "wrong"),
            ("CUSTOM", custom),
        ]
        .map(|(name, value)| (name.into(), value.into()))
        .into();

        let script = launch_script(&spec, &spec.dir.settings(), &env);
        let actual = fields(run(&script, tmp.path(), false));

        assert_eq!(
            &actual[..7],
            [
                spec.project_root.to_str().unwrap(),
                spec.id.as_str(),
                spec.runtime.dir.to_str().unwrap(),
                "",
                "",
                "",
                custom,
            ]
        );
        for name in ["LOOM_STAGE_ID", "LOOM_SESSION_ID", "LOOM_WORK_DIR"] {
            assert!(!script.contains(name), "stage variable leaked: {name}");
        }
        assert_eq!(script.matches("LOOM_PLAN_SESSION=").count(), 1);
        assert_eq!(script.matches("LOOM_PLAN_STATE=").count(), 1);
        assert!(!spec.project_root.join("pwned").exists());
    }

    #[test]
    fn launch_script_drops_the_parent_tmux_environment() {
        let tmp = tempfile::tempdir().unwrap();
        let spec = spec(tmp.path());
        let env: Vec<_> = [
            ("TMUX", "/tmp/tmux-1000/default,123,0"),
            ("TMUX_PANE", "%3"),
            ("TMUX_TMPDIR", "/tmp"),
            ("HOME", "/home/u"),
        ]
        .map(|(name, value)| (name.into(), value.into()))
        .into();

        let script = launch_script(&spec, &spec.dir.settings(), &env);

        assert!(!script.contains("TMUX"), "{script}");
        assert!(script.contains("2>&1 &\nexec env -i "), "{script}");
    }

    #[test]
    fn home_is_the_spec_home_whatever_the_inherited_or_extra_environment_says() {
        let tmp = tempfile::tempdir().unwrap();
        let spec = spec(tmp.path());
        fs::write(&spec.claude_bin, "#!/bin/sh\nprintf '%s' \"$HOME\"\n").unwrap();
        let env: Vec<_> = [("HOME", "/home/inherited"), ("HOME", "/home/extra")]
            .map(|(name, value)| (name.into(), value.into()))
            .into();

        let script = launch_script(&spec, &spec.dir.settings(), &env);
        let output = run(&script, tmp.path(), false);

        assert_eq!(
            String::from_utf8(output.stdout).unwrap(),
            spec.home.to_str().unwrap()
        );
        assert_eq!(script.matches("HOME=").count(), 1, "{script}");
    }

    #[test]
    fn explicit_opening_is_the_last_escaped_word() {
        let tmp = tempfile::tempdir().unwrap();
        let mut spec = spec(tmp.path());
        let opening = "/loom-plan:pressure doc/plans/it's-$(touch pwned)\nplan.md";
        spec.opening_argv = Some(opening.into());

        let script = launch_script(&spec, &spec.dir.settings(), &[]);
        let actual = fields(run(&script, tmp.path(), false));

        assert_eq!(actual.len(), 27);
        assert_eq!(actual.last().unwrap(), opening);
        assert!(!script.contains(OPENING));
        assert!(!spec.project_root.join("pwned").exists());
    }

    #[test]
    fn shell_accepts_the_script_syntax_with_and_without_an_opening() {
        let tmp = tempfile::tempdir().unwrap();
        let mut spec = spec(tmp.path());
        for opening in [None, Some("/loom-plan:pressure doc/plans/PLAN-x.md".into())] {
            spec.opening_argv = opening;
            let script = launch_script(&spec, &spec.dir.settings(), &[]);
            run(&script, tmp.path(), true);
        }
    }
}
