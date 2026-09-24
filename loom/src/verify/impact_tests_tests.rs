//! Impact selection over real git repositories with a published source-graph
//! base layer; an injected runner records commands so no test runner starts.

use super::*;
use crate::context::graph_store::GraphStore;
use crate::context::refresh::{reconcile_source_graph, SourceGraphScope};
use crate::context::store::{ContextStore, CACHE_RELATIVE_DIR};
use crate::verify::criteria::ProbeRun;
use std::cell::RefCell;
use std::fs;
use tempfile::TempDir;

const CARGO_TOML: &str = "[package]\nname = \"demo\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// `add`, with its unit tests in a `#[path]` module file.
const LIB: &str = "pub fn add(a: u32, b: u32) -> u32 {\n    a + b\n}\n\n\
                   #[cfg(test)]\n#[path = \"lib_tests.rs\"]\nmod tests;\n";

// The calls sit outside `assert_eq!`: the Rust extractor does not look inside
// macro arguments, so a call there leaves no edge to reach.
const LIB_TESTS: &str = "use super::*;\n\n#[test]\nfn adds_in_unit() {\n    \
                         let sum = add(1, 1);\n    assert_eq!(sum, 2);\n}\n";

const ADD_TEST: &str = "use demo::add;\n\n#[test]\nfn adds_two() {\n    \
                        let sum = add(1, 2);\n    assert_eq!(sum, 3);\n}\n";

const OTHER_TEST: &str = "#[test]\nfn subtracts() {\n    assert_eq!(5 - 3, 2);\n}\n";

const CARGO_PASS: &str =
    "running 2 tests\ntest adds_two ... ok\ntest tests::adds_in_unit ... ok\n\n\
                          test result: ok. 2 passed; 0 failed; 0 ignored; 0 measured; 0 filtered \
                          out; finished in 0.00s\n";

/// Records each command with its cwd instead of running it; reports a pass.
#[derive(Default)]
struct Recorder {
    commands: RefCell<Vec<(String, PathBuf)>>,
}

impl ProbeRunner for Recorder {
    fn run(&self, command: &str, package_dir: &Path) -> Result<ProbeRun> {
        let call = (command.to_string(), package_dir.to_path_buf());
        self.commands.borrow_mut().push(call);
        Ok(ProbeRun {
            stdout: CARGO_PASS.to_string(),
            stderr: String::new(),
            exit_code: Some(0),
            timed_out: false,
        })
    }
}

/// Run git in `root` with ambient config neutralised; returns trimmed stdout.
fn git(root: &Path, args: &[&str]) -> String {
    let output = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

/// A repository with `manifest` and a crate whose `add` two test files call
/// and one does not, committed, with a base layer published for `HEAD`.
fn repo_with_base(manifest: (&str, &str)) -> TempDir {
    let temp = TempDir::new().unwrap();
    let root = temp.path();
    git(root, &["init", "-b", "main"]);
    git(root, &["config", "user.email", "t@t.com"]);
    git(root, &["config", "user.name", "t"]);
    let files = [
        manifest,
        ("src/lib.rs", LIB),
        ("src/lib_tests.rs", LIB_TESTS),
        ("tests/add_test.rs", ADD_TEST),
        ("tests/other_test.rs", OTHER_TEST),
    ];
    for (path, content) in files {
        write(root, path, content);
    }
    git(root, &["add", "."]);
    git(root, &["commit", "-m", "seed"]);
    let revision = git(root, &["rev-parse", "HEAD"]);
    let store = ContextStore::with_root(root.join(CACHE_RELATIVE_DIR));
    let graph_store = GraphStore::new(store.root(), &root.join(".loom/work"));
    let scope = SourceGraphScope::Base { revision };
    reconcile_source_graph(&store, &graph_store, root, scope).unwrap();
    temp
}

/// Change `add` in the worktree, then select what reaches it.
fn select_after_changing_add(root: &Path, recorder: &Recorder) -> ImpactOutcome {
    write(root, "src/lib.rs", &LIB.replace("a + b", "b + a"));
    let graph = build_for_worktree(root).unwrap();
    assert_eq!(graph.degraded, None);
    assert_eq!(graph.changed, vec![PathBuf::from("src/lib.rs")]);
    run_with(&Stage::default(), root, &graph, recorder).unwrap()
}

#[test]
fn impact_selection_runs_reached_tests() {
    let temp = repo_with_base(("Cargo.toml", CARGO_TOML));
    let root = temp.path().canonicalize().unwrap();
    let recorder = Recorder::default();

    let outcome = select_after_changing_add(&root, &recorder);

    let commands = recorder.commands.into_inner();
    assert_eq!(commands.len(), 1, "{commands:?}");
    let (command, package_dir) = &commands[0];
    assert!(command.starts_with("cargo test -- "), "{command}");
    assert!(command.contains("adds_two"), "{command}");
    assert!(command.contains("tests::adds_in_unit"), "{command}");
    assert!(!command.contains("subtracts"), "{command}");
    assert_eq!(package_dir.canonicalize().unwrap(), root);
    assert_eq!(outcome.ran, vec![command.clone()]);
    assert!(outcome.notes.is_empty(), "{:?}", outcome.notes);
}

#[test]
fn impact_selection_skips_unsupported_adapters() {
    let cmake = "cmake_minimum_required(VERSION 3.20)\n";
    let temp = repo_with_base(("CMakeLists.txt", cmake));
    let root = temp.path().canonicalize().unwrap();
    let recorder = Recorder::default();

    let outcome = select_after_changing_add(&root, &recorder);

    assert!(recorder.commands.borrow().is_empty());
    assert!(outcome.ran.is_empty());
    let note = format!("ctest cannot select tests by file; {FULL_SUITE}");
    assert_eq!(outcome.notes, vec![note]);
}
