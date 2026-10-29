//! Shared fixtures for the summary-store contracts: project trees, drafts, seals,
//! evaluation state names and the `loom plan summary` command.
#![allow(dead_code)]

mod fixtures;
pub use fixtures::*;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use loom::planning::project::Project;
use loom::planning::runtime::{runtime_paths_under, RuntimePaths};
use loom::planning::summary::claims::Claim;
use loom::planning::summary::evaluate::{evaluate, Evaluation, SentenceState, SummaryState};
use loom::planning::summary::format::{Block, Sentence, SummaryFile};
use loom::planning::summary::seal::{seal, SealError};
use tempfile::TempDir;

pub const PRODUCT: &str = "overview/demo/product";
pub const ENGINEERING: &str = "overview/demo/engineering";
pub const VISION: &str = "doc/product/demo/index.md";
pub const S1: &str = "doc/product/demo/section_01.md";
pub const S2: &str = "doc/product/demo/section_02.md";
pub const META: &str = "doc/plans/demo/index.md";
pub const PLAN_X: &str = "doc/plans/demo/PLAN-x.md";
/// Link from the product overview to the vision's sections table.
pub const VISION_LINK: &str = "../../../../product/demo/index.md#specification-sections";
/// Link from the engineering overview to the metaplan's title section.
pub const META_LINK: &str = "../../../../plans/demo/index.md#metaplan";

/// A git-less tree under `<tmp>/repo` holding `files`; returns the temp dir, the
/// repository root and the loom user dir `<tmp>/home`.
pub fn project(files: &[(&str, &str)]) -> (TempDir, PathBuf, PathBuf) {
    let temp = tempfile::Builder::new()
        .prefix("loom-summary-contracts-")
        .tempdir_in(std::env::temp_dir())
        .expect("create scratch directory");
    let root = temp.path().join("repo");
    let loom_dir = temp.path().join("home");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&loom_dir).unwrap();
    for (rel, text) in files {
        put(&root, rel, text);
    }
    (temp, root, loom_dir)
}

/// [`project`] with `git init -q` run in the root, for the CLI contracts.
pub fn git_project(files: &[(&str, &str)]) -> (TempDir, PathBuf, PathBuf) {
    let (temp, root, loom_dir) = project(files);
    let init = Command::new("git")
        .args(["init", "-q"])
        .current_dir(&root)
        .status();
    assert!(init.unwrap().success(), "git init");
    (temp, root, loom_dir)
}

pub fn put(root: &Path, rel: &str, content: impl AsRef<[u8]>) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

pub fn read(root: &Path, rel: &str) -> String {
    fs::read_to_string(root.join(rel)).unwrap_or_else(|e| panic!("read {rel}: {e}"))
}

/// The runtime paths of `root` under the loom user dir `loom_dir`.
pub fn paths(root: &Path, loom_dir: &Path) -> RuntimePaths {
    runtime_paths_under(loom_dir, root).expect("runtime paths")
}

/// Repo-relative path of summary `id` in the store.
pub fn summary_rel(id: &str) -> String {
    format!("doc/loom/summaries/{id}.md")
}

/// Writes `body` as the draft of `id` under the runtime dir.
pub fn write_draft(root: &Path, loom_dir: &Path, id: &str, body: &str) -> PathBuf {
    let path = paths(root, loom_dir).drafts.join(format!("{id}.md"));
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(&path, body).unwrap();
    path
}

/// Seals `id` from its existing draft, mapping a rejection to `<line>: <message>` lines.
pub fn seal_id(root: &Path, loom_dir: &Path, id: &str, force: bool) -> Result<(), String> {
    match seal(root, &paths(root, loom_dir), id, force) {
        Ok(_) => Ok(()),
        Err(SealError::Rejected(errors)) => Err(errors
            .iter()
            .map(|e| format!("{}: {}", e.line, e.message))
            .collect::<Vec<_>>()
            .join("\n")),
        Err(SealError::Io(error)) => Err(format!("io: {error:#}")),
    }
}

/// Writes the draft of `id` and seals it without `--force`.
pub fn seal_draft(root: &Path, loom_dir: &Path, id: &str, body: &str) -> Result<(), String> {
    write_draft(root, loom_dir, id, body);
    seal_id(root, loom_dir, id, false)
}

/// [`seal_draft`] that must succeed.
pub fn sealed(root: &Path, loom_dir: &Path, id: &str, body: &str) {
    if let Err(e) = seal_draft(root, loom_dir, id, body) {
        panic!("seal {id} failed:\n{e}\ndraft:\n{body}");
    }
}

pub fn load(root: &Path) -> Project {
    Project::load(root).expect("load project")
}

pub fn eval(root: &Path, claims: &[Claim]) -> Evaluation {
    evaluate(&load(root), claims)
}

pub fn summary_state(s: &SummaryState) -> &'static str {
    match s {
        SummaryState::Current => "current",
        SummaryState::Incomplete => "incomplete",
        SummaryState::Stale => "stale",
        SummaryState::Rebuilding => "rebuilding",
        SummaryState::Missing => "missing",
    }
}

pub fn sentence_state(s: &SentenceState) -> &'static str {
    match s {
        SentenceState::Current => "current",
        SentenceState::Stale => "stale",
    }
}

/// The state name of summary `id` in `evaluation`.
pub fn state(evaluation: &Evaluation, id: &str) -> &'static str {
    let summary = evaluation
        .summaries
        .get(id)
        .unwrap_or_else(|| panic!("{id} not evaluated"));
    summary_state(&summary.state)
}

/// `(text, state)` of every sentence of summary `id` in `evaluation`.
pub fn sentence_states(evaluation: &Evaluation, id: &str) -> Vec<(String, &'static str)> {
    evaluation.summaries[id]
        .sentences
        .iter()
        .map(|s| (s.text.clone(), sentence_state(&s.state)))
        .collect()
}

/// Every sentence of a parsed summary, in order.
pub fn sentences(file: &SummaryFile) -> Vec<&Sentence> {
    let mut out = Vec::new();
    for block in &file.blocks {
        if let Block::Paragraph(list) = block {
            out.extend(list.iter());
        }
    }
    out
}

/// A vision whose `Specification sections` table lists `numbers`.
pub fn vision(numbers: &[u32]) -> String {
    let row = |n: &u32| format!("| {n:02} | S{n} |\n");
    let rows: String = numbers.iter().map(row).collect();
    format!("# Vision\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n{rows}")
}

/// A product section with headings `## A` and `## B` holding `a` and `b`.
pub fn section(n: u32, a: &str, b: &str) -> String {
    format!("# Section {n:02}\n\nIntro.\n\n## A\n\n{a}\n\n## B\n\n{b}\n")
}

/// Link from summary `demo/product/section_<n>` to `anchor` of its source.
pub fn doc_link(n: u32, anchor: &str) -> String {
    format!("../../../../product/demo/section_{n:02}.md#{anchor}")
}

/// A document summary draft for section `n`: one `<text> [§](#anchor)` line per pair.
pub fn doc_draft(n: u32, lines: &[(&str, &str)]) -> String {
    let body: String = lines
        .iter()
        .map(|(text, anchor)| format!("{text} [§]({})\n\n", doc_link(n, anchor)))
        .collect();
    format!("# S{n}\n\n{body}")
}

/// The sentence id of `text` as the format module computes it.
pub fn sid(text: &str) -> String {
    loom::planning::summary::format::sentence_id(text)
}

/// Link from an overview to sentence `text` of member summary `member`.
pub fn member_link(member: &str, text: &str) -> String {
    format!("../../{member}.md#{}", sid(text))
}

/// A metaplan naming each plan file in `files`, one numbered entry each.
pub fn metaplan(files: &[&str]) -> String {
    let entry = |(i, f): (usize, &&str)| format!("## {}. E{} (`{f}`)\n\n", i + 1, i + 1);
    let entries: String = files.iter().enumerate().map(entry).collect();
    format!("# Metaplan\n\n{entries}")
}

/// Runs `loom plan summary <args>` in `dir` with `LOOM_HOME=home`, no other inherited
/// `LOOM_*` variable, and `envs` added.
pub fn loom_summary(dir: &Path, home: &Path, args: &[&str], envs: &[(&str, &str)]) -> Output {
    let mut command = Command::new(env!("CARGO_BIN_EXE_loom"));
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
    command
        .args(["plan", "summary"])
        .args(args)
        .current_dir(dir)
        .env("LOOM_HOME", home)
        .envs(envs.iter().copied());
    command.output().expect("run loom plan summary")
}

pub fn stderr(output: &Output) -> String {
    String::from_utf8_lossy(&output.stderr).into_owned()
}
