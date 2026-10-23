//! Contracts for stage contract-collection: the derived test-collection
//! scaffold and the completion chain check.
//!
//! Every fixture is an in-memory map from a source-root-relative path to the
//! file's content, read through a `FileSource`.

use std::collections::{BTreeMap, BTreeSet};

use loom::verify::contracts::collection::rust::{
    chain_problems, derive_scaffold, scaffold_problems,
};
use loom::verify::contracts::collection::{ScaffoldEdit, ScaffoldModule};

const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const CONTRACT_FILE: &str = "src/device_lock/contract_tests.rs";
const CONTRACT_TEST: &str = "device_lock::contract_tests::rejects_x";
const CONTRACT_BODY: &str = "#[test]\nfn rejects_x() {}\n";
const BASE_MAIN: &str = "mod devices;\n\nfn main() {}";

/// A fixture crate with the default manifest plus `entries`; an entry for
/// `Cargo.toml` replaces the default manifest.
fn fixture(entries: &[(&str, &str)]) -> BTreeMap<String, String> {
    let mut files = BTreeMap::from([("Cargo.toml".to_string(), MANIFEST.to_string())]);
    for (path, content) in entries {
        files.insert((*path).to_string(), (*content).to_string());
    }
    files
}

fn chain(files: &BTreeMap<String, String>, file: &str, test: &str) -> Vec<String> {
    let lookup = |path: &str| files.get(path).cloned();
    chain_problems(&lookup, file, test)
}

fn scaffold_base() -> BTreeMap<String, String> {
    fixture(&[("src/main.rs", BASE_MAIN), ("src/devices.rs", "")])
}

fn base_edits() -> Vec<ScaffoldEdit> {
    let base = scaffold_base();
    let lookup = |path: &str| base.get(path).cloned();
    derive_scaffold(&lookup, &[(CONTRACT_FILE, CONTRACT_TEST)])
        .expect("the liquidmon-shaped base derives a scaffold")
}

fn problems_against(current: &BTreeMap<String, String>) -> Vec<String> {
    let base = scaffold_base();
    let edits = base_edits();
    let base_lookup = |path: &str| base.get(path).cloned();
    let current_lookup = |path: &str| current.get(path).cloned();
    scaffold_problems(&edits, &base_lookup, &current_lookup)
}

fn module(name: &str, test_only: bool) -> ScaffoldModule {
    ScaffoldModule {
        name: name.to_string(),
        test_only,
    }
}

#[test]
fn chain_rejects_path_redirect() {
    let files = fixture(&[
        ("src/lib.rs", "mod device_lock;\n"),
        (
            "src/device_lock/mod.rs",
            "#[cfg(test)]\n#[path = \"easy.rs\"]\nmod contract_tests;\n",
        ),
        (CONTRACT_FILE, CONTRACT_BODY),
        ("src/device_lock/easy.rs", CONTRACT_BODY),
    ]);
    let problems = chain(&files, CONTRACT_FILE, CONTRACT_TEST);
    assert!(
        !problems.is_empty(),
        "a #[path] redirect must break the chain"
    );
    assert!(
        problems.iter().any(|p| p.contains("#[path")),
        "one problem must name the #[path attribute: {problems:?}"
    );
}

#[test]
fn chain_rejects_inner_cfg() {
    let files = fixture(&[
        ("src/lib.rs", "mod device_lock;\n"),
        (
            "src/device_lock/mod.rs",
            "#![cfg(not(test))]\n\n#[cfg(test)]\nmod contract_tests;\n",
        ),
        (CONTRACT_FILE, CONTRACT_BODY),
    ]);
    let problems = chain(&files, CONTRACT_FILE, CONTRACT_TEST);
    assert!(
        !problems.is_empty(),
        "an inner #![cfg(not(test))] must break the chain"
    );
    assert!(
        problems.iter().any(|p| p.contains("#![cfg")),
        "one problem must name the inner #![cfg attribute: {problems:?}"
    );
}

#[test]
fn chain_rejects_non_target_root() {
    let manifest = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n\
                    autobins = false\n\n[[bin]]\nname = \"app\"\npath = \"src/app.rs\"\n";
    let files = fixture(&[
        ("Cargo.toml", manifest),
        ("src/main.rs", "mod device_lock;\n\nfn main() {}\n"),
        (
            "src/device_lock/mod.rs",
            "#[cfg(test)]\nmod contract_tests;\n",
        ),
        (CONTRACT_FILE, CONTRACT_BODY),
        ("src/app.rs", "fn main() {}\n"),
    ]);
    let problems = chain(&files, CONTRACT_FILE, CONTRACT_TEST);
    assert!(
        !problems.is_empty(),
        "src/main.rs is no target root when autobins = false"
    );
}

#[test]
fn chain_accepts_cfg_test_declaration() {
    let files = fixture(&[
        ("src/main.rs", "pub mod device_lock;\n\nfn main() {}\n"),
        (
            "src/device_lock/mod.rs",
            "pub fn try_lock_file() {}\n\n#[cfg(test)]\nmod contract_tests;\n",
        ),
        (CONTRACT_FILE, "#[test]\nfn rejects_second_handle() {}\n"),
    ]);
    let problems = chain(
        &files,
        CONTRACT_FILE,
        "device_lock::contract_tests::rejects_second_handle",
    );
    assert!(
        problems.is_empty(),
        "a #[cfg(test)] declaration in a bin-only crate is a valid chain: {problems:?}"
    );
}

#[test]
fn scaffold_derives_new_module_root() {
    let edits = base_edits();
    let declare = ScaffoldEdit::Declare {
        path: "src/main.rs".to_string(),
        module: module("device_lock", false),
    };
    assert!(
        edits.contains(&declare),
        "src/main.rs must declare device_lock: {edits:?}"
    );
    let expected_paths: BTreeSet<&str> = ["src/device_lock/mod.rs", "src/device_lock.rs"].into();
    let root = edits.iter().find_map(|edit| match edit {
        ScaffoldEdit::CreateRoot { paths, modules } => Some((paths, modules)),
        ScaffoldEdit::Declare { .. } => None,
    });
    let (paths, modules) = root.expect("a CreateRoot for the new device_lock module");
    let got: BTreeSet<&str> = paths.iter().map(String::as_str).collect();
    assert_eq!(got, expected_paths);
    assert_eq!(paths.len(), 2, "no duplicate root paths: {paths:?}");
    assert_eq!(modules, &vec![module("contract_tests", true)]);
}

#[test]
fn scaffold_derives_nothing_for_new_integration_test() {
    let base = fixture(&[("src/lib.rs", "pub fn f() {}\n")]);
    let lookup = |path: &str| base.get(path).cloned();
    let edits = derive_scaffold(&lookup, &[("tests/x_contract.rs", "rejects_x")]);
    assert_eq!(edits, Ok(vec![]));
}

#[test]
fn scaffold_rejects_production_code() {
    let mut current = scaffold_base();
    current.insert(
        "src/main.rs".to_string(),
        "mod device_lock;\nmod devices;\n\nfn main() {}".to_string(),
    );
    current.insert(
        "src/device_lock/mod.rs".to_string(),
        "#[cfg(test)]\nmod contract_tests;\n\npub fn try_lock_file() {}\n".to_string(),
    );
    current.insert(CONTRACT_FILE.to_string(), CONTRACT_BODY.to_string());
    let problems = problems_against(&current);
    assert!(
        !problems.is_empty(),
        "production code in a scaffold root must be refused"
    );
}

#[test]
fn scaffold_accepts_reordered_declaration() {
    let mut current = scaffold_base();
    current.insert(
        "src/main.rs".to_string(),
        "mod device_lock;\nmod devices;\n\nfn main() {}".to_string(),
    );
    current.insert(
        "src/device_lock.rs".to_string(),
        "#[cfg(test)]\nmod contract_tests;\n".to_string(),
    );
    current.insert(CONTRACT_FILE.to_string(), CONTRACT_BODY.to_string());
    let problems = problems_against(&current);
    assert!(
        problems.is_empty(),
        "a rustfmt-ordered declaration and a 2018-style root are valid: {problems:?}"
    );
}
