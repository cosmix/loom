//! Test fixtures shared by the evaluation, engine and store-write tests: scratch projects and
//! summaries written the way `seal` writes them, hashed from the current sources.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};

use tempfile::TempDir;

use crate::planning::project::Project;
use crate::planning::runtime::{runtime_paths_under, RuntimePaths};
use crate::planning::summary::evaluate::{cite_key, members_hash};
use crate::planning::summary::format::{self, FrontMatter, SummaryFile, SummaryKind};
use crate::planning::summary::store;

pub(crate) const VISION: &str = "doc/product/index.md";
pub(crate) const S1: &str = "doc/product/section_01.md";
pub(crate) const S2: &str = "doc/product/section_02.md";
pub(crate) const META: &str = "doc/plans/index.md";
pub(crate) const ID1: &str = "product/section_01";
pub(crate) const ID2: &str = "product/section_02";

/// A scratch repository with a loom user dir beside it.
pub(crate) struct Fixture {
    _temp: TempDir,
    pub root: PathBuf,
    pub paths: RuntimePaths,
}

/// A repository under a fresh temp dir holding `files`.
pub(crate) fn fixture(files: &[(&str, &str)]) -> Fixture {
    let temp = tempfile::tempdir().expect("temp dir");
    let root = temp.path().join("repo");
    let home = temp.path().join("home");
    fs::create_dir_all(&root).expect("root");
    fs::create_dir_all(&home).expect("home");
    for (rel, text) in files {
        put(&root, rel, text);
    }
    let paths = runtime_paths_under(&home, &root).expect("runtime paths");
    Fixture {
        _temp: temp,
        root,
        paths,
    }
}

/// A vision listing sections 01 and 02, both written with `## A` and `## B`.
pub(crate) fn two_sections() -> Fixture {
    let (s1, s2) = (section(1, "Alpha.", "Beta."), section(2, "Alpha.", "Beta."));
    fixture(&[(VISION, &vision(&[1, 2])), (S1, &s1), (S2, &s2)])
}

pub(crate) fn put(root: &Path, rel: &str, text: &str) {
    let path = root.join(rel);
    fs::create_dir_all(path.parent().expect("parent")).expect("mkdir");
    fs::write(path, text).expect("write");
}

pub(crate) fn load(root: &Path) -> Project {
    Project::load(root).expect("load project")
}

/// A vision whose `Specification sections` table lists `numbers`.
pub(crate) fn vision(numbers: &[u32]) -> String {
    let rows: String = numbers
        .iter()
        .map(|n| format!("| {n:02} | S{n} |\n"))
        .collect();
    format!("# Vision\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n{rows}")
}

/// A product section with headings `## A` and `## B` holding `a` and `b`.
pub(crate) fn section(n: u32, a: &str, b: &str) -> String {
    format!("# Section {n:02}\n\nIntro.\n\n## A\n\n{a}\n\n## B\n\n{b}\n")
}

/// A metaplan naming each plan file of `files`.
pub(crate) fn metaplan(files: &[&str]) -> String {
    let entry = |(i, f): (usize, &&str)| format!("## {}. E{} (`{f}`)\n\n", i + 1, i + 1);
    let entries: String = files.iter().enumerate().map(entry).collect();
    format!("# Metaplan\n\n{entries}")
}

/// Write `file` rendered as summary `id` with plain `std::fs` (as a hand edit or a pull would).
pub(crate) fn write_file(root: &Path, id: &str, file: &SummaryFile) -> String {
    let text = format::render(file);
    put(root, &store::summary_path(id).to_string_lossy(), &text);
    text
}

fn cites_for(project: &Project, id: &str, refs: &[(PathBuf, &str)]) -> BTreeMap<String, String> {
    refs.iter()
        .map(|(path, anchor)| {
            let sections = &project.docs[path].sections;
            let span = sections
                .find(anchor)
                .unwrap_or_else(|| panic!("{id}: no anchor {anchor}"));
            (cite_key(path, anchor), span.hash.clone())
        })
        .collect()
}

/// `link#anchor`, or `link` alone for the preamble.
fn target(link: &str, anchor: &str) -> String {
    if anchor.is_empty() {
        link.to_string()
    } else {
        format!("{link}#{anchor}")
    }
}

fn with_body(id: &str, front: FrontMatter, body: &str) -> SummaryFile {
    let parsed = format::parse_draft(id, body).unwrap_or_else(|e| panic!("{id} draft: {e:?}"));
    SummaryFile {
        front,
        title: parsed.title,
        blocks: parsed.blocks,
    }
}

/// A sealed document summary of `id` with one `<text> [§](<source>#<anchor>)` sentence per
/// pair, hashed from the current project; written and returned.
pub(crate) fn seal_document(root: &Path, id: &str, lines: &[(&str, &str)]) -> SummaryFile {
    let cited: Vec<(&str, Vec<&str>)> = lines.iter().map(|(text, a)| (*text, vec![*a])).collect();
    seal_document_citing(root, id, &cited)
}

/// Like [`seal_document`], but each sentence cites every anchor of its list, in order.
pub(crate) fn seal_document_citing(
    root: &Path,
    id: &str,
    lines: &[(&str, Vec<&str>)],
) -> SummaryFile {
    let project = load(root);
    let source = store::source_of(&project, id).unwrap_or_else(|| panic!("no source for {id}"));
    let link = store::link_from_summary(id, &source);
    let body: String = lines
        .iter()
        .map(|(text, anchors)| {
            let links: Vec<String> = anchors
                .iter()
                .map(|a| format!("[§]({})", target(&link, a)))
                .collect();
            format!("{text} {}\n\n", links.join(" "))
        })
        .collect();
    let refs: Vec<(PathBuf, &str)> = lines
        .iter()
        .flat_map(|(_, anchors)| anchors.iter().map(|a| (source.clone(), *a)))
        .collect();
    let front = FrontMatter {
        kind: SummaryKind::Document,
        of: source.to_string_lossy().into_owned(),
        file: project.content_hash(&source).map(str::to_string),
        cites: cites_for(&project, id, &refs),
        members: BTreeMap::new(),
    };
    let file = with_body(id, front, &format!("# {id}\n\n{body}"));
    write_file(root, id, &file);
    file
}

/// A sealed overview `id` citing `(member id, member sentence text)` pairs and the raw
/// passages `(path, anchor)`; `members` records every member in scope now.
pub(crate) fn seal_overview(
    root: &Path,
    id: &str,
    cited: &[(&str, &str)],
    raw: &[(&str, &str)],
) -> SummaryFile {
    let project = load(root);
    let mut body = String::from("# Overview\n\n");
    for (i, (member, text)) in cited.iter().enumerate() {
        let sid = format::sentence_id(text);
        body.push_str(&format!("Member {i} is kept. [§]({member}.md#{sid})\n\n"));
    }
    for (i, (path, anchor)) in raw.iter().enumerate() {
        let link = store::link_from_summary(id, Path::new(path));
        body.push_str(&format!(
            "Point {i} holds. [§]({})\n\n",
            target(&link, anchor)
        ));
    }
    let refs: Vec<(PathBuf, &str)> = raw.iter().map(|(p, a)| (PathBuf::from(p), *a)).collect();
    let front = FrontMatter {
        kind: SummaryKind::Overview,
        of: id.to_string(),
        file: None,
        cites: cites_for(&project, id, &refs),
        members: members_now(root, &project, id),
    };
    let file = with_body(id, front, &body);
    write_file(root, id, &file);
    file
}

fn members_now(root: &Path, project: &Project, id: &str) -> BTreeMap<String, String> {
    let scope = store::scope(project);
    let members = if id == "product" {
        scope.product
    } else {
        scope.engineering
    };
    members
        .into_iter()
        .filter_map(|m| {
            let text = fs::read_to_string(root.join(store::summary_path(&m))).ok()?;
            let file = format::parse(&store::summary_path(&m), &text).ok()?;
            Some((m, members_hash(&file)))
        })
        .collect()
}
