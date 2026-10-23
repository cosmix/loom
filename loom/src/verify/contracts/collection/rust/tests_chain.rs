use super::super::{chain_problems, derive_scaffold, tree};
use crate::verify::contracts::collection::{ScaffoldEdit, ScaffoldModule};

const MANIFEST: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";
const FILE: &str = "src/device_lock/contract_tests.rs";
const TEST: &str = "device_lock::contract_tests::rejects_x";
const BODY: &str = "#[test]\nfn rejects_x() {}\n";

/// The chain problems of a lib crate whose root is `lib` and whose
/// `src/device_lock/mod.rs` is `module_root`.
fn lib_chain(lib: &str, module_root: &str) -> Vec<String> {
    let source = tree(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", lib),
        ("src/device_lock/mod.rs", module_root),
        (FILE, BODY),
    ]);
    chain_problems(&source, FILE, TEST)
}

fn assert_names(problems: &[String], needle: &str) {
    assert!(
        problems.iter().any(|problem| problem.contains(needle)),
        "expected a problem naming {needle}: {problems:?}"
    );
}

#[test]
fn a_plain_chain_is_clean() {
    assert_eq!(
        lib_chain("mod device_lock;\n", "#[cfg(test)]\nmod contract_tests;\n"),
        Vec::<String>::new()
    );
}

#[test]
fn a_cfg_attr_path_link_is_refused() {
    let module_root = "#[cfg_attr(test, path = \"easy.rs\")]\nmod contract_tests;\n";
    assert_names(
        &lib_chain("mod device_lock;\n", module_root),
        "#[cfg_attr(test",
    );
}

#[test]
fn a_cfg_not_test_link_is_refused() {
    let module_root = "#[cfg(not(test))]\nmod contract_tests;\n";
    assert_names(
        &lib_chain("mod device_lock;\n", module_root),
        "#[cfg(not(test))]",
    );
}

#[test]
fn an_allow_attribute_is_accepted() {
    let module_root = "#[allow(dead_code)]\n#[cfg(test)]\nmod contract_tests;\n";
    assert!(lib_chain("mod device_lock;\n", module_root).is_empty());
}

#[test]
fn a_path_attribute_behind_a_comment_is_refused() {
    let module_root = "#[path = \"easy.rs\"]\n// note\n/* more */\nmod contract_tests;\n";
    assert_names(&lib_chain("mod device_lock;\n", module_root), "#[path");
}

#[test]
fn an_inline_module_on_the_chain_is_refused() {
    let source = tree(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/lib.rs",
            "mod device_lock {\n    mod contract_tests;\n}\n",
        ),
        (FILE, BODY),
    ]);
    assert_names(&chain_problems(&source, FILE, TEST), "inline");
}

#[test]
fn a_duplicated_declaration_is_refused() {
    let module_root = "#[cfg(test)]\nmod contract_tests;\n\n#[cfg(test)]\nmod contract_tests;\n";
    assert_names(&lib_chain("mod device_lock;\n", module_root), "2 times");
}

#[test]
fn an_inner_cfg_in_the_crate_root_is_refused() {
    let problems = lib_chain(
        "#![cfg(not(test))]\nmod device_lock;\n",
        "#[cfg(test)]\nmod contract_tests;\n",
    );
    assert_names(&problems, "src/lib.rs starts with #![cfg(not(test))]");
}

#[test]
fn an_inner_cfg_attr_is_refused() {
    let problems = lib_chain(
        "mod device_lock;\n",
        "#![cfg_attr(test, allow(unused))]\nmod contract_tests;\n",
    );
    assert_names(&problems, "#![cfg_attr");
}

#[test]
fn an_inner_cfg_test_in_a_module_root_is_accepted() {
    let problems = lib_chain("mod device_lock;\n", "#![cfg(test)]\nmod contract_tests;\n");
    assert!(problems.is_empty(), "{problems:?}");
}

#[test]
fn a_declaring_file_that_does_not_parse_is_refused() {
    let module_root = "mod contract_tests;\nfn broken( {\n";
    assert_names(
        &lib_chain("mod device_lock;\n", module_root),
        "does not parse",
    );
}

#[test]
fn a_missing_declaration_is_refused() {
    let problems = lib_chain("mod device_lock;\n", "pub fn f() {}\n");
    assert_names(
        &problems,
        "src/device_lock/mod.rs does not declare contract_tests",
    );
}

#[test]
fn a_test_path_outside_the_module_path_is_refused_by_both() {
    let problems = lib_chain("mod device_lock;\n", "#[cfg(test)]\nmod contract_tests;\n");
    assert!(problems.is_empty());
    let source = tree(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "mod device_lock;\n"),
        (
            "src/device_lock/mod.rs",
            "#[cfg(test)]\nmod contract_tests;\n",
        ),
    ]);
    assert_names(
        &chain_problems(&source, FILE, "elsewhere::rejects_x"),
        "does not start with device_lock::contract_tests",
    );
    let refused = derive_scaffold(&source, &[(FILE, "elsewhere::rejects_x")])
        .expect_err("no root compiles the file as elsewhere::rejects_x");
    assert!(refused.contains("does not start with"), "{refused}");
}

#[test]
fn an_integration_contract_is_its_own_root() {
    let file = "tests/x_contract.rs";
    let source = tree(&[("Cargo.toml", MANIFEST), ("src/lib.rs", ""), (file, BODY)]);
    assert!(chain_problems(&source, file, "rejects_x").is_empty());
    assert_eq!(derive_scaffold(&source, &[(file, "rejects_x")]), Ok(vec![]));

    let harnessless = format!(
        "{MANIFEST}\n[[test]]\nname = \"x_contract\"\npath = \"{file}\"\nharness = false\n"
    );
    let source = tree(&[("Cargo.toml", harnessless.as_str()), (file, BODY)]);
    assert_names(
        &chain_problems(&source, file, "rejects_x"),
        "harness = false",
    );
}

#[test]
fn a_contract_under_a_bin_directory_resolves_to_that_bin() {
    let file = "src/bin/tool/contract_tests.rs";
    let test = "contract_tests::rejects_x";
    let current = tree(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub fn f() {}\n"),
        (
            "src/bin/tool/main.rs",
            "#[cfg(test)]\nmod contract_tests;\n\nfn main() {}\n",
        ),
        (file, BODY),
    ]);
    assert!(chain_problems(&current, file, test).is_empty());

    let base = tree(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "pub fn f() {}\n"),
        ("src/bin/tool/main.rs", "fn main() {}\n"),
    ]);
    let declare = ScaffoldEdit::Declare {
        path: "src/bin/tool/main.rs".to_string(),
        module: ScaffoldModule {
            name: "contract_tests".to_string(),
            test_only: true,
        },
    };
    assert_eq!(derive_scaffold(&base, &[(file, test)]), Ok(vec![declare]));
}

#[test]
fn both_files_of_a_chain_module_are_refused() {
    let source = tree(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "mod device_lock;\n"),
        ("src/device_lock.rs", "#[cfg(test)]\nmod contract_tests;\n"),
        (
            "src/device_lock/mod.rs",
            "#[cfg(test)]\nmod contract_tests;\n",
        ),
        (FILE, BODY),
    ]);
    assert_names(
        &chain_problems(&source, FILE, TEST),
        "both src/device_lock.rs",
    );
}

#[test]
fn a_module_file_beside_the_contract_file_is_refused() {
    let source = tree(&[
        ("Cargo.toml", MANIFEST),
        ("src/lib.rs", "mod device_lock;\n"),
        (
            "src/device_lock/mod.rs",
            "#[cfg(test)]\nmod contract_tests;\n",
        ),
        ("src/device_lock/contract_tests/mod.rs", BODY),
    ]);
    assert_names(
        &chain_problems(&source, FILE, TEST),
        "src/device_lock/contract_tests/mod.rs exists",
    );
}

#[test]
fn a_contract_file_without_a_package_is_refused() {
    let source = tree(&[("src/lib.rs", "mod device_lock;\n")]);
    assert_eq!(
        chain_problems(&source, FILE, TEST),
        vec![format!("no Cargo package owns {FILE}")]
    );
}

/// rustc looks for the children of a flat crate root `tests/it.rs` in
/// `tests/`, so `tests/it/` is unreachable from it, even through `mod it;`.
#[test]
fn a_first_module_that_is_the_root_itself_is_refused_by_both() {
    let file = "tests/it/contracts.rs";
    let test = "it::contracts::rejects_x";
    let source = tree(&[
        ("Cargo.toml", MANIFEST),
        ("tests/it.rs", "mod it;\n#[cfg(test)]\nmod contracts;\n"),
        (file, BODY),
    ]);
    let refusal = format!("no test target of package fx compiles {file} as {test}");
    assert_eq!(chain_problems(&source, file, test), vec![refusal.clone()]);
    let base = tree(&[("Cargo.toml", MANIFEST), ("tests/it.rs", "")]);
    let refused = derive_scaffold(&base, &[(file, test)]).expect_err("tests/it/ is unreachable");
    assert!(refused.contains(&refusal), "{refused}");
}

#[test]
fn a_bin_directory_beside_its_flat_bin_root_is_refused() {
    let file = "src/bin/tool/x.rs";
    let test = "tool::x::rejects_x";
    let source = tree(&[
        ("Cargo.toml", MANIFEST),
        (
            "src/bin/tool.rs",
            "mod tool;\n#[cfg(test)]\nmod x;\n\nfn main() {}\n",
        ),
        (file, BODY),
    ]);
    assert_names(&chain_problems(&source, file, test), "no test target");
}

#[test]
fn a_directory_test_root_reaches_its_own_modules() {
    let file = "tests/it/contracts.rs";
    let test = "contracts::rejects_x";
    let current = tree(&[
        ("Cargo.toml", MANIFEST),
        ("tests/it/main.rs", "#[cfg(test)]\nmod contracts;\n"),
        (file, BODY),
    ]);
    assert!(chain_problems(&current, file, test).is_empty());
    let base = tree(&[("Cargo.toml", MANIFEST), ("tests/it/main.rs", "")]);
    let declare = ScaffoldEdit::Declare {
        path: "tests/it/main.rs".to_string(),
        module: ScaffoldModule {
            name: "contracts".to_string(),
            test_only: true,
        },
    };
    assert_eq!(derive_scaffold(&base, &[(file, test)]), Ok(vec![declare]));
}
