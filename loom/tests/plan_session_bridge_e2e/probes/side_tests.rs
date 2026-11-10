//! P2's side-process checks (plan B-D1, step B1), each against a live, idle session.

use serde_json::json;
use serial_test::serial;

use super::p2::install_skills;
use super::probe::Probe;
use super::side_run::{run_side, Variant, INITIALIZE};

/// Where the `initialize` request shape comes from, for the notes.
const SHAPE: &str = "Claude Code 2.1.321 bundle (the native binary under \
~/.local/share/claude/versions/2.1.321): its own clients send \
{type:\"control_request\",request_id,request:{subtype:\"initialize\"}}, optionally with hooks, \
sdkMcpServers, jsonSchema, systemPrompt and appendSystemPrompt; the answer is \
{type:\"control_response\",response:{subtype:\"success\",request_id,response:{commands,agents,\
output_style,available_output_styles,models,account,pid,...}}}. A mid-session list change is \
pushed as {type:\"system\",subtype:\"commands_changed\",commands,uuid,session_id}.";

fn loom_ui_off() -> serde_json::Value {
    json!({"loom-ui@skills-dir": false})
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p2_side_process_initialize_and_commands_changed() {
    let mut p = Probe::open("p2-side-initialize", install_skills);
    p.out
        .note("Control request shape", &format!("{INITIALIZE}\n\n{SHAPE}"));
    let variant = Variant {
        label: "loom-ui-disabled",
        extra: &[],
        plugins: Some(loom_ui_off()),
        add_skill: true,
    };
    run_side(&mut p, &variant);
    p.finish();
}

#[test]
#[ignore = "operator probe: needs a logged-in Claude Code and the network"]
#[serial]
fn p2_side_process_max_turns_and_loom_ui() {
    let mut p = Probe::open("p2-side-variants", install_skills);
    let max_turns = Variant {
        label: "max-turns-0",
        extra: &["--max-turns", "0"],
        plugins: Some(loom_ui_off()),
        add_skill: false,
    };
    run_side(&mut p, &max_turns);
    let loom_ui = p.home.join(".claude/skills/loom-ui");
    if loom_ui.is_dir() {
        let enabled = Variant {
            label: "loom-ui-enabled",
            extra: &[],
            plugins: None,
            add_skill: false,
        };
        run_side(&mut p, &enabled);
    } else {
        let why = "loom-ui is not installed (no ~/.claude/skills/loom-ui): the enabled \
                   sub-check is skipped; the disabled run is in p2-side-initialize";
        p.out.note("loom-ui enabled", why);
    }
    p.finish();
}
