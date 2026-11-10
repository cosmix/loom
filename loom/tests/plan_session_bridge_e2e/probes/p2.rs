//! P2 (plan step B1): skills pasted through the chat queue after Idle (a plugin skill with
//! args, a personal skill, a `context: fork` skill, a skill with `allowed-tools: WebFetch`)
//! and inline skill shell inside the sandbox.

use std::path::Path;

use serial_test::serial;

use super::p1::await_notification;
use super::paste::{paste, Spec};
use super::probe::Probe;
use super::setup::{record_extra_hooks, skill};

/// The personal skill every P2 session can run: it echoes its arguments.
pub const PERSONAL: &str = "probe-personal";

/// The recording hooks and the personal, fork and WebFetch skills.
pub fn install_skills(home: &Path, base: &Path) {
    record_extra_hooks(home, base);
    let echo = "Reply with exactly PERSONAL=$ARGUMENTS and nothing else.";
    skill(home, PERSONAL, &[], echo);
    skill(
        home,
        "probe-fork",
        &["context: fork"],
        "Reply with the single word FORK-OK.",
    );
    skill(
        home,
        "probe-webfetch",
        &["allowed-tools: WebFetch"],
        "Use the WebFetch tool to fetch https://example.com and reply with the page title only.",
    );
}

/// Where the inline-shell skill's two commands write: inside the project and outside it.
fn shell_targets(base: &Path) -> (std::path::PathBuf, std::path::PathBuf) {
    let inside = base.join("project/inline-inside");
    let outside = base.join("outside/inline-outside");
    (inside, outside)
}

/// `install_skills`, plus a skill whose inline shell touches a file in the project and one
/// outside it.
fn install_shell_skill(home: &Path, base: &Path) {
    install_skills(home, base);
    std::fs::create_dir_all(base.join("outside")).unwrap();
    let (inside, outside) = shell_targets(base);
    let body = format!(
        "Inside: !`touch {} && echo INSIDE-RAN`\n\nOutside: !`touch {} && echo OUTSIDE-RAN`\n\n\
         Reply with the single word SHELL-OK.",
        inside.display(),
        outside.display()
    );
    skill(home, "probe-shell", &[], &body);
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p2_plugin_personal_and_fork_skills() {
    let mut p = Probe::open("p2-skills", install_skills);
    paste(
        &mut p,
        &Spec::record("/loom-plan:args alpha beta", "loom-plan:args"),
    );
    paste(&mut p, &Spec::record("/probe-personal hello", PERSONAL));
    let lines = p.lines().len();
    paste(&mut p, &Spec::record("/probe-fork", "probe-fork"));
    let found = await_notification(&p, lines, 40);
    let body = if found.is_empty() {
        "no task-notification record within 40 s of the fork skill's turn".to_owned()
    } else {
        super::evidence::joined(&found)
    };
    p.out.note("Fork skill: the background result", &body);
    p.finish();
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p2_inline_shell_and_webfetch_skills() {
    let mut p = Probe::open("p2-shell-webfetch", install_shell_skill);
    paste(&mut p, &Spec::record("/probe-shell", "probe-shell"));
    let (inside, outside) = shell_targets(&p.base);
    let shell = p.lines();
    let ran = |needle: &str| !super::evidence::containing(&shell, needle).is_empty();
    let body = format!(
        "file in the project exists: {}\nfile outside the project exists: {}\n\
         a record shows INSIDE-RAN: {}\na record shows OUTSIDE-RAN: {}",
        inside.exists(),
        outside.exists(),
        ran("INSIDE-RAN"),
        ran("OUTSIDE-RAN")
    );
    p.out.note("Inline skill shell inside the sandbox", &body);
    paste(&mut p, &Spec::record("/probe-webfetch", "probe-webfetch"));
    p.finish();
}
