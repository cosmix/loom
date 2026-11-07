use super::path_modules as found;

fn pair(name: &str, path: &str) -> (String, String) {
    (name.to_string(), path.to_string())
}

#[test]
fn a_plain_cfg_test_path_declaration_is_taken() {
    let text = "#[cfg(test)]\n#[path = \"moved_tests.rs\"]\nmod tests;\n";
    assert_eq!(found(text), [pair("tests", "moved_tests.rs")]);
}

#[test]
fn lint_attributes_and_visibility_are_allowed_beside_the_path() {
    let text = "#[allow(dead_code)]\n#[expect(unused)]\n#[warn(x)]\n#[deny(y)]\n\
                #[path = \"a.rs\"]\npub(crate) mod a;\n";
    assert_eq!(found(text), [pair("a", "a.rs")]);
}

#[test]
fn another_cfg_or_a_second_path_or_no_path_is_refused() {
    let text = "#[cfg(any())]\n#[path = \"a.rs\"]\nmod a;\n\
                #[cfg(feature = \"x\")]\n#[path = \"b.rs\"]\nmod b;\n\
                #[cfg_attr(test, path = \"c.rs\")]\nmod c;\n\
                #[path = \"d.rs\"]\n#[path = \"e.rs\"]\nmod d;\n\
                #[cfg(test)]\nmod plain;\n";
    assert_eq!(found(text), Vec::<(String, String)>::new());
}

#[test]
fn declarations_that_are_not_top_level_items_are_ignored() {
    let text = "/*\n#[cfg(test)]\n#[path = \"a.rs\"]\nmod a;\n*/\n\
                // #[path = \"b.rs\"]\n// mod b;\n\
                #[cfg(any())]\nmod off {\n    #[path = \"c.rs\"]\n    mod c;\n}\n\
                const RAW: &str = r#\"\n#[path = \"d.rs\"]\nmod d;\n\"#;\n\
                macro_rules! m {\n    () => {\n        #[path = \"e.rs\"]\n        mod e;\n    };\n}\n\
                fn body() {\n    #[path = \"f.rs\"]\n    mod f;\n}\n";
    assert_eq!(found(text), Vec::<(String, String)>::new());
}

#[test]
fn a_path_spelled_with_an_escape_or_a_raw_string_is_refused() {
    let text = "#[path = \"a\\\\b.rs\"]\nmod a;\n#[path = r\"b.rs\"]\nmod b;\n";
    assert_eq!(found(text), Vec::<(String, String)>::new());
}

#[test]
fn a_file_that_does_not_parse_yields_nothing() {
    let text = "#[cfg(test)]\n#[path = \"a.rs\"]\nmod a;\nfn broken( {\n";
    assert_eq!(found(text), Vec::<(String, String)>::new());
}
