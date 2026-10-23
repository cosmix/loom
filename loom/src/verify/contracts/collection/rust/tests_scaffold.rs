use super::super::{scaffold_problems, tree};
use crate::verify::contracts::collection::{ScaffoldEdit, ScaffoldModule};

const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const BASE_MAIN: &str = "mod devices;\n\nfn main() {}\n";

fn module(name: &str, test_only: bool) -> ScaffoldModule {
    ScaffoldModule {
        name: name.to_string(),
        test_only,
    }
}

fn declare(path: &str, name: &str, test_only: bool) -> ScaffoldEdit {
    ScaffoldEdit::Declare {
        path: path.to_string(),
        module: module(name, test_only),
    }
}

fn create(paths: &[&str], modules: Vec<ScaffoldModule>) -> ScaffoldEdit {
    ScaffoldEdit::CreateRoot {
        paths: paths.iter().map(|path| path.to_string()).collect(),
        modules,
    }
}

/// The liquidmon scaffold: declare `device_lock` in `src/main.rs` and create
/// its root holding `contract_tests`.
fn liquidmon_edits() -> Vec<ScaffoldEdit> {
    vec![
        declare("src/main.rs", "device_lock", false),
        create(
            &["src/device_lock/mod.rs", "src/device_lock.rs"],
            vec![module("contract_tests", true)],
        ),
    ]
}

/// The diff of `current` against the liquidmon base.
fn liquidmon_problems(current: &[(&str, &str)]) -> Vec<String> {
    let base = tree(&[("Cargo.toml", MANIFEST), ("src/main.rs", BASE_MAIN)]);
    scaffold_problems(&liquidmon_edits(), &base, &tree(current))
}

#[cfg(feature = "source-graph")]
mod derivation {
    use super::super::super::derive_scaffold;
    use super::*;

    #[test]
    fn a_two_level_new_module_declares_one_and_creates_two_roots() {
        let base = tree(&[("Cargo.toml", MANIFEST), ("src/lib.rs", "")]);
        let contract = [(
            "src/a/b/contract_tests.rs",
            "a::b::contract_tests::rejects_x",
        )];
        assert_eq!(
            derive_scaffold(&base, &contract),
            Ok(vec![
                declare("src/lib.rs", "a", false),
                create(&["src/a/mod.rs", "src/a.rs"], vec![module("b", false)]),
                create(
                    &["src/a/b/mod.rs", "src/a/b.rs"],
                    vec![module("contract_tests", true)]
                ),
            ])
        );
    }

    #[test]
    fn two_contracts_in_one_new_module_share_its_root() {
        let base = tree(&[("Cargo.toml", MANIFEST), ("src/lib.rs", "")]);
        let contracts = [
            ("src/a/x_tests.rs", "a::x_tests::rejects_x"),
            ("src/a/y_tests.rs", "a::y_tests::rejects_y"),
        ];
        assert_eq!(
            derive_scaffold(&base, &contracts),
            Ok(vec![
                declare("src/lib.rs", "a", false),
                create(
                    &["src/a/mod.rs", "src/a.rs"],
                    vec![module("x_tests", true), module("y_tests", true)]
                ),
            ])
        );
    }

    #[test]
    fn a_tests_support_module_is_offered_only_as_mod_rs() {
        let cargo = format!("{MANIFEST}\n[[test]]\nname = \"it\"\npath = \"tests/it.rs\"\n");
        let base = tree(&[
            ("Cargo.toml", cargo.as_str()),
            ("tests/it.rs", "#[test]\nfn smoke() {}\n"),
        ]);
        let contract = [("tests/support/x.rs", "support::x::rejects_x")];
        assert_eq!(
            derive_scaffold(&base, &contract),
            Ok(vec![
                declare("tests/it.rs", "support", false),
                create(&["tests/support/mod.rs"], vec![module("x", true)]),
            ])
        );
    }

    #[test]
    fn the_root_needing_fewest_edits_wins() {
        let base = tree(&[
            ("Cargo.toml", MANIFEST),
            ("src/lib.rs", ""),
            ("src/main.rs", "mod device_lock;\n\nfn main() {}\n"),
            ("src/device_lock.rs", ""),
        ]);
        let contract = [(
            "src/device_lock/contract_tests.rs",
            "device_lock::contract_tests::rejects_x",
        )];
        assert_eq!(
            derive_scaffold(&base, &contract),
            Ok(vec![declare("src/device_lock.rs", "contract_tests", true)])
        );
    }
}

#[test]
fn the_liquidmon_scaffold_with_blank_lines_and_crlf_is_accepted() {
    let problems = liquidmon_problems(&[
        (
            "src/main.rs",
            "mod devices;\r\n\r\nmod device_lock;\r\n\r\n\r\nfn main() {}\r\n",
        ),
        (
            "src/device_lock/mod.rs",
            "\r\n#[cfg(test)]\r\nmod contract_tests;\r\n",
        ),
    ]);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn a_public_declaration_in_the_flat_root_is_accepted() {
    let problems = liquidmon_problems(&[
        (
            "src/main.rs",
            "pub(crate) mod device_lock;\nmod devices;\n\nfn main() {}\n",
        ),
        ("src/device_lock.rs", "pub mod contract_tests;\n"),
    ]);
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn production_code_beside_a_declaration_is_refused() {
    let problems = liquidmon_problems(&[
        (
            "src/main.rs",
            "mod device_lock;\nmod devices;\n\nfn main() {}\nfn extra() {}\n",
        ),
        ("src/device_lock/mod.rs", "mod contract_tests;\n"),
    ]);
    assert_eq!(
        problems,
        vec![
            "src/main.rs: extra line `fn extra() {}`: a scaffold file may only gain the derived mod declarations"
                .to_string()
        ]
    );
}

#[test]
fn a_changed_base_line_is_refused() {
    let problems = liquidmon_problems(&[
        (
            "src/main.rs",
            "mod device_lock;\npub mod devices;\n\nfn main() {}\n",
        ),
        ("src/device_lock/mod.rs", "mod contract_tests;\n"),
    ]);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("line `mod devices;` of the stage base is missing")),
        "{problems:?}"
    );
}

#[test]
fn a_declaration_that_takes_a_base_attribute_is_refused() {
    let base = tree(&[("src/lib.rs", "#[cfg(test)]\nmod tests;\n")]);
    let current = tree(&[("src/lib.rs", "#[cfg(test)]\nmod device_lock;\nmod tests;\n")]);
    let edits = [declare("src/lib.rs", "device_lock", false)];
    assert!(!scaffold_problems(&edits, &base, &current).is_empty());

    let current = tree(&[(
        "src/lib.rs",
        "#[allow(unused)]\nmod device_lock;\nmod tests;\n",
    )]);
    let base = tree(&[("src/lib.rs", "#[allow(unused)]\nmod tests;\n")]);
    let problems = scaffold_problems(&edits, &base, &current);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("sits under `#[allow(unused)]`")),
        "{problems:?}"
    );
}

#[test]
fn a_scaffold_root_must_be_created_exactly_once() {
    let missing = liquidmon_problems(&[(
        "src/main.rs",
        "mod device_lock;\nmod devices;\n\nfn main() {}\n",
    )]);
    assert_eq!(missing.len(), 1, "{missing:?}");
    assert!(missing[0].starts_with("none of src/device_lock/mod.rs, src/device_lock.rs exists"));

    let both = liquidmon_problems(&[
        ("src/device_lock/mod.rs", "mod contract_tests;\n"),
        ("src/device_lock.rs", "mod contract_tests;\n"),
    ]);
    assert!(
        both.iter().any(|problem| problem.contains("all exist")),
        "{both:?}"
    );
}

#[test]
fn a_scaffold_root_present_at_base_is_refused() {
    let base = tree(&[("src/device_lock.rs", "")]);
    let current = tree(&[("src/device_lock.rs", "mod contract_tests;\n")]);
    let edits = [create(
        &["src/device_lock/mod.rs", "src/device_lock.rs"],
        vec![module("contract_tests", true)],
    )];
    let problems = scaffold_problems(&edits, &base, &current);
    assert!(
        problems
            .iter()
            .any(|problem| problem.contains("exists in the stage base")),
        "{problems:?}"
    );
}

#[test]
fn a_removed_declaring_file_is_refused() {
    let problems = liquidmon_problems(&[("src/device_lock/mod.rs", "mod contract_tests;\n")]);
    assert!(
        problems
            .iter()
            .any(|problem| problem.starts_with("src/main.rs was removed")),
        "{problems:?}"
    );
}
