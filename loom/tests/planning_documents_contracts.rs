//! Contracts for `loom::planning`'s document model.

use loom::planning::discover::{discover, read_doc};
use loom::planning::paths::{check_relpath_syntax, symlink_component};
use loom::planning::runtime::{project_key, runtime_paths_from, runtime_paths_under};
use loom::planning::sections::{github_slug, normalized_hash, sections_of};
use loom::planning::slug::plan_slug;
use loom::planning::watch::{OwnWrites, Watcher};
use std::fs;
use std::os::unix::fs::{symlink, PermissionsExt};
use std::path::{Path, PathBuf};
use tempfile::TempDir;

#[path = "planning_documents_contracts/features.rs"]
mod features;
#[path = "planning_documents_contracts/kinds.rs"]
mod kinds;
#[path = "planning_documents_contracts/locks.rs"]
mod locks;
#[path = "planning_documents_contracts/plans.rs"]
mod plans;

fn tmp() -> (TempDir, PathBuf) {
    let builder = tempfile::Builder::new().prefix("loom-planning-").clone();
    let temp = builder.tempdir_in(std::env::temp_dir()).unwrap();
    let path = temp.path().to_path_buf();
    (temp, path)
}

fn put(root: &Path, rel: &str, content: impl AsRef<[u8]>) {
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), content).unwrap();
}

fn p(rel: &str) -> PathBuf {
    PathBuf::from(rel)
}

/// `anchor:start-end` for the preamble (empty anchor) and every heading, in order.
fn layout(text: &str) -> String {
    let s = sections_of(text);
    let spans = s.headings.iter().map(|h| (h.anchor.as_str(), &h.span));
    let all = std::iter::once(("", &s.preamble)).chain(spans);
    let parts: Vec<String> = all
        .map(|(a, span)| format!("{a}:{}-{}", span.start_line, span.end_line))
        .collect();
    parts.join(" ")
}

fn found(text: &str, anchor: &str) -> Option<(usize, usize)> {
    let s = sections_of(text);
    s.find(anchor).map(|span| (span.start_line, span.end_line))
}

/// Polls four times; every path reported as added, changed or renamed-to along the way.
fn settle(watcher: &mut Watcher) -> Vec<PathBuf> {
    let mut seen = Vec::new();
    for batch in (0..4).filter_map(|_| watcher.poll().unwrap()) {
        seen.extend(batch.added.into_iter().chain(batch.changed));
        seen.extend(batch.renamed.into_iter().map(|(_, to)| to));
    }
    seen
}

#[test]
fn plan_slug_strips_every_prefix() {
    let cases = "PLAN-a.md=a IN_PROGRESS-PLAN-a.md=a DONE-PLAN-a.md=a RETIRED-PLAN-a.md=a \
                 REVIEW-PLAN-a.md= PLAN-.md= notes.md= DONE-PLAN-DONE-x.md=DONE-x";
    for (name, want) in cases.split(' ').filter_map(|case| case.split_once('=')) {
        let want = Some(want).filter(|slug| !slug.is_empty());
        assert_eq!(plan_slug(name).as_deref(), want, "{name}");
    }
}

#[test]
fn plan_anchor_numbering() {
    let text = "# Title\n\n## 4.2 Line Grammar\n\n## Detection rules\n\n## Detection rules\n\n\
                ### Plans: and *their* lifecycle\n\n## Detection rules";
    let want = ":0-0 title:0-11 42-line-grammar:2-4 detection-rules:4-6 detection-rules-1:6-10 \
                plans-and-their-lifecycle:8-10 detection-rules-2:10-11";
    assert_eq!(layout(text), want);
    assert_eq!(github_slug("4.2 Line Grammar"), "42-line-grammar");
}

#[test]
fn sections_ignore_fenced_headings() {
    let text = "Preamble line.\n# T\n## A\n```sh\n## Not a heading\n# shell comment\n```\n\
                ### A.1\ntext\n## B\nend";
    assert_eq!(layout(text), ":0-1 t:1-11 a:2-9 a1:7-9 b:9-11");
    assert_eq!(found(text, ""), Some((0, 1)), "find(\"\") is the preamble");
}

#[test]
fn span_hash_normalises_whitespace() {
    let lf = "# Doc\n\n## A\nalpha text\n\n## B\nbeta text\n";
    let crlf = lf.replace('\n', "  \r\n");
    let edited = lf.replace("beta text", "gamma text");
    let hashes = |text: &str| {
        let s = sections_of(text);
        let mut all: Vec<String> = s.headings.iter().map(|h| h.span.hash.clone()).collect();
        all.push(s.file_hash);
        all
    };
    let (b, d, c) = (hashes(lf), hashes(&crlf), hashes(&edited));
    assert_eq!(b, d, "CRLF and trailing spaces are normalised");
    assert_eq!(b[3], normalized_hash(lf));
    assert_eq!(b[1], c[1], "an edit in ## B leaves ## A alone");
    assert_ne!(b[2], c[2], "an edit in ## B changes ## B");
    assert_ne!(b[0], c[0], "# Doc encloses ## B");
}

#[test]
fn plan_metadata_anchors() {
    let text = "# Plan\n\nProse.\n\n<!-- loom METADATA -->\n\n```yaml\nloom:\n  version: 1\n  \
                stages:\n    - id: alpha\n      name: Alpha\n    - id: beta\n      name: Beta\n\
                ```\n\n<!-- END loom METADATA -->\n\n## Pressure tests\nOne line.";
    assert_eq!(found(text, "loom-metadata"), Some((4, 17)));
    assert_eq!(found(text, "stage:alpha"), Some((10, 12)));
    assert_eq!(found(text, "stage:beta"), Some((12, 14)));
    assert_eq!(found(text, "pressure-tests"), Some((18, 20)));
}

#[test]
fn discovery_skips_escaping_symlinks() {
    let (_temp, root) = tmp();
    let files = "doc/a.md doc/.hidden.md doc/b.md~ doc/loom/summaries/product.md .env \
                 doc/product/index.md";
    for rel in files.split(' ') {
        put(&root, rel, "alpha\n");
    }
    let links = "a.md:link-in.md ../.env:link-out.md \
                 loom/summaries/product.md:link-summary.md product:dir-link";
    for (target, name) in links.split(' ').filter_map(|link| link.split_once(':')) {
        symlink(target, root.join("doc").join(name)).unwrap();
    }
    let tree = discover(&root).unwrap();
    let paths: Vec<&Path> = tree.docs.iter().map(|d| d.path.as_path()).collect();
    let want = ["doc/a.md", "doc/link-in.md", "doc/product/index.md"];
    assert_eq!(paths, want.map(Path::new));
    assert!(tree.get(Path::new("doc/link-in.md")).unwrap().via_symlink);
    assert!(!tree.get(Path::new("doc/a.md")).unwrap().via_symlink);
    let read = |rel: &str| read_doc(&root, Path::new(rel));
    assert_eq!(read("doc/link-in.md").unwrap(), "alpha\n");
    assert!(read("doc/link-out.md").is_err(), "escaping link");
    assert!(read("doc/../.env").is_err(), "a .. path");
}

#[test]
fn rejects_unsafe_relpaths() {
    let unsafe_paths = "../x.md a/../b.md /etc/passwd a//b.md ./a.md a\\b.md";
    let mut refused: Vec<String> = unsafe_paths.split(' ').map(String::from).collect();
    refused.extend(["a\u{0}b.md".into(), "a\u{1b}b.md".into(), "a".repeat(1025)]);
    for raw in &refused {
        assert!(check_relpath_syntax(raw).is_err(), "{raw:?}");
    }
    let ok = check_relpath_syntax("plans/PLAN-x.md").unwrap();
    assert_eq!(ok, p("doc/plans/PLAN-x.md"));
}

#[test]
fn watcher_batches_and_renames() {
    let (_temp, root) = tmp();
    put(&root, "doc/a.md", "alpha, the file that moves\n");
    put(&root, "doc/b.md", "bravo\n");
    for i in 0..38 {
        put(&root, &format!("doc/f{i:02}.md"), format!("file {i}\n"));
    }
    let mut watcher = Watcher::new(root.clone(), OwnWrites::default());
    let first = watcher.poll().unwrap().expect("initial batch");
    assert_eq!((first.initial, first.added.len()), (true, 40));
    put(&root, "doc/c.md", "charlie\n");
    assert!(watcher.poll().unwrap().is_none(), "changes found: wait");
    fs::create_dir_all(root.join("doc/archive")).unwrap();
    fs::rename(root.join("doc/a.md"), root.join("doc/archive/a.md")).unwrap();
    put(&root, "doc/b.md", "bravo, edited to a new length\n");
    assert!(watcher.poll().unwrap().is_none(), "still changing");
    put(&root, "doc/x.md.swp", "swap\n");
    put(&root, "doc/.#y.md", "lock\n");
    let batch = watcher.poll().unwrap().expect("quiet walk delivers");
    assert_eq!((batch.initial, batch.removed.len()), (false, 0));
    assert_eq!(batch.added, [p("doc/c.md")]);
    assert_eq!(batch.changed, [p("doc/b.md")]);
    let renamed = (p("doc/a.md"), p("doc/archive/a.md"));
    assert_eq!(batch.renamed, [renamed]);
}

#[test]
fn watcher_ignores_own_summary_write() {
    let (_temp, root) = tmp();
    let rel = "doc/loom/summaries/plans/x.md";
    put(&root, "doc/a.md", "alpha\n");
    let own = OwnWrites::default();
    let mut watcher = Watcher::new(root.clone(), own.clone());
    assert!(watcher.poll().unwrap().expect("initial").initial);
    // Registers the hash of `registered` (if any), writes `text`, reports whether it surfaced.
    let mut write = |text: &str, registered: Option<&str>| {
        if let Some(registered) = registered {
            own.register(Path::new(rel), &normalized_hash(registered));
        }
        put(&root, rel, text);
        settle(&mut watcher).contains(&p(rel))
    };
    let ours = "summary written by the engine\n";
    assert!(!write(ours, Some(ours)), "own write");
    assert!(write("summary edited by someone else\n", None));
    let registered = "registered text\n";
    assert!(write("other text\n", Some(registered)), "stale hash");
    assert!(write(registered, None), "stale hash kept");
}

#[test]
fn watcher_survives_binary_and_large_files() {
    let (_temp, root) = tmp();
    let png = b"\x89PNG\r\n\x1a\n\xff\xfe\x00";
    put(&root, "doc/images/logo.png", png);
    put(&root, "doc/big.md", vec![b'a'; 9 * 1024 * 1024]);
    put(&root, "doc/a.md", "alpha\n");
    let mut watcher = Watcher::new(root.clone(), OwnWrites::default());
    let mut added = watcher.poll().unwrap().expect("initial batch").added;
    added.sort();
    let want = ["doc/a.md", "doc/big.md", "doc/images/logo.png"];
    assert_eq!(added, want.map(p));
    put(&root, "doc/a.md", "alpha, edited\n");
    assert!(settle(&mut watcher).contains(&p("doc/a.md")));
    put(&root, "doc/images/logo.png", [&png[..], b"\x01"].concat());
    assert!(settle(&mut watcher).contains(&p("doc/images/logo.png")));
}

#[test]
fn runtime_dir_follows_loom_home() {
    let (_temp, r) = tmp();
    let (loom_dir, proj) = (r.join("loomdir"), r.join("my proj"));
    fs::create_dir_all(&proj).unwrap();
    let paths = runtime_paths_under(&loom_dir, &proj).unwrap();
    assert_eq!(paths.planning_root, loom_dir.join("plan"));
    assert_eq!(paths.dir.parent(), Some(paths.planning_root.as_path()));
    let key = paths.dir.file_name().unwrap().to_str().unwrap();
    let hash = key.strip_prefix("my_proj-").expect("sanitised name");
    assert_eq!(hash.len(), 12, "{key}");
    assert!(hash.chars().all(|c| c.is_ascii_hexdigit()), "{key}");
    assert_eq!(project_key(&proj).unwrap(), key);
    let (one, two) = (r.join("one/proj"), r.join("two/proj"));
    fs::create_dir_all(&one).unwrap();
    fs::create_dir_all(&two).unwrap();
    let (k1, k2) = (project_key(&one).unwrap(), project_key(&two).unwrap());
    assert!(k1.starts_with("proj-") && k2.starts_with("proj-"));
    assert_ne!(k1, k2, "two checkouts named proj collide");
}

fn server_ignores_state(home: &Path, state: &Path, root: &Path, key: &str) {
    std::env::set_var("LOOM_HOME", home);
    std::env::set_var("LOOM_PLAN_STATE", state);
    let resolved = loom::planning::runtime::server_runtime_paths(root);
    std::env::remove_var("LOOM_PLAN_STATE");
    std::env::remove_var("LOOM_HOME");
    let (paths, ignored) = resolved.unwrap();
    assert_eq!(paths.dir, home.join("plan").join(key));
    assert_eq!(ignored, Some(state.as_os_str().to_owned()));
}

#[test]
#[serial_test::serial]
fn runtime_dir_from_session_state() {
    let (_temp, r) = tmp();
    let (root, loom_dir) = (r.join("proj"), r.join("loomdir"));
    fs::create_dir_all(&root).unwrap();
    let (key, other) = (project_key(&root).unwrap(), project_key(&r).unwrap());
    for dir in ["ro/{k}", "ro/{o}", "rw/{k}", "lnk"] {
        fs::create_dir_all(r.join(dir.replace("{k}", &key).replace("{o}", &other))).unwrap();
    }
    let good = r.join("ro").join(&key);
    symlink(&good, r.join("lnk").join(&key)).unwrap();
    let chmod = |mode| {
        for dir in ["ro", "lnk"] {
            fs::set_permissions(r.join(dir), fs::Permissions::from_mode(mode)).unwrap();
        }
    };
    chmod(0o555);
    let sid = Some("123e4567-e89b-12d3-a456-426614174000");
    let resolve = |state: &Path, sid| runtime_paths_from(Some(state), sid, &loom_dir, &root);
    let accepted = resolve(&good, sid);
    let refused = [
        resolve(&r.join("rw").join(&key), sid),
        resolve(&good, None),
        resolve(Path::new(&key), sid),
        resolve(&r.join("lnk").join(&key), sid),
        resolve(&r.join("ro").join(&other), sid),
    ];
    chmod(0o755);
    let accepted = accepted.unwrap();
    assert_eq!(accepted.dir, good);
    assert_eq!(accepted.planning_root, r.join("ro"));
    for (case, result) in refused.iter().enumerate() {
        let err = format!("{:#}", result.as_ref().expect_err("refused"));
        assert!(err.contains("LOOM_PLAN_STATE"), "case {case}: {err}");
    }
    let fallback = runtime_paths_from(None, None, &loom_dir, &root).unwrap();
    assert_eq!(fallback.dir, loom_dir.join("plan").join(&key));
    server_ignores_state(&r.join("home"), &good, &root, &key);
}

#[test]
fn symlink_component_found() {
    let (_temp, r) = tmp();
    let target = Path::new("doc/plans/PLAN-x.md");
    for dir in ["one/doc", "elsewhere", "two/doc/plans", "three/doc/plans"] {
        fs::create_dir_all(r.join(dir)).unwrap();
    }
    symlink(r.join("elsewhere"), r.join("one/doc/plans")).unwrap();
    let first = symlink_component(&r.join("one"), target).unwrap();
    assert_eq!(first, Some(p("doc/plans")), "linked directory");
    put(&r, "real.md", "# Real\n");
    symlink(r.join("real.md"), r.join("two/doc/plans/PLAN-x.md")).unwrap();
    let last = symlink_component(&r.join("two"), target).unwrap();
    assert_eq!(last, Some(p("doc/plans/PLAN-x.md")), "linked file");
    let tail = Path::new("doc/plans/new/PLAN-z.md");
    let missing = symlink_component(&r.join("three"), tail).unwrap();
    assert_eq!(missing, None, "no link, missing tail");
}
