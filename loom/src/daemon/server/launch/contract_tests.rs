//! Contract test: the daemon child never receives the caller's terminal
//! emulator.

use super::*;

#[test]
fn daemon_child_never_receives_a_terminal_hint() {
    let env = DaemonEnvironment::capture_from([
        ("LOOM_TERMINAL", "kitty"),
        ("TERM", "xterm-256color"),
        ("HOME", "/home/user"),
    ]);
    let command = daemon_command(
        Path::new("/usr/bin/loom"),
        Path::new("/repo/.loom/work"),
        &DaemonConfig::default(),
        &env,
    );
    let envs: Vec<_> = command.get_envs().collect();
    assert!(
        envs.iter().all(|(key, _)| *key != "LOOM_TERMINAL"),
        "the daemon child must get no LOOM_TERMINAL entry: {envs:?}"
    );
    assert!(
        envs.iter()
            .any(|(key, value)| *key == "TERM"
                && value.is_some_and(|value| value == "xterm-256color")),
        "the daemon child keeps TERM: {envs:?}"
    );
}
