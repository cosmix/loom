//! The files the freeze's static gate may write into the worktree. The gate's
//! cleanup removes only these: any other untracked file new since the gate
//! started stays, so the freeze's tree binding refuses a file a writer added
//! while the gate ran.

use std::collections::BTreeSet;
use std::path::{Component, Path, PathBuf};

use crate::plan::schema::validation::shell_lex::{lex, Token};
use crate::verify::contracts::format_gate::runs_at_freeze;
use crate::verify::contracts::site::ContractSite;

/// Directories a gate tool keeps a cache or build output in, matched as
/// consecutive directory components anywhere in a path: `target` is the
/// build directory of `cargo clippy`, `.ruff_cache` the cache of `ruff check`
/// and `ruff format`, `node_modules/.cache` where `prettier --cache` and other
/// node tools keep theirs.
const ARTEFACT_DIRS: [&[&str]; 3] = [&["target"], &[".ruff_cache"], &["node_modules", ".cache"]];

/// File names a gate tool writes: `.eslintcache` is `eslint --cache`'s cache,
/// `Cargo.lock` is what `cargo clippy` creates for a package without one.
const ARTEFACT_FILES: [&str; 2] = [".eslintcache", "Cargo.lock"];

/// File extensions a gate tool writes: `tsbuildinfo` is TypeScript's
/// incremental build state, written by a check whose tsconfig sets
/// `incremental` or `composite`.
const ARTEFACT_EXTENSIONS: [&str; 1] = ["tsbuildinfo"];

/// Redirection operators that open their target for writing.
const OUTPUT_REDIRECTS: [&str; 7] = [">", ">>", ">|", "&>", "&>>", "<>", ">&"];

/// What one run of the gate may write: the tool artefacts above, and the
/// output redirection targets of the commands the gate runs.
#[derive(Debug, Default)]
pub(super) struct GateWrites {
    /// Redirection targets, relative to the worktree root.
    redirects: BTreeSet<String>,
}

impl GateWrites {
    /// The writes of the gate `check_formatting` runs for `site`: the
    /// acceptance and `after_stage` checks `runs_at_freeze` admits, and the
    /// stage's setup commands, which run before each of them.
    pub(super) fn of(site: &ContractSite, contracts_compile: bool) -> Self {
        let stage = &site.stage;
        let checks: Vec<&str> = stage
            .acceptance
            .iter()
            .map(|criterion| criterion.command())
            .chain(stage.after_stage.iter().map(|check| check.command.as_str()))
            .filter(|command| runs_at_freeze(command, &site.working_dir, contracts_compile))
            .collect();
        if checks.is_empty() {
            return Self::default();
        }
        // Resolved alike, so a target below the working directory strips the
        // root as git's paths do.
        let resolved = |path: &Path| path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let (root, cwd) = (resolved(&site.worktree_root), resolved(&site.working_dir));
        let mut targets = Vec::new();
        let setup = stage.setup.iter().map(String::as_str);
        for command in checks.into_iter().chain(setup) {
            redirect_targets(command, &cwd, &mut targets);
        }
        let redirects = targets
            .iter()
            .filter_map(|target| tree_path(&root, target))
            .collect();
        Self { redirects }
    }

    /// Whether `path`, relative to the worktree root as git lists it, is a
    /// file this gate may have written.
    pub(super) fn covers(&self, path: &str) -> bool {
        self.redirects.contains(path) || is_tool_artefact(path)
    }
}

fn is_tool_artefact(path: &str) -> bool {
    let parts: Vec<&str> = path.split('/').collect();
    let Some((name, dirs)) = parts.split_last() else {
        return false;
    };
    let extension = Path::new(name).extension();
    ARTEFACT_FILES.contains(name)
        || extension.is_some_and(|ext| ARTEFACT_EXTENSIONS.iter().any(|known| ext == *known))
        || ARTEFACT_DIRS
            .iter()
            .any(|dir| dirs.windows(dir.len()).any(|window| window == *dir))
}

/// Push the absolute target of every output redirection in `command`, run
/// from `cwd`. A plain `cd <dir>` moves the directory for the commands after
/// it; a target that expands a variable or a substitution is left out, so
/// the file it names stays and the tree binding decides.
fn redirect_targets(command: &str, cwd: &Path, targets: &mut Vec<PathBuf>) {
    let tokens = lex(command);
    let mut dir = cwd.to_path_buf();
    let mut words: Vec<&str> = Vec::new();
    let mut pending: Option<&str> = None;
    for token in &tokens {
        match token {
            Token::Control(_) => {
                if let ["cd", target] = words[..] {
                    dir = dir.join(target);
                }
                words.clear();
                pending = None;
            }
            Token::Redirect(op) => pending = Some(*op),
            Token::Word(word) => match pending.take() {
                Some(op) if writes_file(op, &word.value) && !word.expands => {
                    targets.push(dir.join(&word.value));
                }
                Some(_) => {}
                None => words.push(&word.value),
            },
        }
    }
}

/// Whether redirection `op` with `target` writes a file: `>&2` and `>&-`
/// duplicate or close a descriptor instead.
fn writes_file(op: &str, target: &str) -> bool {
    let descriptor = target == "-" || target.bytes().all(|byte| byte.is_ascii_digit());
    OUTPUT_REDIRECTS.contains(&op) && !(op == ">&" && descriptor)
}

/// `target` relative to `root` with `.` and `..` resolved lexically, as git
/// lists a path; `None` outside `root`.
fn tree_path(root: &Path, target: &Path) -> Option<String> {
    let relative = target.strip_prefix(root).ok()?;
    let mut parts: Vec<&str> = Vec::new();
    for part in relative.components() {
        match part {
            Component::Normal(name) => parts.push(name.to_str()?),
            Component::CurDir => {}
            Component::ParentDir => {
                parts.pop()?;
            }
            Component::RootDir | Component::Prefix(_) => return None,
        }
    }
    (!parts.is_empty()).then(|| parts.join("/"))
}
