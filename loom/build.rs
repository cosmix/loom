//! Build script: derives `LOOM_VERSION`/`LOOM_COMMIT`/`LOOM_BUILD_DATE`/
//! `LOOM_TARGET` from git state and embeds them as compile-time env vars.

use std::fs;
use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{SystemTime, UNIX_EPOCH};

include!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/src/version/derive.rs"
));

const AGENTS_ROOT: &str = "agents";
const COMMANDS_ROOT: &str = "commands";
const SKILLS_ROOT: &str = "skills";
const CODEX_SKILLS_ROOT: &str = "codex/skills";
const CLAUDE_TEMPLATE: &str = "CLAUDE.md.template";
const AGENTS_TEMPLATE: &str = "AGENTS.md.template";

/// The six asset roots embedded into the binary. Named once here so
/// `emit_asset_rerun_keys` (rerun-if-changed watchers) and
/// `generate_embedded_assets` (the actual embed) cannot drift apart.
const ASSET_ROOTS: [&str; 6] = [
    AGENTS_ROOT,
    COMMANDS_ROOT,
    SKILLS_ROOT,
    CODEX_SKILLS_ROOT,
    CLAUDE_TEMPLATE,
    AGENTS_TEMPLATE,
];

fn main() {
    let describe_exact = run_git(&["describe", "--tags", "--exact-match"]);
    let describe = run_git(&["describe", "--tags"]);
    let short_sha = run_git(&["rev-parse", "--short", "HEAD"]);

    let version = derive_version(
        describe_exact.as_deref(),
        describe.as_deref(),
        short_sha.as_deref(),
    );
    let commit = short_sha.unwrap_or_else(|| "unknown".to_string());
    let target = std::env::var("TARGET").unwrap_or_else(|_| "unknown".to_string());

    println!("cargo:rustc-env=LOOM_VERSION={version}");
    println!("cargo:rustc-env=LOOM_COMMIT={commit}");
    println!("cargo:rustc-env=LOOM_BUILD_DATE={}", build_date());
    println!("cargo:rustc-env=LOOM_TARGET={target}");

    emit_rerun_keys();

    let repo_root = repository_root();
    emit_asset_rerun_keys(&repo_root);
    generate_embedded_assets(&repo_root);
}

fn repository_root() -> PathBuf {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));

    manifest_dir
        .parent()
        .expect("loom package must have a repository parent")
        .canonicalize()
        .expect("repository root must exist")
}

fn emit_asset_rerun_keys(repo_root: &Path) {
    for root in ASSET_ROOTS {
        let path = repo_root.join(root);
        if !path.exists() {
            panic!("asset root is missing: {}", path.display());
        }
        emit_if_exists(&path);
    }
}

/// Generate assets from the working tree, not the git index: local builds
/// deliberately embed operator files, while CI's clean checkout is reproducible.
fn generate_embedded_assets(repo_root: &Path) {
    let agents = repo_root.join(AGENTS_ROOT);
    let commands = repo_root.join(COMMANDS_ROOT);
    let skills = repo_root.join(SKILLS_ROOT);
    let codex_skills = repo_root.join(CODEX_SKILLS_ROOT);
    let mut generated = String::new();

    emit_group(
        &mut generated,
        "CLAUDE_AGENTS",
        &agents,
        top_level_markdown(&agents),
        KeyShape::Flat,
    );
    emit_group(
        &mut generated,
        "CLAUDE_COMMANDS",
        &commands,
        top_level_markdown(&commands),
        KeyShape::Flat,
    );
    emit_group(
        &mut generated,
        "SKILLS",
        &skills,
        loom_skill_files(&skills),
        KeyShape::Nested,
    );
    emit_group(
        &mut generated,
        "CODEX_SKILLS",
        &codex_skills,
        walk_files(&codex_skills),
        KeyShape::Nested,
    );
    emit_scalar(
        &mut generated,
        "CLAUDE_MD_TEMPLATE",
        &repo_root.join(CLAUDE_TEMPLATE),
    );
    emit_scalar(
        &mut generated,
        "AGENTS_MD_TEMPLATE",
        &repo_root.join(AGENTS_TEMPLATE),
    );

    let output = PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR must be set"))
        .join("embedded_assets.rs");
    fs::write(&output, generated)
        .unwrap_or_else(|error| panic!("failed to write {}: {error}", output.display()));
}

fn top_level_markdown(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in read_dir(root) {
        let entry =
            entry.unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));
        let path = entry.path();
        if !is_skipped(&path)
            && entry
                .file_type()
                .expect("asset type must be readable")
                .is_file()
            && path.extension().and_then(|extension| extension.to_str()) == Some("md")
        {
            validate_utf8(&path);
            files.push(path);
        }
    }
    files
}

fn loom_skill_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    for entry in read_dir(root) {
        let entry =
            entry.unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));
        let path = entry.path();
        let is_skill = path
            .file_name()
            .and_then(|name| name.to_str())
            .is_some_and(|name| name.starts_with("loom-"));
        if !is_skipped(&path)
            && is_skill
            && entry
                .file_type()
                .expect("asset type must be readable")
                .is_dir()
        {
            files.extend(walk_files(&path));
        }
    }
    files
}

fn walk_files(root: &Path) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_files_inner(root, &mut files);
    files
}

fn walk_files_inner(root: &Path, files: &mut Vec<PathBuf>) {
    for entry in read_dir(root) {
        let entry =
            entry.unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));
        let path = entry.path();
        if is_skipped(&path) {
            continue;
        }

        let file_type = entry.file_type().expect("asset type must be readable");
        if file_type.is_dir() {
            walk_files_inner(&path, files);
        } else if file_type.is_file() {
            validate_utf8(&path);
            files.push(path);
        }
    }
}

fn read_dir(root: &Path) -> fs::ReadDir {
    fs::read_dir(root).unwrap_or_else(|error| panic!("failed to walk {}: {error}", root.display()))
}

fn is_skipped(path: &Path) -> bool {
    path.file_name()
        .and_then(|name| name.to_str())
        .is_some_and(|name| name == "__pycache__" || name.starts_with('.'))
}

fn validate_utf8(path: &Path) {
    let bytes = fs::read(path)
        .unwrap_or_else(|error| panic!("failed to read asset {}: {error}", path.display()));
    std::str::from_utf8(&bytes)
        .unwrap_or_else(|_| panic!("asset file is not valid UTF-8: {}", path.display()));
}

/// Whether a group's asset keys are bare file names (`CLAUDE_AGENTS`,
/// `CLAUDE_COMMANDS`) or must be nested inside a directory (`SKILLS`,
/// `CODEX_SKILLS`). `Nested` enforces the installer's invariant that a key
/// derived by splitting on the first `/` names a real skill directory. A key
/// with no `/` would be embedded here and then silently dropped at install
/// time, so `Nested` panics the build instead.
enum KeyShape {
    Flat,
    Nested,
}

fn emit_group(
    output: &mut String,
    name: &str,
    root: &Path,
    files: Vec<PathBuf>,
    key_shape: KeyShape,
) {
    let mut rows: Vec<_> = files
        .into_iter()
        .map(|path| (asset_key(root, &path), absolute_path(&path)))
        .collect();
    if matches!(key_shape, KeyShape::Nested) {
        for (key, path) in &rows {
            if !key.contains('/') {
                panic!("asset must live inside a skill directory: {path}");
            }
        }
    }
    rows.sort_by(|left, right| left.0.cmp(&right.0));

    output.push_str(&format!("pub const {name}: &[Asset] = &[\n"));
    for (key, path) in rows {
        output.push_str(&format!(
            "    ({}, include_str!({})),\n",
            rust_literal(&key),
            rust_literal(&path)
        ));
    }
    output.push_str("];\n\n");
}

fn emit_scalar(output: &mut String, name: &str, path: &Path) {
    validate_utf8(path);
    output.push_str(&format!(
        "pub const {name}: &str = include_str!({});\n\n",
        rust_literal(&absolute_path(path))
    ));
}

fn asset_key(root: &Path, path: &Path) -> String {
    path.strip_prefix(root)
        .expect("asset must be below its source root")
        .to_str()
        .unwrap_or_else(|| panic!("asset path is not UTF-8: {}", path.display()))
        .replace('\\', "/")
}

fn absolute_path(path: &Path) -> String {
    path.to_str()
        .unwrap_or_else(|| panic!("asset path is not UTF-8: {}", path.display()))
        .to_string()
}

fn rust_literal(value: &str) -> String {
    format!("{:?}", value)
}

/// Run a `git` subcommand, returning `None` on spawn failure, non-zero exit,
/// non-UTF8 output, or empty trimmed output.
fn run_git(args: &[&str]) -> Option<String> {
    let output = Command::new("git").args(args).output().ok()?;
    if !output.status.success() {
        return None;
    }
    let stdout = String::from_utf8(output.stdout).ok()?;
    let trimmed = stdout.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

/// Emit `cargo:rerun-if-changed` for real, existing paths only. A path that
/// does not exist is permanently dirty to cargo, forcing a rebuild every
/// time, so nothing is emitted for one.
///
/// The watched paths are resolved with `git rev-parse --git-path HEAD` (the
/// per-worktree HEAD) and `git rev-parse --git-common-dir` (from which
/// `refs/tags` and `packed-refs` are derived), rather than guessed directly,
/// because this build script's CWD is the package root (`loom/`), and in a
/// linked git worktree `.git` is a file pointing elsewhere, not a directory —
/// so neither path can be assumed literally.
///
/// On a normal branch checkout `HEAD` is a symref (`ref: refs/heads/<branch>`)
/// and its own mtime does not change when a commit lands on that branch — the
/// file that actually moves is the branch ref `HEAD` points at. Watching only
/// `HEAD` would leave `LOOM_COMMIT` stale after every commit on a branch, so
/// the resolved ref file is watched too. `symbolic-ref -q HEAD` fails on a
/// detached HEAD, where `HEAD` itself holds the sha and is already watched
/// above, so that case needs no extra path. A branch whose ref is packed
/// rather than loose falls back to `packed-refs`, which is already watched.
///
/// `logs/HEAD` (the reflog) is watched too as a belt-and-braces trigger: it
/// gains an entry on every commit, checkout, and reset regardless of which
/// path above resolved it, so it catches any HEAD movement the specific
/// cases above did not anticipate.
fn emit_rerun_keys() {
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-changed=src/version/derive.rs");

    if let Some(head) = run_git(&["rev-parse", "--git-path", "HEAD"]) {
        emit_if_exists(Path::new(&head));
    }

    if let Some(symbolic) = run_git(&["symbolic-ref", "-q", "HEAD"]) {
        if let Some(ref_path) = run_git(&["rev-parse", "--git-path", &symbolic]) {
            emit_if_exists(Path::new(&ref_path));
        }
    }

    if let Some(reflog) = run_git(&["rev-parse", "--git-path", "logs/HEAD"]) {
        emit_if_exists(Path::new(&reflog));
    }

    if let Some(common_dir) = run_git(&["rev-parse", "--git-common-dir"]) {
        let common_dir = PathBuf::from(common_dir);
        emit_if_exists(&common_dir.join("refs").join("tags"));
        emit_if_exists(&common_dir.join("packed-refs"));
    }
}

fn emit_if_exists(path: &Path) {
    if let Ok(canonical) = path.canonicalize() {
        println!("cargo:rerun-if-changed={}", canonical.display());
    }
}

/// Compute today's UTC date (`YYYY-MM-DD`) with `std` only. The days-from-civil
/// math lives in `civil_date_from_days` (`src/version/derive.rs`) so it can be
/// unit tested; this just reads the clock.
fn build_date() -> String {
    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default();
    let days = (now.as_secs() / 86400) as i64;

    civil_date_from_days(days)
}
