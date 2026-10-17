//! Selections that cannot be run, and how a long selection displays.

use super::*;
use crate::models::stage::Stage;
use crate::testrun::registry;
use crate::verify::criteria::{CriteriaConfig, CriteriaProbe};
use tempfile::TempDir;

fn cargo() -> &'static dyn TestRunnerAdapter {
    registry::by_name("cargo-test").unwrap()
}

fn named(names: impl IntoIterator<Item = String>) -> Vec<TestTarget> {
    names
        .into_iter()
        .map(|name| TestTarget {
            file: "src/lib.rs".to_string(),
            name: Some(name),
        })
        .collect()
}

/// Fails as the real runner does when exec refuses the command: an OS error
/// under a context that repeats the whole command.
struct CannotSpawn;

impl ProbeRunner for CannotSpawn {
    fn run(&self, _command: &str, _package_dir: &Path) -> Result<ProbeRun> {
        let os_error = anyhow::Error::from(std::io::Error::from_raw_os_error(7));
        Err(os_error.context(format!(
            "Failed to execute criterion: {}",
            "x".repeat(300_000)
        )))
    }
}

#[test]
fn a_selection_that_cannot_start_is_a_note() {
    let root = TempDir::new().unwrap();
    let mut selection = Selection::new(root.path(), &CannotSpawn, BTreeSet::new());
    let targets = named(["a", "b", "c"].map(String::from));
    selection.run_group(Path::new(""), cargo(), &targets);
    let outcome = selection.finish("s").unwrap();
    assert!(outcome.ran.is_empty());
    assert_eq!(outcome.notes.len(), 1);
    let note = &outcome.notes[0];
    assert!(note.contains("could not be run"), "{note}");
    assert!(note.contains(FULL_SUITE), "{note}");
    assert!(note.len() < 1_000, "{} bytes", note.len());
}

#[test]
fn an_oversized_selection_fails_exec_and_becomes_a_note() {
    let root = TempDir::new().unwrap();
    let stage = Stage::default();
    let probe = CriteriaProbe {
        stage: &stage,
        config: CriteriaConfig::default(),
    };
    let mut selection = Selection::new(root.path(), &probe, BTreeSet::new());
    let targets = named(["a".repeat(4 << 20)]);
    selection.run_group(Path::new(""), cargo(), &targets);
    let outcome = selection.finish("s").unwrap();
    assert!(outcome.ran.is_empty());
    assert_eq!(outcome.notes.len(), 1);
    let note = &outcome.notes[0];
    assert!(
        note.contains("could not be run"),
        "{}",
        &note[..200.min(note.len())]
    );
    assert!(note.len() < 1_000, "{} bytes", note.len());
}

/// A selection runs in its package with the whole checkout visible, so the
/// package's sibling packages resolve as they do for acceptance at the root.
#[test]
fn a_probe_with_a_tree_root_reads_sibling_packages() {
    if crate::process::sandbox_probe::skip_unless(
        crate::verify::criteria::host_launcher().is_some(),
        "a_probe_with_a_tree_root_reads_sibling_packages",
        "OS sandbox launcher absent",
    ) {
        return;
    }
    let root = TempDir::new().unwrap();
    let package = root.path().join("pkg");
    std::fs::create_dir(&package).unwrap();
    std::fs::create_dir(root.path().join("contracts")).unwrap();
    std::fs::write(root.path().join("contracts/schema"), "shared").unwrap();
    let stage = Stage::default();
    let probe = CriteriaProbe {
        stage: &stage,
        config: CriteriaConfig::default().with_tree_root(Some(root.path())),
    };
    let run = probe.run("cat ../contracts/schema", &package).unwrap();
    assert_eq!(run.exit_code, Some(0), "{}", run.stderr);
    assert_eq!(run.stdout, "shared");
}

#[test]
fn a_short_command_displays_unchanged() {
    assert_eq!(shown_command("cargo test -- a", 1), "cargo test -- a");
}

#[test]
fn a_long_command_is_shortened_with_its_target_count() {
    let command = format!("cargo test -- {}", "é".repeat(500));
    let shown = shown_command(&command, 500);
    assert!(shown.ends_with("… (500 targets)"), "{shown}");
    assert!(shown.len() < COMMAND_DISPLAY_CHARS + 30);
    assert!(shown.starts_with("cargo test -- é"));
}

#[test]
fn selected_lists_the_first_ten_names() {
    let targets = named((0..25).map(|n| format!("t{n}")));
    let listed = selected(&targets);
    assert!(listed.starts_with("selected: t0, t1,"), "{listed}");
    assert!(listed.contains("t9, and 15 more"), "{listed}");
    assert!(!listed.contains("t10"), "{listed}");
    assert_eq!(selected(&targets[..3]), "selected: t0, t1, t2");
}
