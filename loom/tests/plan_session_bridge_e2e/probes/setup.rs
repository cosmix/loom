//! What a probe installs before its launch: the session's guidance and plugin, the git
//! project, test skills in the copied HOME, and recording hooks for events loom does not spool.

use std::fs;
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::process::Command;

use loom::plan_session::bridge::{LaunchGuidance, StartRequest};
use loom::plan_session::kind::{SessionDir, SessionKind};
use loom::plan_session::model::Origin;
use serde_json::{json, Value};

/// The opening every probe session answers before its scenario starts.
pub const OPENING: &str = "Reply with the single word READY.";
/// Under the probe's base: the payloads of the recording hooks, one JSON line each.
pub const EXTRA_HOOKS: &str = "extra-hooks.jsonl";
/// Hook events Claude Code may fire that loom's `SPOOL_EVENTS` does not register.
const RECORDED: [&str; 2] = ["UserPromptExpansion", "PostModelSwitch"];

/// The plugin's skill that echoes its arguments, run as `/loom-plan:args <words>`.
const ARGS_SKILL: &str = "---\nname: args\ndescription: Echo the arguments back\n---\n\
Reply with exactly ARGS=$ARGUMENTS and nothing else.\n";

/// An authoring start whose guidance writes the brief and the `loom-plan` plugin, with
/// `/loom-plan:probe` and the `/loom-plan:args` skill; the opening arrives by paste.
pub fn probe_request() -> StartRequest {
    let guidance = Box::new(move |dir: &SessionDir| -> anyhow::Result<LaunchGuidance> {
        fs::write(dir.brief(), "You are a test session. Answer briefly.")?;
        let manifest = dir.plugin().join(".claude-plugin");
        fs::create_dir_all(&manifest)?;
        let plugin = r#"{"name": "loom-plan", "version": "0.0.0", "description": "test"}"#;
        fs::write(manifest.join("plugin.json"), plugin)?;
        fs::create_dir_all(dir.plugin().join("commands"))?;
        let probe = "Reply with the single word PROBE-OK.";
        fs::write(dir.plugin().join("commands/probe.md"), probe)?;
        fs::create_dir_all(dir.plugin().join("skills/args"))?;
        fs::write(dir.plugin().join("skills/args/SKILL.md"), ARGS_SKILL)?;
        Ok(LaunchGuidance {
            brief_path: dir.brief(),
            plugin_dir: dir.plugin(),
            opening_message: OPENING.to_owned(),
            model: "sonnet".to_owned(),
            effort: "low".to_owned(),
            display_name: "loom plan: probe".to_owned(),
            opening_on_argv: false,
        })
    });
    StartRequest {
        kind: SessionKind::Authoring,
        guidance,
        opening_origin: Some(Origin::Offer),
        offer: None,
        feature: None,
        plan: None,
        extra_env: Vec::new(),
    }
}

pub fn git_init(root: &Path) {
    let git = Command::new("git").args(["init", "-q"]).arg(root).status();
    assert!(git.is_ok_and(|status| status.success()), "git init failed");
}

/// The first line of `claude --version`.
pub fn version_of(claude: &Path) -> String {
    match Command::new(claude).arg("--version").output() {
        Ok(out) => {
            let text = String::from_utf8_lossy(&out.stdout);
            text.lines().next().unwrap_or_default().to_owned()
        }
        Err(error) => format!("cannot run {}: {error}", claude.display()),
    }
}

/// Writes `<home>/.claude/skills/<name>/SKILL.md` with `frontmatter` lines and `body`.
pub fn skill(home: &Path, name: &str, frontmatter: &[&str], body: &str) {
    let dir = home.join(".claude/skills").join(name);
    fs::create_dir_all(&dir).unwrap();
    let mut text = format!("---\nname: {name}\ndescription: Probe skill {name}\n");
    for line in frontmatter {
        text.push_str(line);
        text.push('\n');
    }
    text.push_str("---\n");
    text.push_str(body);
    text.push('\n');
    fs::write(dir.join("SKILL.md"), text).unwrap();
}

/// Registers, in the copied user settings, a hook for each of `RECORDED` that appends its
/// payload to `<base>/extra-hooks.jsonl`, so the notes show whether Claude Code fires it.
pub fn record_extra_hooks(home: &Path, base: &Path) {
    let script = base.join("record-hook.sh");
    let body = "#!/bin/sh\npayload=$(cat)\n\
printf '{\"event\":\"%s\",\"payload\":%s}\\n' \"$2\" \"${payload:-null}\" >> \"$1\"\n";
    fs::write(&script, body).unwrap();
    fs::set_permissions(&script, fs::Permissions::from_mode(0o755)).unwrap();
    let path = home.join(".claude/settings.json");
    let text = fs::read_to_string(&path).unwrap_or_else(|_| "{}".to_owned());
    let mut settings: Value = serde_json::from_str(&text).unwrap_or_else(|_| json!({}));
    if !settings["hooks"].is_object() {
        settings["hooks"] = json!({});
    }
    let log = base.join(EXTRA_HOOKS);
    for event in RECORDED {
        let command = format!("'{}' '{}' {event}", script.display(), log.display());
        let hook = json!({"hooks": [{"type": "command", "command": command}]});
        settings["hooks"][event] = json!([hook]);
    }
    fs::write(&path, serde_json::to_string_pretty(&settings).unwrap()).unwrap();
}
