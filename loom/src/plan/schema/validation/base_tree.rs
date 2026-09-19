//! Evaluate simple search criteria against the committed tree.
//!
//! A criterion that already passes at HEAD cannot tell a stage that did its
//! work from one that did nothing. For each acceptance criterion that is a
//! single `rg` or `grep` over literal paths, this reproduces the command's exit
//! status against HEAD without running it, and warns when that status already
//! meets the criterion's expectation. The `regex` crate is the dialect `rg`
//! uses; a `grep` basic regex is evaluated only when it has no construct the
//! two dialects read differently.

use std::cell::OnceCell;
use std::collections::HashSet;
use std::path::{Component, Path};

use regex::{Regex, RegexBuilder};

use super::super::detect::detect_stage_type;
use super::super::types::{AcceptanceCriterion, StageDefinition, StageType};
use super::search_args::SearchArgs;
use super::shell_lex::{lex, Token, Word};
use crate::git::runner::{run_git, run_git_bool, run_git_checked};

/// The only short options an evaluable criterion may use.
const EVALUABLE_FLAGS: &str = "qFinwe";

/// Characters that make a GNU basic regex mean something else to `rg`.
const BRE_DIVERGENT: &str = "|(){}+?";

/// Base-tree findings for the `baseline` warning bucket, and a note when the
/// whole check was skipped.
#[derive(Debug, Default)]
pub(crate) struct BaseTreeReport {
    pub warnings: Vec<String>,
    pub note: Option<String>,
}

/// Evaluate every evaluable acceptance criterion of the plan's standard and
/// integration-verify stages against HEAD of `repo_root`.
pub(crate) fn check_base_tree(
    stages: &[StageDefinition],
    repo_root: Option<&Path>,
) -> BaseTreeReport {
    let Some(root) = repo_root else {
        return skipped("no git repository contains the plan file");
    };
    let root = if root.as_os_str().is_empty() {
        Path::new(".")
    } else {
        root
    };
    if !run_git_bool(&["rev-parse", "--verify", "--quiet", "HEAD^{commit}"], root) {
        return skipped("HEAD does not resolve to a commit");
    }
    let tree = HeadTree {
        root,
        changed: OnceCell::new(),
    };
    let mut report = BaseTreeReport::default();
    for stage in stages {
        let gated = matches!(
            detect_stage_type(stage),
            StageType::Standard | StageType::IntegrationVerify
        );
        if !gated {
            continue;
        }
        let Some(working_dir) = normalize_relative(&stage.working_dir) else {
            continue;
        };
        for (idx, criterion) in stage.acceptance.iter().enumerate() {
            if let Some(search) = Search::parse(criterion) {
                let label = format!(
                    "Stage '{}': acceptance criterion #{} `{}`",
                    stage.id,
                    idx + 1,
                    criterion.command()
                );
                tree.evaluate(&working_dir, &search, &label, &mut report.warnings);
            }
        }
    }
    report
}

fn skipped(reason: &str) -> BaseTreeReport {
    BaseTreeReport {
        warnings: Vec::new(),
        note: Some(format!("base-tree evaluation skipped: {reason}")),
    }
}

/// A criterion reduced to what decides its exit status.
struct Search {
    regex: Regex,
    quiet: bool,
    /// `rg` searches directory operands; `grep` without `-r` rejects them.
    recursive: bool,
    operands: Vec<String>,
    expected_exit: i32,
}

impl Search {
    /// The search a criterion runs, when it is a single `rg` or `grep` with no
    /// pipe, separator, substitution, expansion or redirection, only the
    /// flags in `EVALUABLE_FLAGS`, one pattern and at least one path.
    fn parse(criterion: &AcceptanceCriterion) -> Option<Self> {
        let expected_exit = match criterion {
            AcceptanceCriterion::Simple(_) => 0,
            AcceptanceCriterion::Extended(check) => {
                let checks_output = !check.stdout_contains.is_empty()
                    || !check.stdout_not_contains.is_empty()
                    || check.stderr_empty == Some(true);
                if checks_output {
                    return None;
                }
                check.exit_code.unwrap_or(0)
            }
        };
        let tokens = lex(criterion.command());
        let mut words: Vec<&Word> = Vec::with_capacity(tokens.len());
        for token in &tokens {
            match token {
                Token::Word(word) if !word.expands && word.substitutions.is_empty() => {
                    words.push(word);
                }
                _ => return None,
            }
        }
        let args = SearchArgs::parse(&words)?;
        let flags = args.short_flags.as_str();
        let plain_flags = flags.chars().all(|f| EVALUABLE_FLAGS.contains(f));
        let &[(at, offset)] = args.patterns.as_slice() else {
            return None;
        };
        if !plain_flags || !args.long_options.is_empty() || args.operands.is_empty() {
            return None;
        }
        let pattern = words.get(at)?.value.get(offset..)?;
        let operands = args.operands.iter().map(|&i| words[i].value.clone());
        Some(Search {
            regex: build_regex(pattern, &args)?,
            quiet: flags.contains('q'),
            recursive: args.is_rg,
            operands: operands.collect(),
            expected_exit,
        })
    }
}

fn build_regex(pattern: &str, args: &SearchArgs) -> Option<Regex> {
    let fixed = args.short_flags.contains('F');
    if pattern.contains('\n') || (!fixed && pattern.contains("\\n")) {
        return None;
    }
    let body = if fixed {
        regex::escape(pattern)
    } else if args.is_rg || !pattern.contains(|c: char| BRE_DIVERGENT.contains(c)) {
        pattern.to_string()
    } else {
        return None;
    };
    let body = if args.short_flags.contains('w') {
        format!(r"\b{{start-half}}(?:{body})\b{{end-half}}")
    } else {
        body
    };
    RegexBuilder::new(&body)
        .case_insensitive(args.short_flags.contains('i'))
        .build()
        .ok()
}

/// Read access to HEAD, preferring the checkout for files it has not changed.
struct HeadTree<'a> {
    root: &'a Path,
    /// Paths whose checkout differs from HEAD; `None` when git cannot say.
    changed: OnceCell<Option<HashSet<String>>>,
}

impl HeadTree<'_> {
    /// Push the warnings `search` earns against HEAD.
    fn evaluate(&self, working_dir: &str, search: &Search, label: &str, out: &mut Vec<String>) {
        let mut files = Vec::new();
        let (mut missing, mut present) = (false, false);
        for operand in &search.operands {
            let Some(path) = resolve_operand(working_dir, operand) else {
                return;
            };
            let Some(found) = self.files(&path) else {
                return;
            };
            if found.is_empty() {
                missing = true;
                if self.root.join(&path).exists() {
                    out.push(format!(
                        "{label}: '{path}' exists here but is not tracked at HEAD, so stage \
                         worktrees will not see it"
                    ));
                }
                continue;
            }
            let single_file = found.len() == 1 && found[0] == path;
            if !single_file && !search.recursive {
                return;
            }
            present = true;
            let visible = found
                .into_iter()
                .filter(|f| single_file || !hidden_below(f, &path));
            files.extend(visible);
        }
        if !present {
            return;
        }
        let matched = files.iter().any(|file| {
            self.content(file)
                .is_some_and(|bytes| matches_lines(&search.regex, &bytes))
        });
        if exit_status(matched, missing, search.quiet) == search.expected_exit {
            out.push(format!(
                "{label}: criterion passes on the untouched tree; it cannot detect this stage \
                 doing nothing"
            ));
        }
    }

    /// Tracked regular files at or below `path` at HEAD; symlinks and
    /// submodules are left out. `None` when git fails.
    fn files(&self, path: &str) -> Option<Vec<String>> {
        let spec = if path.is_empty() { "." } else { path };
        let args = ["ls-tree", "-r", "-z", "HEAD", "--", spec];
        let listing = run_git_checked(&args, self.root).ok()?;
        let files = listing
            .split('\0')
            .filter_map(|entry| entry.split_once('\t'))
            .filter(|(meta, name)| meta.starts_with("100") && is_at_or_below(name, path))
            .map(|(_, name)| name.to_string())
            .collect();
        Some(files)
    }

    /// The content of `path` at HEAD.
    fn content(&self, path: &str) -> Option<Vec<u8>> {
        let changed = self.changed.get_or_init(|| changed_paths(self.root));
        let unchanged = changed.as_ref().is_some_and(|set| !set.contains(path));
        if unchanged {
            if let Ok(bytes) = std::fs::read(self.root.join(path)) {
                return Some(bytes);
            }
        }
        let object = format!("HEAD:{path}");
        let output = run_git(&["show", &object], self.root).ok()?;
        output.status.success().then_some(output.stdout)
    }
}

/// The status `rg` or `grep` exits with: 0 on a match, 1 on none, 2 when an
/// operand is missing, unless `-q` saw a match anyway.
fn exit_status(matched: bool, missing: bool, quiet: bool) -> i32 {
    match (matched, missing) {
        (true, false) => 0,
        (true, true) if quiet => 0,
        (_, true) => 2,
        (false, false) => 1,
    }
}

/// Paths whose checkout may differ from HEAD, staged or not. Plumbing
/// `diff-index` never refreshes the index, so verification writes nothing; a
/// stat-dirty but unchanged file is listed and read through `git show`.
fn changed_paths(root: &Path) -> Option<HashSet<String>> {
    let args = ["diff-index", "--name-only", "-z", "HEAD", "--"];
    let listing = run_git_checked(&args, root).ok()?;
    let paths = listing.split('\0').filter(|p| !p.is_empty());
    Some(paths.map(str::to_string).collect())
}

/// Whether any line of `bytes` matches, the way `rg` reads lines. Binary
/// content never matches: `rg` skips it when it searches a directory.
fn matches_lines(regex: &Regex, bytes: &[u8]) -> bool {
    if bytes.contains(&0) {
        return false;
    }
    let text = String::from_utf8_lossy(bytes);
    text.split('\n').any(|line| regex.is_match(line))
}

/// `operand` resolved against the stage's working directory, relative to
/// the repository root. `None` for anything git cannot name literally.
fn resolve_operand(working_dir: &str, operand: &str) -> Option<String> {
    if operand.contains(['*', '?', '[', '~']) || operand == "-" {
        return None;
    }
    if working_dir.is_empty() {
        normalize_relative(operand)
    } else {
        normalize_relative(&format!("{working_dir}/{operand}"))
    }
}

/// `path` as a `/`-joined relative path with `.` segments removed; `None`
/// when it is absolute or climbs with `..`.
fn normalize_relative(path: &str) -> Option<String> {
    let mut parts = Vec::new();
    for component in Path::new(path).components() {
        match component {
            Component::Normal(part) => parts.push(part.to_str()?),
            Component::CurDir => {}
            Component::ParentDir | Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    Some(parts.join("/"))
}

fn is_at_or_below(name: &str, path: &str) -> bool {
    match name.strip_prefix(path) {
        _ if path.is_empty() => true,
        Some(rest) => rest.is_empty() || rest.starts_with('/'),
        None => false,
    }
}

/// Whether `file` sits in a hidden entry below the directory `dir`, which
/// `rg` skips unless asked.
fn hidden_below(file: &str, dir: &str) -> bool {
    let rest = file.strip_prefix(dir).unwrap_or(file);
    rest.split('/').any(|part| part.starts_with('.'))
}
