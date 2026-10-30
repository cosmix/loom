use super::super::tree;
use super::target_scope;
use crate::testrun::TargetScope;

const PACKAGE: &str = "[package]\nname = \"fx\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

fn scope(entries: &[(&str, &str)], file: &str, test: &str) -> Option<TargetScope> {
    target_scope(&tree(entries), file, test, "")
}

fn test_target(name: &str) -> Option<TargetScope> {
    Some(TargetScope::Test(name.to_string()))
}

mod resolved {
    use super::*;

    #[test]
    fn a_module_of_the_lib_scopes_to_the_lib_even_beside_a_main() {
        let files = [
            ("Cargo.toml", PACKAGE),
            ("src/lib.rs", "mod a;\n"),
            ("src/main.rs", "fn main() {}\n"),
            ("src/a.rs", ""),
        ];
        assert_eq!(scope(&files, "src/a.rs", "a::t"), Some(TargetScope::Lib));
    }

    #[test]
    fn an_integration_test_root_scopes_to_its_file_name() {
        let files = [
            ("Cargo.toml", PACKAGE),
            ("src/lib.rs", ""),
            ("tests/it.rs", ""),
        ];
        assert_eq!(scope(&files, "tests/it.rs", "t"), test_target("it"));
    }

    #[test]
    fn a_custom_test_path_scopes_to_the_entry_name() {
        let cargo = format!("{PACKAGE}\n[[test]]\nname = \"e2e\"\npath = \"tests/e2e/mod.rs\"\n");
        let files = [
            ("Cargo.toml", cargo.as_str()),
            ("src/lib.rs", ""),
            ("tests/e2e/mod.rs", ""),
        ];
        assert_eq!(scope(&files, "tests/e2e/mod.rs", "t"), test_target("e2e"));
    }

    #[test]
    fn a_test_directory_main_and_its_modules_scope_to_the_directory() {
        let files = [
            ("Cargo.toml", PACKAGE),
            ("src/lib.rs", ""),
            ("tests/suite/main.rs", "mod m;\n"),
            ("tests/suite/m.rs", ""),
        ];
        let want = test_target("suite");
        assert_eq!(scope(&files, "tests/suite/main.rs", "t"), want);
        assert_eq!(scope(&files, "tests/suite/m.rs", "m::t"), want);
    }

    #[test]
    fn bins_scope_by_their_name() {
        let tool = [
            ("Cargo.toml", PACKAGE),
            ("src/bin/tool.rs", "mod c;\n"),
            ("src/bin/c.rs", ""),
        ];
        let bin = |name: &str| Some(TargetScope::Bin(name.to_string()));
        assert_eq!(scope(&tool, "src/bin/tool.rs", "t"), bin("tool"));
        let main = [("Cargo.toml", PACKAGE), ("src/main.rs", "")];
        assert_eq!(scope(&main, "src/main.rs", "t"), bin("fx"));
    }

    #[test]
    fn a_package_below_the_source_root_scopes_when_run_from_it() {
        let files = [("pkg/Cargo.toml", PACKAGE), ("pkg/tests/it.rs", "")];
        let source = tree(&files);
        let file = "pkg/tests/it.rs";
        assert_eq!(target_scope(&source, file, "t", "pkg"), test_target("it"));
        assert_eq!(target_scope(&source, file, "t", ""), None, "wrong cwd");
    }

    #[test]
    fn an_uncertain_target_stays_unscoped() {
        let both = [
            ("Cargo.toml", PACKAGE),
            ("src/lib.rs", "mod a;\n"),
            ("src/main.rs", "mod a;\n"),
            ("src/a.rs", ""),
        ];
        assert_eq!(scope(&both, "src/a.rs", "a::t"), None, "two targets");
        let undeclared = [
            ("Cargo.toml", PACKAGE),
            ("src/lib.rs", ""),
            ("src/a.rs", ""),
        ];
        assert_eq!(scope(&undeclared, "src/a.rs", "a::t"), None);
        let lib = [
            ("Cargo.toml", PACKAGE),
            ("src/lib.rs", "mod a;\n"),
            ("src/a.rs", ""),
        ];
        assert_eq!(scope(&lib, "src/a.rs", "b::t"), None, "misspelt module");
        assert_eq!(scope(&lib, "src/a.rs", "src/a.rs - f (line 3)"), None);
        assert_eq!(scope(&[("src/a.rs", "")], "src/a.rs", "a::t"), None);
        let bench = [("Cargo.toml", PACKAGE), ("benches/b.rs", "")];
        assert_eq!(scope(&bench, "benches/b.rs", "t"), None);
    }

    #[test]
    fn a_file_that_is_its_own_root_does_not_take_its_stem_as_a_module() {
        let files = [
            ("Cargo.toml", PACKAGE),
            ("src/lib.rs", ""),
            ("tests/it.rs", "mod common;\n"),
            ("tests/common.rs", ""),
        ];
        assert_eq!(scope(&files, "tests/common.rs", "common::helper"), None);
        assert_eq!(
            scope(&files, "tests/common.rs", "helper"),
            test_target("common")
        );
    }

    #[test]
    fn a_dotfile_test_root_stays_unscoped() {
        let files = [
            ("Cargo.toml", PACKAGE),
            ("src/lib.rs", ""),
            ("tests/.x.rs", ""),
        ];
        assert_eq!(scope(&files, "tests/.x.rs", "t"), None);
    }
}
