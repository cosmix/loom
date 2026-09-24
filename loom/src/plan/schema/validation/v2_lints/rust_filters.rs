//! Rust test filters that select nothing (G5): `cargo test` given a module
//! path (`--lib a::b::`, or any filter containing `::`) that no module in the
//! source graph's base layer for HEAD answers to, and that no path the stage
//! declares could create. cargo then runs zero tests and exits 0.
//!
//! The base layer is only read (`GraphStore::load_base`); nothing here builds,
//! refreshes or publishes a graph.

use std::ffi::OsStr;
use std::path::Path;

use crate::context::graph_store::{GraphLayer, GraphStore};
use crate::context::resolve::node_names;
use crate::context::store::ContextStore;
use crate::fs::work_dir::WorkDir;
use crate::plan::schema::StageDefinition;

use super::super::shell_lex::Word;
use super::{visit_stage_argvs, LintContext, LintFinding};

/// `cargo test` options that take the next word as their value.
const CARGO_VALUE_OPTIONS: &str = "--manifest-path -p --package --test --bin --example --bench \
     --features -F --target --target-dir -j --jobs --color --message-format --profile --config \
     -Z --exclude --lockfile-path";

/// libtest options (after `--`) that take the next word as their value.
const LIBTEST_VALUE_OPTIONS: &str =
    "--skip --test-threads --format --color --logfile --shuffle-seed -Z";

/// A module-path filter one stage command passes to `cargo test`.
struct ModuleFilter<'a> {
    stage: &'a StageDefinition,
    /// The finding's message, used only when nothing matches.
    message: String,
    /// The filter's module segments, outermost first.
    module: Vec<String>,
}

/// Push a warning per filter that matches nothing; return a note instead when
/// filters exist but no base layer can be read for them.
pub(super) fn check(ctx: &LintContext<'_>, out: &mut Vec<LintFinding>) -> Option<String> {
    let filters = module_filters(ctx);
    if filters.is_empty() {
        return None;
    }
    let layer = match head_base_layer(ctx.repo_root) {
        Ok(layer) => layer,
        Err(reason) => {
            return Some(format!(
                "Rust test filters were not checked against the source graph: {reason}"
            ));
        }
    };
    for filter in filters {
        let module = filter.module.as_slice();
        if !graph_has_module(&layer, module) && !stage_could_create(filter.stage, module) {
            out.push(LintFinding::in_stage(filter.stage, filter.message, false));
        }
    }
    None
}

fn module_filters<'a>(ctx: &LintContext<'a>) -> Vec<ModuleFilter<'a>> {
    let mut filters = Vec::new();
    for stage in &ctx.metadata.loom.stages {
        visit_stage_argvs(stage, &mut |command, argv| {
            for filter in test_filters(argv) {
                let Some(module) = module_path(&filter) else {
                    continue;
                };
                let problem = format!(
                    "filters `cargo test` on `{filter}`, but no module `{}` exists in the source \
                     graph for HEAD and no path in the stage's files/artifacts could create it; \
                     a filter that matches no test runs zero tests and exits 0",
                    module.join("::")
                );
                let message = command.describe(&problem);
                filters.push(ModuleFilter {
                    stage,
                    message,
                    module,
                });
            }
        });
    }
    filters
}

/// The test-name filters of a `cargo test` argv that contain `::`, before and
/// after `--`, skipping option values.
fn test_filters(argv: &[&Word]) -> Vec<String> {
    let Some((first, args)) = argv.split_first() else {
        return Vec::new();
    };
    let mut words = args
        .iter()
        .map(|word| word.value.as_str())
        .skip_while(|word| word.starts_with('-') || word.starts_with('+'));
    if first.command_name() != "cargo" || words.next() != Some("test") {
        return Vec::new();
    }
    let mut value_options = CARGO_VALUE_OPTIONS;
    let mut skip_value = false;
    let mut filters = Vec::new();
    for word in words {
        if std::mem::take(&mut skip_value) {
            continue;
        }
        if word == "--" {
            value_options = LIBTEST_VALUE_OPTIONS;
        } else if word.starts_with('-') {
            skip_value = value_options
                .split_whitespace()
                .any(|option| option == word);
        } else if word.contains("::") {
            filters.push(word.to_string());
        }
    }
    filters
}

/// Module segments of a filter. A trailing `::` marks every segment as a
/// module; otherwise the last segment may name a test, or part of one, and is
/// dropped.
fn module_path(filter: &str) -> Option<Vec<String>> {
    let mut segments: Vec<&str> = filter.split("::").collect();
    if !filter.ends_with("::") {
        segments.pop();
    }
    segments.retain(|segment| !segment.is_empty());
    (!segments.is_empty()).then(|| segments.into_iter().map(str::to_string).collect())
}

/// Whether every segment of the module path occurs in the graph. cargo matches
/// a filter as a substring of the full test path, so each segment may match
/// loosely, and all of them must: an outer typo fails even when the innermost
/// name exists.
fn graph_has_module(layer: &GraphLayer, module: &[String]) -> bool {
    module.iter().all(|name| graph_has_segment(layer, name))
}

/// Whether some node answers to `segment`: a node name contains it,
/// case-insensitively (the substring fallback `loom map --find-all` resolves
/// names with), or a directory name or file stem on the node's path equals it.
fn graph_has_segment(layer: &GraphLayer, segment: &str) -> bool {
    let needle = segment.to_lowercase();
    let exact = OsStr::new(segment);
    layer.nodes().any(|node| {
        let dirs = node.path.parent().into_iter().flat_map(Path::iter);
        node_names(node)
            .iter()
            .any(|candidate| candidate.to_lowercase().contains(&needle))
            || dirs.chain(node.path.file_stem()).any(|part| part == exact)
    })
}

/// Whether a `files:`/`artifacts:` entry could create the module `a::b`:
/// `a/b.rs`, `a/b/mod.rs`, or anything under `a/b/`, below any directory
/// (a filter need not start at the crate root).
fn stage_could_create(stage: &StageDefinition, module: &[String]) -> bool {
    let relative = module.join("/");
    stage
        .files
        .iter()
        .chain(&stage.artifacts)
        .any(|entry| entry_could_create(entry, &relative))
}

fn entry_could_create(entry: &str, relative: &str) -> bool {
    let entry = entry.trim_start_matches("./");
    let glob_start = entry.find(['*', '?', '[']).unwrap_or(entry.len());
    let literal = &entry[..glob_start];
    if names_module_dir(literal, relative) {
        return true;
    }
    let Ok(pattern) = glob::Pattern::new(entry) else {
        return false;
    };
    let files = [format!("{relative}.rs"), format!("{relative}/mod.rs")];
    directory_prefixes(literal).any(|prefix| {
        files
            .iter()
            .any(|file| pattern.matches(&format!("{prefix}{file}")))
    })
}

/// Whether the literal part of an entry is the module's directory or lies
/// under it, at a path-component boundary.
fn names_module_dir(literal: &str, relative: &str) -> bool {
    let bare = literal.trim_end_matches('/');
    let dir = format!("{relative}/");
    bare == relative
        || bare.ends_with(&format!("/{relative}"))
        || literal.starts_with(&dir)
        || literal.contains(&format!("/{dir}"))
}

/// `""` and every prefix of `literal` that ends at a `/`.
fn directory_prefixes(literal: &str) -> impl Iterator<Item = &str> {
    let prefixes = literal
        .match_indices('/')
        .map(move |(idx, _)| &literal[..=idx]);
    std::iter::once("").chain(prefixes)
}

/// The base layer for the repository's HEAD, read without building one.
fn head_base_layer(repo_root: Option<&Path>) -> Result<GraphLayer, String> {
    let root = repo_root.ok_or("no repository root was found")?;
    let head = crate::git::run_git_checked(&["rev-parse", "--verify", "HEAD"], root)
        .map_err(|_| "the repository has no HEAD commit".to_string())?;
    let short = head.get(..12).unwrap_or(&head);
    let work_dir = WorkDir::new(root).map_err(|error| format!("{error:#}"))?;
    let store = ContextStore::open(&work_dir).map_err(|error| format!("{error:#}"))?;
    match GraphStore::new(store.root(), work_dir.root()).load_base(&head) {
        Ok(Some(layer)) => Ok(layer),
        Ok(None) => Err(format!(
            "no source-graph base layer exists for HEAD {short} (`loom map` or `loom run` \
             builds one)"
        )),
        Err(error) => Err(format!(
            "the base layer for HEAD {short} could not be read ({error:#})"
        )),
    }
}
