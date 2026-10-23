#[cfg(feature = "source-graph")]
use super::super::chain_problems;
use super::super::tree;
use super::{package_of, run_dir_problems, test_roots, Kind, Root};

const PACKAGE: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// The test roots that may reach `file` in a tree of `entries`.
fn roots(entries: &[(&str, &str)], file: &str) -> Vec<Root> {
    let source = tree(entries);
    let package = package_of(&source, file).expect("a package owns the file");
    test_roots(&source, &package, file)
}

fn root(path: &str, kind: Kind, not_test: Option<&'static str>) -> Root {
    Root {
        path: path.to_string(),
        kind,
        not_test,
    }
}

fn manifest(extra: &str) -> String {
    format!("{PACKAGE}{extra}")
}

#[test]
fn default_lib_is_a_test_root() {
    let found = roots(&[("Cargo.toml", PACKAGE), ("src/lib.rs", "")], "src/a.rs");
    assert_eq!(found, vec![root("src/lib.rs", Kind::Lib, None)]);
}

#[test]
fn lib_path_replaces_the_default_lib_root() {
    let cargo = manifest("\n[lib]\npath = \"src/other.rs\"\n");
    let files = [
        ("Cargo.toml", cargo.as_str()),
        ("src/lib.rs", ""),
        ("src/other.rs", ""),
    ];
    assert_eq!(
        roots(&files, "src/a.rs"),
        vec![root("src/other.rs", Kind::Lib, None)]
    );
}

#[test]
fn autolib_false_leaves_no_implicit_lib_root() {
    let cargo = manifest("autolib = false\n");
    let files = [("Cargo.toml", cargo.as_str()), ("src/lib.rs", "")];
    assert_eq!(roots(&files, "src/a.rs"), Vec::<Root>::new());
}

/// The chain check follows `[lib] path`: the default `src/lib.rs` is not
/// compiled, so a declaration there reaches nothing.
#[cfg(feature = "source-graph")]
#[test]
fn the_chain_starts_at_the_lib_path() {
    let cargo = manifest("\n[lib]\npath = \"src/other.rs\"\n");
    let chain = |lib: &str, other: &str| {
        let files = [
            ("Cargo.toml", cargo.as_str()),
            ("src/lib.rs", lib),
            ("src/other.rs", other),
            ("src/a.rs", ""),
        ];
        chain_problems(&tree(&files), "src/a.rs", "a::rejects_x")
    };
    assert!(!chain("mod a;\n", "").is_empty());
    assert_eq!(chain("", "mod a;\n"), Vec::<String>::new());
}

#[cfg(feature = "source-graph")]
#[test]
fn autolib_false_leaves_the_chain_no_target() {
    let cargo = manifest("autolib = false\n");
    let files = [
        ("Cargo.toml", cargo.as_str()),
        ("src/lib.rs", "mod a;\n"),
        ("src/a.rs", ""),
    ];
    let problems = chain_problems(&tree(&files), "src/a.rs", "a::rejects_x");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(problems[0].contains("no test target"), "{problems:?}");
}

#[test]
fn default_bin_is_a_test_root() {
    let found = roots(&[("Cargo.toml", PACKAGE), ("src/main.rs", "")], "src/a.rs");
    assert_eq!(found, vec![root("src/main.rs", Kind::Bin, None)]);
}

#[test]
fn autobins_false_keeps_only_listed_bins() {
    let cargo = manifest("autobins = false\n\n[[bin]]\nname = \"app\"\npath = \"src/app.rs\"\n");
    let files = [
        ("Cargo.toml", cargo.as_str()),
        ("src/main.rs", ""),
        ("src/app.rs", ""),
    ];
    assert_eq!(
        roots(&files, "src/a.rs"),
        vec![root("src/app.rs", Kind::Bin, None)]
    );
}

#[test]
fn bin_without_path_defaults_by_name() {
    let cargo = manifest("\n[[bin]]\nname = \"fx\"\n\n[[bin]]\nname = \"tool\"\n");
    let files = [
        ("Cargo.toml", cargo.as_str()),
        ("src/main.rs", ""),
        ("src/bin/tool.rs", ""),
    ];
    assert_eq!(
        roots(&files, "src/a.rs"),
        vec![
            root("src/main.rs", Kind::Bin, None),
            root("src/bin/tool.rs", Kind::Bin, None),
        ]
    );
}

#[test]
fn lib_with_test_false_is_no_test_target() {
    let cargo = manifest("\n[lib]\ntest = false\n");
    let files = [("Cargo.toml", cargo.as_str()), ("src/lib.rs", "")];
    assert_eq!(
        roots(&files, "src/a.rs"),
        vec![root("src/lib.rs", Kind::Lib, Some("test = false"))]
    );
}

#[test]
fn test_with_harness_false_is_no_test_target() {
    let cargo = manifest("\n[[test]]\nname = \"x\"\npath = \"tests/x.rs\"\nharness = false\n");
    let files = [("Cargo.toml", cargo.as_str()), ("tests/x.rs", "")];
    assert_eq!(
        roots(&files, "tests/x.rs"),
        vec![root("tests/x.rs", Kind::Test, Some("harness = false"))]
    );
}

#[test]
fn bin_with_required_features_is_no_test_target() {
    let cargo = manifest("\n[[bin]]\nname = \"fx\"\nrequired-features = [\"x\"]\n");
    let files = [("Cargo.toml", cargo.as_str()), ("src/main.rs", "")];
    assert_eq!(
        roots(&files, "src/a.rs"),
        vec![root("src/main.rs", Kind::Bin, Some("required-features"))]
    );
}

#[test]
fn edition_2015_bin_entry_turns_bin_discovery_off() {
    let entry = "\n[[bin]]\nname = \"app\"\npath = \"src/app.rs\"\n";
    let bins = |cargo: &str| {
        let files = [
            ("Cargo.toml", cargo),
            ("src/main.rs", ""),
            ("src/app.rs", ""),
        ];
        roots(&files, "src/a.rs")
    };
    let without_edition = format!("[package]\nname = \"fx\"\nversion = \"0.1.0\"\n{entry}");
    assert_eq!(
        bins(&without_edition),
        vec![root("src/app.rs", Kind::Bin, None)]
    );
    assert_eq!(
        bins(&manifest(entry)),
        vec![
            root("src/app.rs", Kind::Bin, None),
            root("src/main.rs", Kind::Bin, None),
        ]
    );
}

#[test]
fn an_inherited_edition_is_read_from_the_workspace() {
    let member = "[package]\nname = \"fx\"\nedition.workspace = true\n\n[[bin]]\nname = \"app\"\npath = \"src/app.rs\"\n";
    let bins = |edition: &str| {
        let workspace = format!(
            "[workspace]\nmembers = [\"a\"]\n\n[workspace.package]\nedition = \"{edition}\"\n"
        );
        let entries = [
            ("Cargo.toml", workspace.as_str()),
            ("a/Cargo.toml", member),
            ("a/src/main.rs", ""),
            ("a/src/app.rs", ""),
        ];
        roots(&entries, "a/src/x.rs").len()
    };
    assert_eq!(bins("2015"), 1, "2015 lists only the declared bin");
    assert_eq!(bins("2021"), 2, "2021 also discovers src/main.rs");
}

#[test]
fn a_workspace_member_owns_its_files() {
    let files = [
        ("Cargo.toml", "[workspace]\nmembers = [\"a\"]\n"),
        ("a/Cargo.toml", PACKAGE),
        ("a/src/lib.rs", ""),
    ];
    let source = tree(&files);
    let package = package_of(&source, "a/src/x.rs").expect("member a owns the file");
    assert_eq!((package.dir.as_str(), package.name.as_str()), ("a", "fx"));
    assert_eq!(
        test_roots(&source, &package, "a/src/x.rs"),
        vec![root("a/src/lib.rs", Kind::Lib, None)]
    );
}

#[test]
fn an_explicit_test_with_required_features_replaces_the_auto_root() {
    let cargo = manifest("\n[[test]]\nname = \"frozen\"\nrequired-features = [\"secret\"]\n");
    let files = [("Cargo.toml", cargo.as_str()), ("tests/frozen.rs", "")];
    assert_eq!(
        roots(&files, "tests/frozen.rs"),
        vec![root(
            "tests/frozen.rs",
            Kind::Test,
            Some("required-features")
        )]
    );
}

#[cfg(feature = "source-graph")]
#[test]
fn a_new_listed_test_is_its_own_root_when_autotests_is_off() {
    use super::super::derive_scaffold;
    let contract = [("tests/new_contract.rs", "rejects_x")];
    let listed = manifest("autotests = false\n\n[[test]]\nname = \"new_contract\"\npath = \"tests/new_contract.rs\"\n");
    let base = tree(&[("Cargo.toml", listed.as_str()), ("src/lib.rs", "")]);
    assert_eq!(derive_scaffold(&base, &contract), Ok(vec![]));

    let unlisted = manifest("autotests = false\n");
    let base = tree(&[("Cargo.toml", unlisted.as_str()), ("src/lib.rs", "")]);
    let refused = derive_scaffold(&base, &contract).expect_err("no target compiles it");
    assert!(refused.contains("no test target"), "{refused}");
}

#[test]
fn run_dir_selects_the_package_from_its_directory_or_below() {
    let source = tree(&[("Cargo.toml", PACKAGE), ("src/lib.rs", "")]);
    for run_dir in ["", ".", "src", "src/device_lock"] {
        let problems = run_dir_problems(&source, "src/device_lock/contract_tests.rs", run_dir);
        assert!(problems.is_empty(), "{run_dir}: {problems:?}");
    }
}

#[test]
fn run_dir_at_a_virtual_workspace_root_does_not_select_a_member() {
    let workspace = "[workspace]\nmembers = [\"a\", \"b\"]\ndefault-members = [\"b\"]\n";
    let source = tree(&[
        ("Cargo.toml", workspace),
        ("a/Cargo.toml", PACKAGE),
        ("b/Cargo.toml", PACKAGE),
    ]);
    let problems = run_dir_problems(&source, "a/tests/x_contract.rs", "");
    assert_eq!(problems.len(), 1, "{problems:?}");
    assert!(
        problems[0].contains("set the contract's cwd to a"),
        "{problems:?}"
    );
    assert!(run_dir_problems(&source, "a/tests/x_contract.rs", "a").is_empty());
}

#[test]
fn run_dir_at_a_root_package_left_out_of_default_members_is_refused() {
    let cargo = manifest("\n[workspace]\nmembers = [\"b\"]\ndefault-members = [\"b\"]\n");
    let source = tree(&[("Cargo.toml", cargo.as_str()), ("b/Cargo.toml", PACKAGE)]);
    assert_eq!(run_dir_problems(&source, "tests/x.rs", "").len(), 1);
}
