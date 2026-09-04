//! Embeds every installable asset (agent/command/skill markdown, the
//! CLAUDE.md/AGENTS.md templates, and the built web dashboard) into the
//! binary. Each asset root is scanned into a generated `OUT_DIR` file that
//! `src/` `include!`s or `include_str!`s, and watched with
//! `cargo:rerun-if-changed` so an edit to any asset triggers a rebuild.

use std::fs;
use std::path::{Path, PathBuf};

const AGENTS_ROOT: &str = "agents";
const COMMANDS_ROOT: &str = "commands";
const SKILLS_ROOT: &str = "skills";
const CODEX_SKILLS_ROOT: &str = "codex/skills";
const WEB_DIST_ROOT: &str = "web/dist";
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

pub(crate) fn repository_root() -> PathBuf {
    let manifest_dir =
        PathBuf::from(std::env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR must be set"));

    manifest_dir
        .parent()
        .expect("loom package must have a repository parent")
        .canonicalize()
        .expect("repository root must exist")
}

pub(crate) fn emit_asset_rerun_keys(repo_root: &Path) {
    for root in ASSET_ROOTS {
        let path = repo_root.join(root);
        if !path.exists() {
            panic!("asset root is missing: {}", path.display());
        }
        crate::emit_if_exists(&path);
    }
}

/// Generate assets from the working tree, not the git index: local builds
/// deliberately embed operator files, while CI's clean checkout is reproducible.
pub(crate) fn generate_embedded_assets(repo_root: &Path) {
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
    write_if_changed(&output, &generated);
}

/// Writes `content` to `path` unless it already holds those exact bytes -
/// every generated file here is `include_str!`ed, so an unconditional write
/// would force a full recompile every time. Missing/unreadable: normal write.
fn write_if_changed(path: &Path, content: &str) {
    if !fs::read_to_string(path).is_ok_and(|existing| existing == content) {
        fs::write(path, content)
            .unwrap_or_else(|error| panic!("failed to write {}: {error}", path.display()));
    }
}

pub(crate) fn generate_web_assets(repo_root: &Path) {
    let dist = repo_root.join(WEB_DIST_ROOT);
    // Unconditional: a missing web/dist must keep build.rs permanently dirty so
    // the first `bun run build` triggers a re-embed. emit_if_exists() would emit
    // nothing here and silently freeze an empty table (see `asset_key` below).
    println!("cargo:rerun-if-changed={}", dist.display());
    let mut generated = String::new();
    if dist.join("index.html").exists() {
        emit_web_assets(&mut generated, &dist);
    } else {
        generated.push_str("pub const WEB_ASSETS: &[WebAsset] = &[];\n");
        println!("cargo:warning=web/dist is missing; `loom status --web` will answer 503. Build it with: cd web && bun install && bun run build");
    }
    let output =
        PathBuf::from(std::env::var("OUT_DIR").expect("OUT_DIR must be set")).join("web_assets.rs");
    write_if_changed(&output, &generated);
}

fn emit_web_assets(output: &mut String, dist: &Path) {
    let mut rows: Vec<_> = walk_files_with(dist, false)
        .into_iter()
        .map(|path| (asset_key(dist, &path), absolute_path(&path)))
        .collect();
    rows.sort_by(|left, right| left.0.cmp(&right.0));

    output.push_str("pub const WEB_ASSETS: &[WebAsset] = &[\n");
    for (key, path) in rows {
        output.push_str(&format!(
            "    ({}, include_bytes!({})),\n",
            rust_literal(&key),
            rust_literal(&path)
        ));
    }
    output.push_str("];\n");
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
    walk_files_with(root, true)
}

fn walk_files_with(root: &Path, validate: bool) -> Vec<PathBuf> {
    let mut files = Vec::new();
    walk_files_inner(root, &mut files, validate);
    files
}

fn walk_files_inner(root: &Path, files: &mut Vec<PathBuf>, validate: bool) {
    for entry in read_dir(root) {
        let entry =
            entry.unwrap_or_else(|error| panic!("failed to read {}: {error}", root.display()));
        let path = entry.path();
        if is_skipped(&path) {
            continue;
        }

        let file_type = entry.file_type().expect("asset type must be readable");
        if file_type.is_dir() {
            walk_files_inner(&path, files, validate);
        } else if file_type.is_file() {
            if validate {
                validate_utf8(&path);
            }
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
