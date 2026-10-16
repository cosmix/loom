//! Forwarders own their prompt protocol and do not receive the generic preamble.
use super::*;

#[test]
fn codex_bound_types_never_receive_the_preamble() {
    let (_temp, hook) = setup_hook();
    let (home, cwd) = (temp(), temp());
    for subagent_type in ["loom-codex-forwarder", "codex:codex-rescue"] {
        let input = json!({"subagent_type": subagent_type, "model": "sonnet", "prompt": "task"});
        let out = run_hook(&hook, "Agent", input, cwd.path(), home.path(), &[]);
        assert_eq!(out.code, 0, "stderr={}", out.stderr);
        assert!(
            !out.stdout.contains("updatedInput") && !out.stdout.contains("LOOM_HOOK_WARN"),
            "{subagent_type}: stdout={}",
            out.stdout
        );
    }
}
