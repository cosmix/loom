//! Layer 2 of a planning session: the `permissions` rules per kind, and the `hooks` object.
//!
//! Path rules use the absolute `//` form, `Edit(/` + absolute path + `)`: a single leading `/`
//! resolves against the settings file's directory (`architecture/execution-containment.md`,
//! "Sandbox-List vs Permission-Rule Path Syntax"). Only `Edit(...)` binds file writes; no
//! `Write(...)` rule and no `Read(...)` deny is ever emitted
//! (`mistakes/sandbox-write-rules-inert.md`, `concerns/sandbox-and-confinement-gaps.md`). No
//! `Edit` deny names `doc/loom/`: it would also be an OS write deny and block `loom knowledge`
//! and `loom plan summary seal`. An `Edit` ask is the form `doc/loom/` takes: it prompts for a file
//! tool and leaves the OS write set alone.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

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
use crate::plan_session::kind::SessionKind;
use crate::plan_session::scope::SessionScope;
use crate::planning::paths::LockReason;
use crate::planning::slug::plan_slug;

pub(super) const ASK_USER_QUESTION: &str = "AskUserQuestion";
/// Tools that address another local Claude Code session, a self-approval route; pressure and
/// team-scoped sessions keep them for their teammates.
const PEER_TOOLS: [&str; 2] = ["SendMessage", "ListAgents"];
const WEB_SEARCH: &str = "WebSearch";
const WEB_FETCH: &str = "WebFetch";
/// Open and Team authoring sessions rebuild summaries after drafting documents.
const SUMMARY_BASH: &str = "Bash(loom plan summary *)";
/// Kickoff sessions may inspect summary status while summary writers need holder approval.
const SUMMARY_STATUS_BASH: &str = "Bash(loom plan summary status*)";
const KNOWLEDGE_BASH: &str = "Bash(loom knowledge *)";
/// Ask rules take precedence over allows, so summary status must stay outside this list.
const KICKOFF_WRITER_ASKS: [&str; 4] = [
    KNOWLEDGE_BASH,
    "Bash(loom plan summary seal*)",
    "Bash(loom plan summary brief*)",
    "Bash(loom plan summary release*)",
];
/// Read-only authoring commands stay allowed in every scope.
const AUTHORING_READ_ONLY_BASH: [&str; 3] = [
    "Bash(loom plan verify *)",
    "Bash(loom plan check*)",
    "Bash(loom map *)",
];
/// Open authoring permits both writers; Team and Kickoff need approval for knowledge writes.
const AUTHORING_DOC_WRITER_BASH: [&str; 2] = [KNOWLEDGE_BASH, SUMMARY_BASH];
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

/// The permission mode shared by the session settings and launch command.
pub fn permission_mode(kind: &SessionKind) -> &'static str {
    match kind {
        SessionKind::Authoring | SessionKind::Pressure { .. } => "auto",
        SessionKind::Summary => "default",
    }
}

/// The `permissions` object of the session.
pub(super) fn permissions(build: &Build<'_>) -> Result<Value> {
    let (allow, ask, deny) = match &build.target {
        Target::Authoring => authoring(build)?,
        Target::Summary => summary(build),
        Target::Pressure { plan, bundle } => pressure(build, plan, bundle)?,
    };
    Ok(json!({
        "defaultMode": permission_mode(build.inputs.kind),
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
    // later matches no rule: it reaches the classifier, and the guard decides it.
    for file in doc_entries(build, EntryKind::File)? {
        allow.push(path_rule("Edit", &doc.join(file), ""));
    }
    allow.extend(runtime_rules(build));
    let mut ask = vec![WEB_FETCH.to_owned()];
    match build.scope {
        SessionScope::Open => allow.extend(AUTHORING_DOC_WRITER_BASH.map(String::from)),
        SessionScope::Team { .. } => {
            allow.push(SUMMARY_BASH.to_owned());
            ask.push(KNOWLEDGE_BASH.to_owned());
        }
        SessionScope::Kickoff(_) => {
            allow.push(SUMMARY_STATUS_BASH.to_owned());
            ask.extend(KICKOFF_WRITER_ASKS.map(String::from));
        }
    }
    ask.extend(file_tool_asks(build, &[])?);
    allow.extend(AUTHORING_READ_ONLY_BASH.map(String::from));
    allow.push(WEB_SEARCH.to_owned());
    let mut deny = base_denies(!build.scope.peer_tools());
    deny.extend(locked_denies(build)?);
    Ok((allow, ask, deny))
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
    allow.extend(
        AUTHORING_DOC_WRITER_BASH
            .into_iter()
            .chain(AUTHORING_READ_ONLY_BASH)
            .map(String::from),
    );
    allow.push(WEB_SEARCH.to_owned());
    let mut ask = vec![WEB_FETCH.to_owned()];
    // An ask outranks an allow, so the notes file the session may write stays out of the list.
    ask.extend(file_tool_asks(build, &[OMISSIONS])?);
    Ok((allow, ask, base_denies(false)))
}

/// The `Edit` asks of an auto-mode session, which keep file-tool writes outside `doc/` and under
/// `doc/loom/` with the holder when the guard does not answer: the classifier decides a path no
/// rule lists. `doc/loom/**`, then each top-level entry but `doc` and `allowed`, enumerated now,
/// a directory with everything beneath it. An ask outranks an allow, so none covers the root or
/// `doc`. A name holding rule syntax gets no rule (a directory named `*` would ask for every
/// directory): layer 1 lists it literally and the guard decides it.
fn file_tool_asks(build: &Build<'_>, allowed: &[&str]) -> Result<Vec<String>> {
    let root = build.inputs.project_root;
    let mut asks = vec![path_rule("Edit", &root.join("doc/loom"), "/**")];
    for (name, kind) in rule_safe_entries(build, Path::new(""))? {
        if name != "doc" && !allowed.contains(&name.as_str()) {
            let suffix = if kind == EntryKind::Directory {
                "/**"
            } else {
                ""
            };
            asks.push(path_rule("Edit", &root.join(name), suffix));
        }
    }
    Ok(asks)
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
    let entries = rule_safe_entries(build, Path::new("doc"))?;
    let of_kind = entries.into_iter().filter(|(_, entry)| *entry == kind);
    Ok(of_kind.map(|(name, _)| name).collect())
}

/// The entries of the real directory `dir` whose names can stand in a permission rule, sorted by
/// name, listed without following a link.
fn rule_safe_entries(build: &Build<'_>, dir: &Path) -> Result<Vec<(String, EntryKind)>> {
    let mut entries: Vec<_> = build
        .tree
        .real_dir_entries(dir)?
        .into_iter()
        .filter_map(|(name, kind)| Some((name.into_string().ok()?, kind)))
        .filter(|(name, _)| rule_syntax_chars(name).is_empty())
        .collect();
    entries.sort_by(|(a, _), (b, _)| a.cmp(b));
    Ok(entries)
}

/// The authoring `Edit` denies of the locked set: each existing locked plan file, the run
/// reviews, the archive, and the bundle of every locked plan, archived ones included, whether
/// or not the bundle directory exists. A name holding rule syntax is left to layer 1 (which
/// lists it literally) and the guard.
fn locked_denies(build: &Build<'_>) -> Result<Vec<String>> {
    let root = build.inputs.project_root;
    let plan_files = build.locked.locked_paths();
    let files = plan_files
        .iter()
        .filter(|rel| {
            matches!(
                build.locked.lock_reason(rel),
                Some(LockReason::PlanState(_))
            )
        })
        .filter(|rel| rule_syntax_chars(&rel.to_string_lossy()).is_empty());
    let mut deny: Vec<String> = files
        .map(|rel| path_rule("Edit", &root.join(rel), ""))
        .collect();
    let existing_bundles = locked_bundles(build)?;
    for home in build.tree.plan_homes()? {
        let plans = root.join(&home);
        deny.push(path_rule("Edit", &plans.join("REVIEW-*.md"), ""));
        deny.push(path_rule("Edit", &plans.join("archive"), "/**"));
        let mut bundles = locked_plan_bundles(build, &home)?;
        let briefs = home.join("briefs");
        bundles.extend(
            existing_bundles
                .iter()
                .filter(|(dir, _)| dir.parent() == Some(briefs.as_path()))
                .map(|(_, slug)| briefs.join(slug)),
        );
        deny.extend(
            bundles
                .iter()
                .filter(|rel| rule_syntax_chars(&rel.to_string_lossy()).is_empty())
                .map(|rel| path_rule("Edit", &root.join(rel), "/**")),
        );
    }
    Ok(deny)
}

/// Include missing bundles of locked plans in the home and its archive. `locked_paths`
/// collapses archives to their directory, so list their plan names without following links.
fn locked_plan_bundles(build: &Build<'_>, home: &Path) -> Result<BTreeSet<PathBuf>> {
    let mut bundles = BTreeSet::new();
    for dir in [home.to_path_buf(), home.join("archive")] {
        for (name, kind) in build.tree.real_dir_entries(&dir)? {
            let Some(name) = name.to_str().filter(|_| kind == EntryKind::File) else {
                continue;
            };
            let Some(slug) = plan_slug(name) else {
                continue;
            };
            let bundle = home.join("briefs").join(slug);
            // The locked set is the authority, including same-slug conflicts across homes.
            if matches!(
                build.locked.lock_reason(&bundle),
                Some(LockReason::BriefOfLockedPlan { .. })
            ) {
                bundles.insert(bundle);
            }
        }
    }
    Ok(bundles)
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
