//! One side-process run against the live session (plan B-D1, step B1): `initialize` with no
//! user message, `system/init`, `commands_changed` on a new skill file, and what the run wrote.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use serde_json::Value;

use super::evidence::{joined, lines_of};
use super::probe::Probe;
use super::secs;
use super::setup::skill;
use super::side::{live_argv, live_env, settings_of, side_argv, side_settings, Side};

/// The stream-json control request, as the 2.1.321 bundle's own clients send it.
pub const INITIALIZE: &str =
    r#"{"type":"control_request","request_id":"probe-init-1","request":{"subtype":"initialize"}}"#;

pub struct Variant {
    pub label: &'static str,
    pub extra: &'static [&'static str],
    /// The settings copy's `enabledPlugins`; `None` removes the key.
    pub plugins: Option<Value>,
    /// Add a skill file after `initialize` and wait for `commands_changed`.
    pub add_skill: bool,
}

/// The files under a directory with their size and modification time.
fn listing(dir: &Path) -> BTreeMap<PathBuf, (u64, Option<SystemTime>)> {
    let mut found = BTreeMap::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(next) = stack.pop() {
        for entry in fs::read_dir(&next)
            .into_iter()
            .flatten()
            .filter_map(Result::ok)
        {
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                stack.push(entry.path());
            } else {
                found.insert(entry.path(), (meta.len(), meta.modified().ok()));
            }
        }
    }
    found
}

/// The projects listing, spool bytes and transcript bytes the run must leave alone.
struct Snapshot {
    projects: BTreeMap<PathBuf, (u64, Option<SystemTime>)>,
    spool: Vec<u8>,
    transcript: Vec<u8>,
}

impl Snapshot {
    fn take(p: &Probe) -> Snapshot {
        Snapshot {
            projects: listing(&p.home.join(".claude/projects")),
            spool: fs::read(p.dir.events()).unwrap_or_default(),
            transcript: fs::read(p.transcript()).unwrap_or_default(),
        }
    }

    /// What differs in `after`, as note lines.
    fn compare(&self, after: &Snapshot) -> String {
        let changed = after
            .projects
            .iter()
            .filter(|(path, now)| self.projects.get(*path) != Some(now));
        let changed: Vec<String> = changed
            .map(|(path, _)| path.display().to_string())
            .collect();
        format!(
            "files added or changed under <copied HOME>/.claude/projects: {}\n{}\n\
             live spool byte-identical: {}\nlive transcript byte-identical: {}",
            changed.len(),
            changed.join("\n"),
            self.spool == after.spool,
            self.transcript == after.transcript
        )
    }
}

fn kind(value: &Value) -> String {
    let (t, s) = (value["type"].as_str(), value["subtype"].as_str());
    format!("{}/{}", t.unwrap_or("?"), s.unwrap_or("-"))
}

fn is_answer(value: &Value) -> bool {
    value["type"] == "control_response"
        && value.pointer("/response/request_id") == Some(&Value::from("probe-init-1"))
}

fn is_kind(subtype: &'static str) -> impl Fn(&Value) -> bool {
    move |value| value["type"] == "system" && value["subtype"] == subtype
}

/// The names in `list`: strings, or objects' `name`.
fn names(list: Option<&Value>) -> Vec<String> {
    let items = list.and_then(Value::as_array).into_iter().flatten();
    let name = |item: &Value| {
        item.as_str()
            .or_else(|| item["name"].as_str())
            .map(str::to_owned)
    };
    items.filter_map(name).collect()
}

/// The `/loom` and `/loom-agents` entries of each list, then every list.
fn command_lists(out: &[(Duration, String)]) -> String {
    let values: Vec<Value> = out
        .iter()
        .filter_map(|(_, l)| serde_json::from_str(l).ok())
        .collect();
    let answer = values.iter().find(|v| is_answer(v));
    let init = values.iter().find(|v| is_kind("init")(v));
    let lists = [
        (
            "initialize commands",
            names(answer.and_then(|v| v.pointer("/response/response/commands"))),
        ),
        ("system/init skills", names(init.map(|v| &v["skills"]))),
        (
            "system/init slash_commands",
            names(init.map(|v| &v["slash_commands"])),
        ),
    ];
    let loom = |n: &&String| {
        ["loom", "loom-agents"]
            .iter()
            .any(|w| *n == w || n.ends_with(&format!(":{w}")))
    };
    let mut text = String::new();
    for (label, list) in &lists {
        let found: Vec<&String> = list.iter().filter(loom).collect();
        text.push_str(&format!(
            "{label}: {} entries; /loom or /loom-agents: {found:?}\n",
            list.len()
        ));
    }
    for (label, list) in &lists {
        text.push_str(&format!("\n{label}:\n{}\n", list.join("\n")));
    }
    text
}

/// Starts the side process for `variant` against the live session; returns it with its
/// argv and the settings copy written beside the session's.
fn launch(p: &Probe, variant: &Variant) -> (Side, Vec<std::ffi::OsString>) {
    let session =
        live_argv(p.pid).unwrap_or_else(|e| p.fail(&format!("cannot read the argv: {e}")));
    let env = live_env(p.pid).unwrap_or_else(|e| p.fail(&format!("cannot read the env: {e}")));
    let cwd = fs::read_link(format!("/proc/{}/cwd", p.pid)).unwrap_or_else(|_| p.root.clone());
    let live =
        settings_of(&session).unwrap_or_else(|| p.fail("the session argv has no --settings"));
    let text = fs::read_to_string(&live)
        .unwrap_or_else(|e| p.fail(&format!("cannot read {}: {e}", live.display())));
    let value: Value = serde_json::from_str(&text).unwrap_or_default();
    let sibling = live.with_file_name(format!("settings.side-{}.json", variant.label));
    let settings = side_settings(&value, variant.plugins.as_ref());
    fs::write(&sibling, serde_json::to_string_pretty(&settings).unwrap()).unwrap();
    let argv = side_argv(&session, &sibling, variant.extra);
    let stderr = p.base.join(format!("side-{}.stderr", variant.label));
    let side = Side::spawn(&argv, &env, &cwd, &stderr, p.seen.clone());
    let side = side.unwrap_or_else(|e| p.fail(&format!("cannot start the side process: {e}")));
    (side, argv)
}

/// What one run saw.
struct Run {
    argv: Vec<String>,
    sent: std::io::Result<()>,
    answered: Option<Duration>,
    init: Option<Duration>,
    changed: Option<Option<Duration>>,
    status: String,
    out: Vec<(Duration, String)>,
}

/// Sends `initialize` and waits 20 s for its answer, 5 s more for `system/init`, and with
/// `add_skill` writes a skill file and waits 10 s for `commands_changed`; then kills and reaps
/// the process.
fn drive(p: &Probe, variant: &Variant) -> Run {
    let (mut side, argv) = launch(p, variant);
    let sent = side.send(INITIALIZE);
    let answered = side.until(secs(20).min(p.left()), is_answer);
    let seen_init = side
        .out
        .iter()
        .find(|(_, l)| l.contains(r#""subtype":"init""#));
    let seen_init = seen_init.map(|(at, _)| *at);
    let init = seen_init.or_else(|| side.until(secs(5).min(p.left()), is_kind("init")));
    let changed = variant.add_skill.then(|| {
        skill(
            &p.home,
            "probe-added",
            &[],
            "Reply with the single word ADDED.",
        );
        side.until(secs(10).min(p.left()), is_kind("commands_changed"))
    });
    let status = side.stop();
    let argv = argv.iter().map(|w| w.to_string_lossy().into_owned());
    Run {
        argv: argv.collect(),
        sent,
        answered,
        init,
        changed,
        status,
        out: std::mem::take(&mut side.out),
    }
}

/// Runs `variant` against the live session and notes everything, the run's stdout as a
/// `probe-notes` file; a run with no output at all fails the probe at `finish`.
pub fn run_side(p: &mut Probe, variant: &Variant) {
    let before = Snapshot::take(p);
    let run = drive(p, variant);
    let after = Snapshot::take(p);
    let kinds = run.out.iter().map(|(at, line)| {
        let value = serde_json::from_str(line).unwrap_or_default();
        format!("{at:?} {}", kind(&value))
    });
    let stderr = lines_of(&p.base.join(format!("side-{}.stderr", variant.label)));
    let body = format!(
        "argv:\n{}\n\nstdin: {INITIALIZE} (sent: {:?}); no user message\n\n\
         initialize answered: {:?}\nsystem/init: {:?}\n\
         commands_changed after adding a skill: {:?}\n{}\n\n{}\n\n\
         stdout line kinds:\n{}\n\n{}\nstderr, last 40 lines:\n{}",
        run.argv.join(" "),
        run.sent,
        run.answered,
        run.init,
        run.changed,
        run.status,
        before.compare(&after),
        joined(kinds),
        command_lists(&run.out),
        joined(stderr.iter().rev().take(40).rev())
    );
    p.out
        .note(&format!("Side process: {}", variant.label), &body);
    let lines: Vec<String> = run.out.into_iter().map(|(_, line)| line).collect();
    let file = format!("probe-notes/{}.side-{}.jsonl", p.out.name(), variant.label);
    p.out.fixture(&file, &lines);
    if lines.is_empty() {
        p.miss(&format!("side process {}: no stdout at all", variant.label));
    }
}
