//! Fixture project, session directories and plugin listings for the guidance contracts.

use loom::plan_session::kind::SessionDir;
use loom::planning::guidance::{
    render_session_guidance, GuidanceModels, SessionGuidance, SessionGuidanceKind,
};
use loom::planning::project::Project;
use loom::planning::runtime::{runtime_paths_under, RuntimePaths};
use std::fs;
use std::os::unix::fs::DirBuilderExt;
use std::path::{Path, PathBuf};
use tempfile::TempDir;

const VISION: &str = "# Vision\n\n## Specification sections\n\n| # | Section |\n| --- | --- |\n\
| 01 | Alpha |\n| 02 | Beta |\n| 03 | Gamma |\n";
const METAPLAN: &str = "# Metaplan\n\n## 1. Delivery truth (`PLAN-delivery-truth.md`)\n\n\
Deliver the truth.\n";

/// The fixture project, its runtime paths and an isolated `LOOM_HOME`.
pub struct Fixture {
    temp: TempDir,
    pub root: PathBuf,
    pub home: PathBuf,
    pub project: Project,
    pub runtime: RuntimePaths,
    pub session: SessionDir,
}

impl Fixture {
    /// Builds the project of the contract surface and a fresh session directory.
    pub fn new() -> Fixture {
        let temp = tempfile::Builder::new()
            .prefix("loom-guidance-")
            .tempdir_in(std::env::temp_dir())
            .unwrap();
        let root = temp.path().join("repo");
        put(&root, "doc/product/index.md", VISION);
        put(
            &root,
            "doc/product/section_01.md",
            "# Alpha\n\nAlpha body.\n",
        );
        put(&root, "doc/product/section_02.md", "# Beta\n\nBeta body.\n");
        put(&root, "doc/plans/index.md", METAPLAN);
        let truth = "# Plan: Delivery truth\n\nImplements: 01\n";
        put(&root, "doc/plans/PLAN-delivery-truth.md", truth);
        put(
            &root,
            "doc/plans/IN_PROGRESS-PLAN-b.md",
            "# Plan: B\n\nImplements: 02\n",
        );
        let home = temp.path().join("home");
        fs::create_dir_all(&home).unwrap();
        let project = Project::load(&root).unwrap();
        let runtime = runtime_paths_under(&temp.path().join("loom"), &root).unwrap();
        let session = session_dir(temp.path(), "s1");
        Fixture {
            temp,
            root,
            home,
            project,
            runtime,
            session,
        }
    }

    /// Another fresh session directory beside the first.
    pub fn new_session(&self, name: &str) -> SessionDir {
        session_dir(self.temp.path(), name)
    }

    /// `GuidanceModels::load(root)` with `$LOOM_HOME/config.toml` holding `config`
    /// (no file when empty).
    pub fn models(&self, config: &str) -> GuidanceModels {
        if !config.is_empty() {
            fs::write(self.home.join("config.toml"), config).unwrap();
        }
        with_loom_home(&self.home, || GuidanceModels::load(&self.root)).unwrap()
    }

    /// Renders `kind` into `session`.
    pub fn render(
        &self,
        kind: &SessionGuidanceKind,
        session: &SessionDir,
        models: &GuidanceModels,
    ) -> anyhow::Result<SessionGuidance> {
        render_session_guidance(kind, &self.project, &self.runtime, session, models)
    }
}

/// A new `0700` session directory `<base>/sessions/<name>`.
fn session_dir(base: &Path, name: &str) -> SessionDir {
    let parent = base.join("sessions");
    fs::create_dir_all(&parent).unwrap();
    let dir = parent.join(name);
    fs::DirBuilder::new().mode(0o700).create(&dir).unwrap();
    SessionDir::new(dir)
}

pub fn put(root: &Path, rel: &str, content: impl AsRef<[u8]>) {
    fs::create_dir_all(root.join(rel).parent().unwrap()).unwrap();
    fs::write(root.join(rel), content).unwrap();
}

/// Runs `f` with `LOOM_HOME` set to `home`, restoring the previous value.
pub fn with_loom_home<T>(home: &Path, f: impl FnOnce() -> T) -> T {
    let old = std::env::var_os("LOOM_HOME");
    std::env::set_var("LOOM_HOME", home);
    let out = f();
    match old {
        Some(value) => std::env::set_var("LOOM_HOME", value),
        None => std::env::remove_var("LOOM_HOME"),
    }
    out
}

/// Every entry under `dir` (files and directories), relative to it, sorted; symlinks are
/// listed and not followed.
pub fn listing(dir: &Path) -> Vec<String> {
    let mut out = Vec::new();
    walk(dir, dir, &mut out);
    out.sort();
    out
}

fn walk(base: &Path, dir: &Path, out: &mut Vec<String>) {
    for entry in fs::read_dir(dir).unwrap() {
        let path = entry.unwrap().path();
        let rel = path
            .strip_prefix(base)
            .unwrap()
            .to_string_lossy()
            .into_owned();
        let meta = fs::symlink_metadata(&path).unwrap();
        if meta.is_dir() {
            walk(base, &path, out);
            out.push(format!("{rel}/"));
        } else {
            out.push(rel);
        }
    }
}

/// The files (no directory entries) under `dir`, relative to it, sorted.
pub fn files(dir: &Path) -> Vec<String> {
    listing(dir)
        .into_iter()
        .filter(|rel| !rel.ends_with('/'))
        .collect()
}

/// The lines between a Markdown file's opening `---` line and the next `---` line.
pub fn frontmatter(text: &str) -> Vec<String> {
    let mut lines = text.lines();
    assert_eq!(lines.next(), Some("---"), "no frontmatter in:\n{text}");
    lines
        .take_while(|line| *line != "---")
        .map(str::to_string)
        .collect()
}

/// A path under the repository checkout holding the crate.
pub fn repo_file(rel: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(rel)
}
