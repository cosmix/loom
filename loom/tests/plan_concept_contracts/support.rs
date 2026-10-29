//! Fixture projects for the concept page contracts: source trees, sealed summaries
//! and concept pages read back as JSON.
#![allow(dead_code)]

use std::fs;
use std::path::{Path, PathBuf};

use loom::plan_server::concept::{load_page, PageContext, PageKey, Scope};
use loom::planning::project::Project;
use loom::planning::runtime::runtime_paths_under;
use loom::planning::summary::evaluate::evaluate;
use loom::planning::summary::seal::{seal, SealError};
use serde_json::Value;
use tempfile::TempDir;

pub const VISION: &str = "doc/product/demo/index.md";
pub const META: &str = "doc/plans/demo/index.md";
pub const PRODUCT_OVERVIEW: &str = "overview/demo/product";
pub const ENGINEERING_OVERVIEW: &str = "overview/demo/engineering";
/// Link from the product overview to the vision's title section.
pub const VISION_LINK: &str = "../../../../product/demo/index.md#vision";
/// Link from the engineering overview to the metaplan's title section.
pub const META_LINK: &str = "../../../../plans/demo/index.md#metaplan";

/// A git-less tree under `<tmp>/repo` holding `files`, with the loom user dir `<tmp>/home`.
pub fn tree(files: &[(String, String)]) -> (TempDir, PathBuf, PathBuf) {
    let temp = tempfile::Builder::new()
        .prefix("loom-concept-contracts-")
        .tempdir_in(std::env::temp_dir())
        .expect("create scratch directory");
    let root = temp.path().join("repo");
    let home = temp.path().join("home");
    fs::create_dir_all(&root).unwrap();
    fs::create_dir_all(&home).unwrap();
    for (rel, text) in files {
        put(&root, rel, text);
    }
    (temp, root, home)
}

pub fn put(root: &Path, rel: &str, content: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// Writes `body` as the draft of summary `id` and seals it, panicking on rejection.
pub fn sealed(root: &Path, home: &Path, id: &str, body: &str) {
    let paths = runtime_paths_under(home, root).expect("runtime paths");
    let draft = paths.drafts.join(format!("{id}.md"));
    fs::create_dir_all(draft.parent().unwrap()).unwrap();
    fs::write(&draft, body).unwrap();
    match seal(root, &paths, id, false) {
        Ok(_) => {}
        Err(SealError::Rejected(errors)) => {
            let lines: Vec<String> = errors
                .iter()
                .map(|e| format!("{}: {}", e.line, e.message))
                .collect();
            panic!("seal {id} rejected: {lines:?}\ndraft:\n{body}");
        }
        Err(SealError::Io(error)) => panic!("seal {id}: {error:#}"),
    }
}

/// Loads the project and its evaluation fresh from disk and renders `key` as JSON.
pub fn page(root: &Path, key: &PageKey, ctx: &PageContext) -> Value {
    let project = Project::load(root).expect("load project");
    let evaluation = evaluate(&project, &[]);
    let page = load_page(&project, &evaluation, key, ctx).expect("the page exists");
    serde_json::to_value(page).expect("serialize the page")
}

/// [`page`] with the default context.
pub fn page_of(root: &Path, key: &PageKey) -> Value {
    page(root, key, &PageContext::default())
}

pub fn product_overview_key() -> PageKey {
    PageKey::Overview {
        feature: "demo".to_string(),
        scope: Scope::Product,
    }
}

pub fn engineering_overview_key() -> PageKey {
    PageKey::Overview {
        feature: "demo".to_string(),
        scope: Scope::Engineering,
    }
}

pub fn summary_key(scope: Scope, doc: &str) -> PageKey {
    PageKey::Summary {
        feature: Some("demo".to_string()),
        scope,
        doc: doc.to_string(),
    }
}

/// A vision titled `# Vision` whose `Specification sections` table lists `numbers` in order.
pub fn vision(numbers: &[u32]) -> String {
    let rows: String = numbers
        .iter()
        .map(|n| format!("| {n:02} | S{n} |\n"))
        .collect();
    format!(
        "# Vision\n\nWhy.\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n{rows}"
    )
}

pub fn section_path(n: u32) -> String {
    format!("doc/product/demo/section_{n:02}.md")
}

/// Summary id of product section `n`.
pub fn section_id(n: u32) -> String {
    format!("demo/product/section_{n:02}")
}

/// A product section with one `## <heading>` per entry, each holding a line of text.
pub fn section(n: u32, headings: &[&str]) -> String {
    let body: String = headings
        .iter()
        .map(|h| format!("## {h}\n\n{h} text.\n\n"))
        .collect();
    format!("# Section {n:02}\n\nIntro.\n\n{body}")
}

/// A document summary draft of section `n`: one `<text> [§](source#anchor)` line per pair.
pub fn section_draft(n: u32, lines: &[(&str, &str)]) -> String {
    let body: String = lines
        .iter()
        .map(|(text, anchor)| {
            format!("{text} [§](../../../../product/demo/section_{n:02}.md#{anchor})\n\n")
        })
        .collect();
    format!("# S{n}\n\n{body}")
}

/// The sentence id of `text` as plan 4's format module computes it.
pub fn sid(text: &str) -> String {
    loom::planning::summary::format::sentence_id(text)
}

/// Link from an overview to sentence `text` of member summary `member`.
pub fn member_link(member: &str, text: &str) -> String {
    format!("../../{member}.md#{}", sid(text))
}

/// A metaplan titled `# Metaplan` naming each plan file in `files`, one entry each.
pub fn metaplan(files: &[&str]) -> String {
    let entries: String = files
        .iter()
        .enumerate()
        .map(|(i, f)| format!("## {}. E{} (`{f}`)\n\n", i + 1, i + 1))
        .collect();
    format!("# Metaplan\n\n{entries}")
}

/// A plan `PLAN-<slug>.md` titled `# Plan: <slug>` with `extra` lines after the title.
pub fn plan(slug: &str, extra: &str) -> (String, String) {
    let path = format!("doc/plans/demo/PLAN-{slug}.md");
    (path, format!("# Plan: {slug}\n\n{extra}Does {slug}.\n"))
}

/// The one-sentence summary draft of plan `slug`, citing its title section.
pub fn plan_draft(slug: &str) -> String {
    format!(
        "# {slug}\n\n{} [§](../../../plans/demo/PLAN-{slug}.md#plan-{slug})\n",
        plan_sentence(slug)
    )
}

pub fn plan_sentence(slug: &str) -> String {
    format!("Plan {slug} does its work.")
}

/// Every sentence of every paragraph block of `page`, in order.
pub fn sentences(page: &Value) -> Vec<Value> {
    let blocks = page["blocks"].as_array().expect("blocks array");
    let paragraphs = blocks.iter().filter(|b| b["kind"] == "paragraph");
    paragraphs
        .flat_map(|b| b["sentences"].as_array().expect("sentences").clone())
        .collect()
}

/// The sentence of `page` whose text starts with `prefix`.
pub fn sentence(page: &Value, prefix: &str) -> Value {
    let all = sentences(page);
    let found = all.iter().find(|s| {
        s["text"]
            .as_str()
            .is_some_and(|t| t.trim_start().starts_with(prefix))
    });
    found
        .cloned()
        .unwrap_or_else(|| panic!("no sentence starting {prefix:?} in {all:#?}"))
}

/// `item[key]["path"]` of every item of array `field` of `value`.
pub fn doc_paths(value: &Value, field: &str, key: &str) -> Vec<String> {
    let items = value[field]
        .as_array()
        .unwrap_or_else(|| panic!("{field} is not an array: {value}"));
    items
        .iter()
        .map(|item| item[key]["path"].as_str().unwrap_or_default().to_string())
        .collect()
}
