//! Layer 2 of a planning session: the `permissions` rules per kind, and the `hooks` object.
//!
//! Path rules use the absolute `//` form, `Edit(/` + absolute path + `)`: a single leading `/`
//! resolves against the settings file's directory (`architecture/execution-containment.md`,
//! "Sandbox-List vs Permission-Rule Path Syntax"). Only `Edit(...)` binds file writes; no
//! `Write(...)` rule and no `Read(...)` deny is ever emitted
//! (`mistakes/sandbox-write-rules-inert.md`, `concerns/sandbox-and-confinement-gaps.md`). No
//! `Edit` deny names `doc/loom/`: it would also be an OS write deny and block `loom knowledge`
//! and `loom plan summary seal`.

use std::collections::BTreeSet;
use std::path::Path;

use anyhow::{bail, Result};
use serde_json::{json, Map, Value};

use super::filesystem::{locked_bundles, OMISSIONS};
use super::{
    hook_command, Build, HookSpec, Target, PRE_TOOL_USE, PRE_TOOL_USE_MATCHER,
    PRE_TOOL_USE_TIMEOUT_SECS,
};
use crate::fs::safe_read::EntryKind;
use crate::orchestrator::terminal::native::session_settings::contents::{
    SESSION_TOOL_DENIES, WORKFLOW_TOOL,
};
use crate::planning::paths::LockReason;
use crate::planning::slug::plan_slug;

pub(super) const ASK_USER_QUESTION: &str = "AskUserQuestion";
/// Tools that address another local Claude Code session, a self-approval route; a pressure
/// session keeps them because `/pressure` teammates need them.
const PEER_TOOLS: [&str; 2] = ["SendMessage", "ListAgents"];
const WEB_SEARCH: &str = "WebSearch";
const WEB_FETCH: &str = "WebFetch";
const SUMMARY_BASH: &str = "Bash(loom plan summary *)";
const AUTHORING_BASH: [&str; 5] = [
    "Bash(loom knowledge *)",
    SUMMARY_BASH,
    "Bash(loom plan verify *)",
    "Bash(loom plan check*)",
    "Bash(loom map *)",
];
/// Directories under `doc/` the authoring per-directory allow skips: `loom` is never allowed,
/// `product` and `plans` have their own rules.
const NOT_PER_DIR: [&str; 3] = ["loom", "product", "plans"];
/// Characters a permission-rule path treats as pattern syntax or rule delimiters.
const RULE_SYNTAX: [char; 10] = ['*', '?', '[', ']', '{', '}', '(', ')', '\\', '!'];

/// The tool denies of a kind: the stage capsule's four (`SESSION_TOOL_DENIES`), `Workflow`,
/// and, when `peer_tools`, `SendMessage` and `ListAgents`.
pub(super) fn tool_denies(peer_tools: bool) -> Vec<&'static str> {
    let peers = peer_tools.then_some(PEER_TOOLS).into_iter().flatten();
    SESSION_TOOL_DENIES
        .into_iter()
        .chain([WORKFLOW_TOOL])
        .chain(peers)
        .collect()
}

/// The `permissions` object of the session.
pub(super) fn permissions(build: &Build<'_>) -> Result<Value> {
    let (allow, ask, deny) = match &build.target {
        Target::Authoring => authoring(build)?,
        Target::Summary => summary(build),
        Target::Pressure { plan, bundle } => pressure(build, plan, bundle)?,
    };
    Ok(json!({
        "defaultMode": "default",
        "disableBypassPermissionsMode": "disable",
        "allow": allow,
        "ask": ask,
        "deny": deny,
    }))
}

type Rules = (Vec<String>, Vec<String>, Vec<String>);

fn authoring(build: &Build<'_>) -> Result<Rules> {
    let doc = build.inputs.project_root.join("doc");
    let mut allow = vec![
        path_rule("Edit", &doc.join("product"), "/**"),
        path_rule("Edit", &doc.join("plans"), "/**"),
    ];
    for dir in doc_entries(build, EntryKind::Directory)? {
        if !NOT_PER_DIR.contains(&dir.as_str()) {
            allow.push(path_rule("Edit", &doc.join(dir), "/**"));
        }
    }
    // Each top-level file by its literal path: `Edit` paths match with gitignore semantics, so
    // `doc/*` would match the directory `doc/loom` and so everything beneath it. A file created
    // later prompts the holder, and the guard still decides it.
    for file in doc_entries(build, EntryKind::File)? {
        allow.push(path_rule("Edit", &doc.join(file), ""));
    }
    allow.extend(runtime_rules(build));
    allow.extend(AUTHORING_BASH.map(String::from));
    allow.push(WEB_SEARCH.to_owned());
    let mut deny = base_denies(true);
    deny.extend(locked_denies(build)?);
    Ok((allow, vec![WEB_FETCH.to_owned()], deny))
}

fn summary(build: &Build<'_>) -> Rules {
    let mut allow = runtime_rules(build);
    allow.push(SUMMARY_BASH.to_owned());
    let mut deny = base_denies(true);
    deny.extend([WEB_SEARCH, WEB_FETCH].map(String::from));
    (allow, Vec::new(), deny)
}

fn pressure(build: &Build<'_>, plan: &Path, bundle: &Path) -> Result<Rules> {
    for path in [plan, bundle] {
        if !rule_syntax_chars(&path.to_string_lossy()).is_empty() {
            bail!(
                "pressure path {} holds permission-rule syntax",
                path.display()
            );
        }
    }
    let root = build.inputs.project_root;
    let marker = build.inputs.session_dir.marker();
    let mut allow = vec![
        path_rule("Edit", &root.join(plan), ""),
        path_rule("Edit", &root.join(bundle), "/**"),
        path_rule("Edit", &root.join(OMISSIONS), ""),
        // Exactly the command `commands/pressure/spawn.rs::completion_instruction` asks for.
        format!("Bash(touch {})", marker.display()),
    ];
    allow.extend(AUTHORING_BASH.map(String::from));
    allow.push(WEB_SEARCH.to_owned());
    Ok((allow, vec![WEB_FETCH.to_owned()], base_denies(false)))
}

/// `<tool>(/` + the absolute `abs` + `suffix` + `)`: the `//` comes from `abs`'s own root.
fn path_rule(tool: &str, abs: &Path, suffix: &str) -> String {
    format!("{tool}(/{}{suffix})", abs.display())
}

fn runtime_rules(build: &Build<'_>) -> Vec<String> {
    let rt = build.inputs.runtime;
    vec![
        path_rule("Edit", &rt.drafts, "/**"),
        path_rule("Read", &rt.drafts, "/**"),
        path_rule("Read", &rt.claims, "/**"),
    ]
}

fn base_denies(peer_tools: bool) -> Vec<String> {
    let tools = tool_denies(peer_tools)
        .into_iter()
        .chain([ASK_USER_QUESTION]);
    tools.map(String::from).collect()
}

/// The names of the existing entries of `kind` directly under a real `doc/`, sorted, listed
/// without following a link: a symlink or special entry is skipped, matching discovery, and so
/// is a name holding rule syntax (a directory named `*` would otherwise allow every directory,
/// `doc/loom` included).
fn doc_entries(build: &Build<'_>, kind: EntryKind) -> Result<Vec<String>> {
    let mut names: Vec<String> = build
        .tree
        .real_dir_entries(Path::new("doc"))?
        .into_iter()
        .filter(|(_, entry)| *entry == kind)
        .filter_map(|(name, _)| name.into_string().ok())
        .filter(|name| rule_syntax_chars(name).is_empty())
        .collect();
    names.sort();
    Ok(names)
}

/// The authoring `Edit` denies of the locked set: each existing locked plan file, the run
/// reviews, the archive, and the bundle of every locked plan, archived ones included, whether
/// or not the bundle directory exists. A name holding rule syntax is left to layer 1 (which
/// lists it literally) and the guard.
fn locked_denies(build: &Build<'_>) -> Result<Vec<String>> {
    let root = build.inputs.project_root;
    let plans = root.join("doc/plans");
    let mut plan_files = Vec::new();
    let mut slugs = BTreeSet::new();
    for rel in build.locked.locked_paths() {
        match build.locked.lock_reason(&rel) {
            Some(LockReason::PlanState(_)) => {
                let name = rel.file_name().and_then(|name| name.to_str());
                slugs.extend(name.and_then(plan_slug));
                plan_files.push(rel);
            }
            Some(LockReason::BriefOfLockedPlan { slug }) => {
                slugs.insert(slug);
            }
            _ => {}
        }
    }
    slugs.extend(locked_bundles(build)?.into_iter().map(|(_, slug)| slug));
    slugs.extend(archived_slugs(build)?);
    let files = plan_files
        .iter()
        .filter(|rel| rule_syntax_chars(&rel.to_string_lossy()).is_empty());
    let mut deny: Vec<String> = files
        .map(|rel| path_rule("Edit", &root.join(rel), ""))
        .collect();
    deny.push(path_rule("Edit", &plans.join("REVIEW-*.md"), ""));
    deny.push(path_rule("Edit", &plans.join("archive"), "/**"));
    let bundles = slugs
        .iter()
        .filter(|slug| rule_syntax_chars(slug).is_empty());
    deny.extend(bundles.map(|slug| path_rule("Edit", &plans.join("briefs").join(slug), "/**")));
    Ok(deny)
}

/// The slugs of the locked plan files directly in `doc/plans/archive/`. `locked_paths` lists the
/// archive directory in place of its files, and a bundle directory that does not exist is not
/// listed by `locked_bundles`, so without these an archived plan's bundle could be created.
/// A symlinked archive lists nothing here; layer 1 denies it and the guard decides beneath it.
fn archived_slugs(build: &Build<'_>) -> Result<Vec<String>> {
    let archive = Path::new("doc/plans/archive");
    let mut slugs = Vec::new();
    for (name, kind) in build.tree.real_dir_entries(archive)? {
        let Some(name) = name.to_str().filter(|_| kind == EntryKind::File) else {
            continue;
        };
        // `LockedSet` stays the only authority on what is locked: keep the slug when it says
        // the plan's bundle belongs to a locked plan.
        let Some(slug) = plan_slug(name) else {
            continue;
        };
        let bundle = Path::new("doc/plans/briefs").join(&slug);
        if matches!(
            build.locked.lock_reason(&bundle),
            Some(LockReason::BriefOfLockedPlan { .. })
        ) {
            slugs.push(slug);
        }
    }
    Ok(slugs)
}

/// The permission-rule syntax characters `text` holds, in order of appearance, without repeats.
pub(super) fn rule_syntax_chars(text: &str) -> String {
    let mut found = String::new();
    for c in text.chars().filter(|c| RULE_SYNTAX.contains(c)) {
        if !found.contains(c) {
            found.push(c);
        }
    }
    found
}

/// The `hooks` object: the guard's `PreToolUse` first, then each of `extra` in order.
pub(super) fn hooks(build: &Build<'_>, extra: &[HookSpec]) -> Result<Value> {
    if let Some(spec) = extra.iter().find(|spec| spec.event == PRE_TOOL_USE) {
        bail!(
            "an extra {} hook is refused: the planning guard owns that event",
            spec.event
        );
    }
    let guard = HookSpec {
        event: PRE_TOOL_USE,
        matcher: Some(PRE_TOOL_USE_MATCHER),
        timeout_secs: Some(PRE_TOOL_USE_TIMEOUT_SECS),
    };
    let (bin, dir) = (build.inputs.loom_bin, build.inputs.session_dir.path());
    let mut hooks = Map::new();
    for spec in std::iter::once(&guard).chain(extra) {
        let mut hook = json!({"type": "command", "command": hook_command(bin, spec.event, dir)});
        if let Some(timeout) = spec.timeout_secs {
            hook["timeout"] = json!(timeout);
        }
        let mut entry = json!({"hooks": [hook]});
        if let Some(matcher) = spec.matcher {
            entry["matcher"] = json!(matcher);
        }
        let list = hooks.entry(spec.event).or_insert_with(|| json!([]));
        if let Some(list) = list.as_array_mut() {
            list.push(entry);
        }
    }
    Ok(Value::Object(hooks))
}
