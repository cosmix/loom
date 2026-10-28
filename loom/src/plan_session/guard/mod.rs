//! The PreToolUse guard of planning sessions: decides every file-tool call against the live tree.
//!
//! Section 12, "3. The planning hook": the third layer under the OS sandbox and the permission
//! rules. It only ever denies or stays silent: an `allow` answer from a PreToolUse hook skips
//! Claude Code's permission rules, voiding layer 2 and the holder's prompt. Every failure is a
//! deny, and a write decision scans the locked set afresh, so a plan locked mid-session is
//! refused at once.

mod paths;
mod search;

#[cfg(test)]
mod tests;
#[cfg(test)]
mod tests_cases;
#[cfg(test)]
mod tests_reads;
#[cfg(test)]
mod tests_reads_fixture;

use std::io::Read;
use std::os::unix::fs::MetadataExt;
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::path::{Path, PathBuf};

use anyhow::{ensure, Context, Result};
use serde_json::{json, Value};

use self::paths::{inside, normalize, ReadRules};
use crate::fs::safe_read::read_to_string_bounded;
use crate::plan_session::kind::SessionKind;
use crate::plan_session::sandbox::filesystem::OMISSIONS;
use crate::plan_session::sandbox::PRE_TOOL_USE;
use crate::planning::paths::{classify_write, symlink_component, DocArea, LockReason, LockedSet};
use crate::planning::runtime::DRAFTS_DIR;
use crate::planning::slug::PlanState;

/// The `guard.json` schema this guard reads.
const SCHEMA: u32 = 1;
/// The policy's name in the session directory (`SessionDir::guard`).
const GUARD_FILE: &str = "guard.json";
/// `guard.json` is the host's and small; anything larger is not one.
const MAX_POLICY_BYTES: usize = 64 * 1024;
/// The payload bound of `commands/hook/worker_brief.rs` (`MAX_STDIN_BYTES`): more is a deny.
const MAX_STDIN_BYTES: u64 = 1024 * 1024;
const DOC: &str = "doc";

/// The guard's only input besides the payload and the live tree, written beside
/// `settings.json` by `sandbox::write_session_sandbox`.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct GuardPolicy {
    /// Always the private `SCHEMA` constant (1).
    pub schema: u32,
    /// Serialised as `{"kind": {"kind": ...}}`, never flattened.
    pub kind: SessionKind,
    pub project_root: PathBuf,
    pub runtime_dir: PathBuf,
    pub session_dir: PathBuf,
    pub home: PathBuf,
}

/// The guard's answer to one tool call. There is no allow: [`Decision::Defer`] leaves the call
/// to the permission rules.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision {
    Defer,
    Deny(String),
}

/// Read `<session_dir>/guard.json` (bounded, no-follow) and refuse any schema but 1, a relative
/// path, or a policy naming another session directory.
pub fn load_policy(session_dir: &Path) -> Result<GuardPolicy> {
    let guard = session_dir.join(GUARD_FILE);
    let text = read_to_string_bounded(session_dir, Path::new(GUARD_FILE), MAX_POLICY_BYTES)
        .with_context(|| format!("cannot read {}", guard.display()))?;
    let policy: GuardPolicy = serde_json::from_str(&text)
        .with_context(|| format!("{} is not a guard policy", guard.display()))?;
    ensure!(
        policy.schema == SCHEMA,
        "guard.json schema {} is not {SCHEMA}",
        policy.schema
    );
    let paths = [
        ("project_root", &policy.project_root),
        ("runtime_dir", &policy.runtime_dir),
        ("session_dir", &policy.session_dir),
        ("home", &policy.home),
    ];
    for (key, path) in paths {
        ensure!(path.is_absolute(), "guard.json {key} is not absolute");
    }
    ensure!(
        policy.session_dir.as_path() == session_dir,
        "guard.json belongs to {}",
        policy.session_dir.display()
    );
    Ok(policy)
}

/// Decide one PreToolUse payload: steps 3-6 of the guard's decision list (plan 6 common brief,
/// "The guard"). A tool outside the matcher list is deferred.
pub fn decide(policy: &GuardPolicy, payload: &Value) -> Decision {
    match evaluate(policy, payload) {
        Ok(()) => Decision::Defer,
        Err(reason) => Decision::Deny(format!("loom plan: {reason}")),
    }
}

/// The one-line deny answer; `reason` goes through `serde_json`, never string formatting.
pub fn render_deny(reason: &str) -> String {
    json!({
        "hookSpecificOutput": {
            "hookEventName": PRE_TOOL_USE,
            "permissionDecision": "deny",
            "permissionDecisionReason": reason,
        }
    })
    .to_string()
}

/// The guard's answer to the payload on `stdin`: `None` prints nothing, `Some` is one deny line
/// (without a trailing newline). Every failure, a panic included, is a deny.
pub fn run_pre_tool_use(session_dir: &Path, stdin: impl Read) -> Option<String> {
    catch_unwind(AssertUnwindSafe(|| answer(session_dir, stdin)))
        .unwrap_or_else(|_| Some(render_deny("loom plan: the planning guard failed")))
}

fn answer(session_dir: &Path, stdin: impl Read) -> Option<String> {
    let deny = |why: String| Some(render_deny(&format!("loom plan: {why}")));
    let payload = match read_payload(stdin) {
        Ok(payload) => payload,
        Err(why) => return deny(why),
    };
    // Another event's payload is not this hook's to answer (the spool lesson,
    // `mistakes/spurious-waiting-for-input.md`).
    match payload.get("hook_event_name") {
        None | Some(Value::Null) => {}
        Some(Value::String(event)) if event == PRE_TOOL_USE => {}
        Some(Value::String(_)) => return None,
        Some(_) => return deny("hook_event_name is not a string".into()),
    }
    let policy = match load_policy(session_dir) {
        Ok(policy) => policy,
        Err(error) => return deny(format!("cannot load the guard policy: {error:#}")),
    };
    match decide(&policy, &payload) {
        Decision::Defer => None,
        Decision::Deny(reason) => Some(render_deny(&reason)),
    }
}

fn read_payload(stdin: impl Read) -> Result<Value, String> {
    let mut raw = Vec::new();
    stdin
        .take(MAX_STDIN_BYTES + 1)
        .read_to_end(&mut raw)
        .map_err(|error| format!("cannot read the payload: {error}"))?;
    if raw.len() as u64 > MAX_STDIN_BYTES {
        return Err("the payload is over 1 MiB".into());
    }
    serde_json::from_slice(&raw).map_err(|error| format!("the payload is not JSON: {error}"))
}

/// What a file tool does with the path it names.
enum Access {
    Write,
    Read,
    Search,
}

fn evaluate(policy: &GuardPolicy, payload: &Value) -> Result<(), String> {
    let tool = payload
        .get("tool_name")
        .and_then(Value::as_str)
        .ok_or("the payload has no tool_name")?;
    let access = match tool {
        "Edit" | "Write" | "MultiEdit" | "NotebookEdit" => Access::Write,
        "Read" => Access::Read,
        "Grep" | "Glob" => Access::Search,
        _ => return Ok(()),
    };
    let input = payload
        .get("tool_input")
        .filter(|input| input.is_object())
        .ok_or("tool_input is not an object")?;
    let cwd = match payload.get("cwd") {
        None | Some(Value::Null) => None,
        Some(Value::String(cwd)) => Some(Path::new(cwd.as_str())),
        Some(_) => return Err("cwd is not a string".into()),
    };
    let path = paths::tool_path(tool, input, cwd, &policy.project_root)?;
    match access {
        Access::Write => check_write(policy, &path),
        Access::Read => ReadRules::new(policy)?.check(&path, false),
        Access::Search => {
            ReadRules::new(policy)?.check(&path, true)?;
            search::check_search(&policy.project_root, tool, input, &path)
        }
    }
}

/// Section 12's write table for the policy's kind; anything no row allows is denied.
fn check_write(policy: &GuardPolicy, path: &Path) -> Result<(), String> {
    let root = &policy.project_root;
    let pressure = matches!(policy.kind, SessionKind::Pressure { .. });
    if inside(path, &root.join(DOC)) {
        return check_doc_write(policy, relative(path, root)?);
    }
    if path == root.join(OMISSIONS).as_path() {
        if !pressure {
            return Err(format!("only a pressure session writes {OMISSIONS}"));
        }
        return confined_file(root, Path::new(OMISSIONS));
    }
    let runtime = &policy.runtime_dir;
    let drafts = runtime.join(DRAFTS_DIR);
    if inside(path, &drafts) && path != drafts.as_path() {
        if pressure {
            return Err("a pressure session does not write runtime drafts".into());
        }
        return confined_file(runtime, relative(path, runtime)?);
    }
    // The project and runtime rows above decide every path under them; the planning runtime root
    // (every project's runtime) and the home are never temporary writes, even when the temporary
    // directory contains them (a test tree under it).
    let planning_root = runtime.parent().unwrap_or(runtime);
    let reserved = [
        root.as_path(),
        planning_root,
        policy.session_dir.as_path(),
        policy.home.as_path(),
    ];
    if let Some(tmp) = claude_tmp_dir() {
        let below = inside(path, &tmp) && path != tmp.as_path();
        if below && !reserved.iter().any(|base| inside(path, base)) {
            check_private_dir(&tmp)?;
            return confined_file(&tmp, relative(path, &tmp)?);
        }
    }
    Err(outside(path))
}

fn outside(path: &Path) -> String {
    format!("{} is outside the session's write set", path.display())
}

/// A write under `<root>/doc/`: symlink and file-type checks, then `classify_write` over a
/// fresh scan of the locked set, then the kind's rule.
fn check_doc_write(policy: &GuardPolicy, rel: &Path) -> Result<(), String> {
    let root = &policy.project_root;
    confined_file(root, rel)?;
    let locked = LockedSet::scan(root).map_err(|e| format!("locked set scan: {e:#}"))?;
    match classify_write(&locked, rel) {
        DocArea::Locked(reason) => Err(locked_text(rel, &reason)),
        DocArea::Knowledge => Err("doc/loom/knowledge is written with `loom knowledge`".into()),
        DocArea::Summaries => {
            Err("doc/loom/summaries is written with `loom plan summary seal`".into())
        }
        DocArea::OutsideDoc => Err(outside(rel)),
        DocArea::Doc => match &policy.kind {
            SessionKind::Authoring => Ok(()),
            SessionKind::Summary => Err("a summary session writes only runtime drafts".into()),
            SessionKind::Pressure { plan } => {
                let bundle = policy.kind.pressure_bundle();
                let in_bundle = bundle.is_some_and(|b| inside(rel, &b) && rel != b.as_path());
                if rel == plan.as_path() || in_bundle {
                    return Ok(());
                }
                Err(format!(
                    "a pressure session writes only {}, its brief bundle and {OMISSIONS}",
                    plan.display()
                ))
            }
        },
    }
}

/// `rel` below `base` reaches no symlink and, when it exists, is a regular file with one link
/// (`symlink_metadata` reads no content; `symlink_component` cleared every component).
fn confined_file(base: &Path, rel: &Path) -> Result<(), String> {
    match symlink_component(base, rel) {
        Ok(None) => {}
        Ok(Some(link)) => return Err(format!("symlink in path: {}", link.display())),
        Err(error) => return Err(format!("cannot check {}: {error:#}", rel.display())),
    }
    let target = base.join(rel);
    match std::fs::symlink_metadata(&target) {
        Ok(meta) if meta.is_file() && meta.nlink() == 1 => Ok(()),
        Ok(_) => Err(format!("{} is not a regular file", target.display())),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(error) => Err(format!("cannot inspect {}: {error}", target.display())),
    }
}

/// Claude Code's per-user temporary directory, `<tmp>/claude-<uid>`: `<tmp>` is `TMPDIR` when
/// set and absolute, else `/tmp`. Claude Code 2.1.311 exports `TMPDIR=/tmp/claude-<uid>` to the
/// commands it runs, so a `TMPDIR` that already ends in `claude-<uid>` is the directory itself.
fn claude_tmp_dir() -> Option<PathBuf> {
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let name = format!("claude-{}", unsafe { libc::getuid() });
    let base = std::env::var_os("TMPDIR")
        .map(PathBuf::from)
        .filter(|tmp| tmp.is_absolute())
        .unwrap_or_else(|| PathBuf::from("/tmp"));
    let base = normalize(&base).ok()?;
    if base.file_name() == Some(std::ffi::OsStr::new(&name)) {
        return Some(base);
    }
    Some(base.join(name))
}

/// The temporary directory must be a real directory of this user: `/tmp` is shared, and a
/// directory another user planted there is not Claude Code's.
fn check_private_dir(dir: &Path) -> Result<(), String> {
    let meta = std::fs::symlink_metadata(dir)
        .map_err(|error| format!("cannot inspect {}: {error}", dir.display()))?;
    // SAFETY: `getuid` has no preconditions and cannot fail.
    let uid = unsafe { libc::getuid() };
    if !meta.is_dir() || meta.uid() != uid {
        return Err(format!("{} is not this user's directory", dir.display()));
    }
    Ok(())
}

fn relative<'p>(path: &'p Path, base: &Path) -> Result<&'p Path, String> {
    path.strip_prefix(base)
        .map_err(|_| format!("{} is not under {}", path.display(), base.display()))
}

/// `doc/plans/IN_PROGRESS-PLAN-x.md is locked (running plan)`.
fn locked_text(rel: &Path, reason: &LockReason) -> String {
    let why = match reason {
        LockReason::PlanState(PlanState::Running) => "running plan".to_owned(),
        LockReason::PlanState(PlanState::Done) => "done plan".to_owned(),
        LockReason::PlanState(PlanState::Retired) => "retired plan".to_owned(),
        LockReason::PlanState(PlanState::Written) => "plan".to_owned(),
        LockReason::Archive => "archived plan".to_owned(),
        LockReason::RunReview => "run review".to_owned(),
        LockReason::BriefOfLockedPlan { slug } => format!("brief of the locked plan {slug}"),
    };
    format!("{} is locked ({why})", rel.display())
}
