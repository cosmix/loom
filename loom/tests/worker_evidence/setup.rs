use super::support::{AGENT_ID, LOOM_SESSION, PARENT_UUID, STAGE};
use loom::fs::permissions::constants::{
    HOOK_COMMON, HOOK_LIFECYCLE, HOOK_READ_LEDGER, HOOK_SUBAGENT_START, HOOK_SUBAGENT_STOP,
    HOOK_TEAMMATE_IDLE,
};
use loom::hooks::{HookEvent, HooksConfig};
use loom::plan::schema::PermissionMode;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::{Path, PathBuf};

const TRANSCRIPT_AT: &str = "2099-01-01T00:00:00.000Z";

pub(super) fn prepare_layout(root: &Path, work: &Path) -> (PathBuf, PathBuf) {
    fs::create_dir_all(work.join("stages")).expect("create stage directory");
    fs::create_dir_all(work.join("subagents")).expect("create subagents directory");
    write_stage(work, LOOM_SESSION);
    let project = root.join("claude-project");
    let parent = project.join(format!("{PARENT_UUID}.jsonl"));
    let worker = project
        .join(PARENT_UUID)
        .join("subagents")
        .join(format!("agent-{AGENT_ID}.jsonl"));
    fs::create_dir_all(worker.parent().expect("worker directory"))
        .expect("create worker directory");
    fs::write(&parent, "{\"type\":\"parent\"}\n").expect("write parent transcript");
    fs::write(&worker, format!("{}\n", transcript_row("finished")))
        .expect("write worker transcript");
    (parent, worker)
}

pub(super) fn write_stage(work: &Path, session: &str) {
    let content = format!("---\nid: {STAGE}\nsession: {session}\n---\n");
    fs::write(work.join("stages").join(format!("01-{STAGE}.md")), content)
        .expect("write stage binding");
}

pub(super) fn transcript_row(text: &str) -> Value {
    json!({
        "type": "assistant",
        "timestamp": TRANSCRIPT_AT,
        "message": {
            "role": "assistant",
            "content": [{"type": "text", "text": text}],
        },
    })
}

pub(super) fn install_hooks(hooks: &Path) {
    fs::create_dir_all(hooks).expect("create hooks directory");
    for (name, content) in [
        ("_common.sh", HOOK_COMMON),
        ("_lifecycle.sh", HOOK_LIFECYCLE),
        ("_read_ledger.sh", HOOK_READ_LEDGER),
        ("subagent-start.sh", HOOK_SUBAGENT_START),
        ("subagent-stop.sh", HOOK_SUBAGENT_STOP),
        ("teammate-idle.sh", HOOK_TEAMMATE_IDLE),
    ] {
        write_exec(&hooks.join(name), content);
    }
}

pub(super) fn generated_commands(hooks: &Path, work: &Path) -> HashMap<HookEvent, String> {
    let settings = HooksConfig::new(
        hooks.to_path_buf(),
        work.to_path_buf(),
        PermissionMode::AcceptEdits,
    )
    .to_settings_hooks();
    let mut commands = HashMap::new();
    for event in [
        HookEvent::SubagentStart,
        HookEvent::SubagentStop,
        HookEvent::TeammateIdle,
    ] {
        let rules = settings
            .get(&event.to_string())
            .expect("generated event rule");
        assert_eq!(rules.len(), 1, "{event} must have exactly one rule");
        assert_eq!(
            rules[0].hooks.len(),
            1,
            "{event} must have exactly one command"
        );
        let hook = &rules[0].hooks[0];
        assert_eq!(hook.hook_type, "command");
        assert_eq!(
            hook.command,
            hooks.join(event.script_name()).display().to_string()
        );
        commands.insert(event, hook.command.clone());
    }
    commands
}

pub(super) fn install_shims(bin: &Path) {
    fs::create_dir_all(bin).expect("create shim directory");
    write_exec(
        &bin.join("date"),
        concat!(
            "#!/usr/bin/env bash\n",
            "if [[ \"$#\" == 2 && \"$1\" == -u && ",
            "\"$2\" == +%Y-%m-%dT%H:%M:%S.000Z ]]; then\n",
            "  printf '%s\\n' \"$FIXTURE_NOW\"\n",
            "  exit 0\n",
            "fi\n",
            "PATH=/usr/bin:/bin exec date \"$@\"\n",
        ),
    );
    write_exec(&bin.join("loom"), "#!/usr/bin/env bash\nexit 0\n");
}

fn write_exec(path: &Path, content: &str) {
    fs::write(path, content).unwrap_or_else(|error| panic!("write {}: {error}", path.display()));
    let mut permissions = fs::metadata(path).expect("hook metadata").permissions();
    permissions.set_mode(0o755);
    fs::set_permissions(path, permissions).expect("make hook executable");
}
