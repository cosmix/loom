//! Check 5's rule for what a hook command passes, whatever its program, and the
//! interpreter grammar it relies on.

use super::*;

/// `scene_with_programs` plus the programs only these cases name.
fn scene_with_tools() -> Scene {
    let scene = scene_with_programs();
    for name in [
        "tool", "ionice", "taskset", "java", "go", "cargo", "gradle", "perl", "bun", "deno", "node",
    ] {
        executable(&scene.bin.join(name));
    }
    scene
}

/// `template` with `{w}` as the writable root and `{o}` as the outside directory.
fn command_in(scene: &Scene, template: &str) -> String {
    template
        .replace("{w}", &scene.writable.display().to_string())
        .replace("{o}", &scene.outside.display().to_string())
}

#[track_caller]
fn assert_refused_under_root(scene: &Scene, templates: &[&str]) {
    for template in templates {
        let command = command_in(scene, template);
        let refused = refusal_text(scene, &command);
        assert!(
            refused.contains("session-writable root"),
            "{command}: {refused:?}"
        );
    }
}

#[track_caller]
fn assert_clean(scene: &Scene, templates: &[&str]) {
    for template in templates {
        let command = command_in(scene, template);
        assert_eq!(
            scan(scene, &[document("Stop", &command)]),
            HookFindings::default(),
            "{command}"
        );
    }
}

#[test]
fn an_assignment_into_a_writable_root_is_refused_whatever_its_name() {
    let scene = scene_with_tools();
    std::fs::create_dir_all(scene.repo.join("lib")).unwrap();
    assert_refused_under_root(
        &scene,
        &[
            "BASH_ENV={w}/x.sh /bin/true",
            "LD_PRELOAD={w}/x.so /bin/true",
            "PYTHONPATH={w}/lib /bin/true",
            "PYTHONPATH={o}/lib:{w}/lib /bin/true",
            "BASH_ENV=./x.sh /bin/true",
            "PYTHONPATH=. /bin/true",
            "PYTHONPATH=lib /bin/true",
            "env BASH_ENV={w}/x.sh /bin/true",
            "env -i LD_PRELOAD={w}/x.so /bin/true",
            "BASH_ENV={w}/x.sh true",
        ],
    );
    assert_clean(
        &scene,
        &[
            "FOO=1 /bin/true",
            "LANG=en_US.UTF-8 /bin/true",
            "TZ=America/New_York /bin/true",
            "LD_PRELOAD={o}/x.so /bin/true",
            "env FOO='some value' /bin/true",
        ],
    );
}

#[test]
fn any_argument_into_a_writable_root_is_refused_whatever_the_program() {
    let scene = scene_with_tools();
    executable(&scene.repo.join("main.jar"));
    executable(&scene.writable.join("status"));
    symlink(scene.writable.join("status"), scene.bin.join("status")).unwrap();
    std::fs::create_dir_all(scene.repo.join("sub")).unwrap();
    std::fs::write(scene.repo.join("sub/config"), "").unwrap();
    assert_refused_under_root(
        &scene,
        &[
            "ionice -c 3 {w}/hook.sh",
            "taskset 1 {w}/hook.sh",
            "java -jar {w}/app.jar",
            "java -cp {o}/lib:{w}/lib Main",
            "java -jar main.jar",
            "tool --config={w}/c",
            "tool -I{w}/lib",
            "tool if={w}/x",
            "tool ./not-yet-written",
            "tool ../repo/not-yet-written",
            "tool sub/config",
            "tool $CLAUDE_PROJECT_DIR/not-yet-written",
            "ionice status",
            "timeout 5 ionice -c 3 {w}/hook.sh",
            "bash -c 'ionice {w}/hook.sh'",
            "python3 {o}/hook.py {w}/data",
            "{o}/hook.sh {w}/data",
            "flock {w}/lock /bin/true",
        ],
    );
    std::fs::create_dir_all(scene.repo.join("build")).unwrap();
    assert_clean(
        &scene,
        &[
            "tool --flag",
            "tool build",
            "tool -n 5",
            "tool America/New_York",
            "tool https://example.com/x",
            "ionice -c 3 {o}/hook.sh",
            "tool --config={o}/c",
            "bash -c 'echo {w}/hook.sh'",
            "echo {w}/hook.sh",
            "test -f {w}/hook.sh",
        ],
    );
}

#[test]
fn a_build_runner_is_refused_as_choosing_what_it_runs() {
    let scene = scene_with_tools();
    for (command, reason) in [
        ("go run .", "`go`"),
        ("cargo run --manifest-path=/opt/Cargo.toml", "`cargo`"),
        ("gradle hook", "`gradle`"),
    ] {
        let refused = refusal_text(&scene, command);
        assert!(
            refused.contains("cannot be verified"),
            "{command}: {refused:?}"
        );
        assert!(refused.contains(reason), "{command}: {refused:?}");
    }
}

/// `-I` takes a directory for perl and ruby, and is python's isolated-mode flag.
#[test]
fn dash_capital_i_is_read_per_interpreter() {
    let scene = scene_with_tools();
    let script = scene.writable.join("hook.pl");
    executable(&script);
    let runs = format!("runs {}", script.display());
    for command in [
        "perl -I {o}/lib {w}/hook.pl",
        "perl -I{o}/lib {w}/hook.pl",
        "ruby -I {o}/lib {w}/hook.pl",
        "python3 -I {w}/hook.pl",
    ] {
        let command = command_in(&scene, command);
        let refused = refusal_text(&scene, &command);
        assert!(refused.contains(&runs), "{command}: {refused:?}");
    }
    assert_clean(
        &scene,
        &[
            "perl -I {o}/lib {o}/hook.pl",
            "ruby -I{o}/lib {o}/hook.pl",
            "python3 -I {o}/hook.py",
        ],
    );
    assert_refused_under_root(&scene, &["perl -I {w}/lib {o}/hook.pl"]);
    for (command, reason) in [
        ("node -I x y", "`node` option `-I`"),
        ("bash -I x", "`bash` option `-I`"),
        ("perl -I", "names no script"),
        ("perl -E 0", "inline"),
    ] {
        let refused = refusal_text(&scene, command);
        assert!(refused.contains(reason), "{command}: {refused:?}");
    }
}

#[test]
fn bun_and_deno_resolve_a_run_script_and_name_what_they_cannot_follow() {
    let scene = scene_with_tools();
    let script = scene.writable.join("hook.ts");
    executable(&script);
    let runs = format!("runs {}", script.display());
    for command in [
        "bun run {w}/hook.ts",
        "deno run {w}/hook.ts",
        "bun {w}/hook.ts",
    ] {
        let command = command_in(&scene, command);
        let refused = refusal_text(&scene, &command);
        assert!(refused.contains(&runs), "{command}: {refused:?}");
    }
    assert_clean(
        &scene,
        &[
            "bun run {o}/hook.ts",
            "deno run {o}/hook.ts",
            "bun {o}/hook.ts",
        ],
    );
    for (command, reason) in [
        ("bun run dev", "`bun run dev`"),
        ("bun run", "`bun run` names no script"),
        (
            "deno run --allow-all /opt/x.ts",
            "`deno run` option `--allow-all`",
        ),
        ("bun test", "`bun test`"),
        ("bun x tool", "`bun x`"),
        ("deno task hook", "`deno task`"),
    ] {
        let refused = refusal_text(&scene, command);
        assert!(
            refused.contains("cannot be verified"),
            "{command}: {refused:?}"
        );
        assert!(refused.contains(reason), "{command}: {refused:?}");
    }
}

/// Loom registers its hooks through `shell_escape`, so a hooks directory holding
/// a pattern character arrives quoted and names one literal path.
#[test]
fn a_quoted_literal_path_with_pattern_characters_is_a_path() {
    let scene = scene_with_tools();
    for name in ["hooks[1]", "ho?ks", "st*r", "br{a,b}ce", "o'brien!"] {
        let clean = scene.outside.join(name).join("commit-guard.sh");
        let session = scene.writable.join(name).join("commit-guard.sh");
        executable(&clean);
        executable(&session);
        let spell =
            |path: &Path| shell_escape::escape(path.display().to_string().into()).into_owned();
        assert_clean(&scene, &[spell(&clean).as_str()]);
        assert_refused_under_root(&scene, &[spell(&session).as_str()]);
        assert_clean(&scene, &[format!("\"{}\"", clean.display()).as_str()]);
    }
    for command in [
        "/opt/h*.sh",
        "/opt/h?.sh",
        "/opt/[ab].sh",
        "/opt/{a,b}.sh",
        "'/opt/x'*",
        "$HOME/{a,b}.sh",
    ] {
        let refused = refusal_text(&scene, command);
        assert!(refused.contains("shell pattern"), "{command}: {refused:?}");
    }
}
