use super::*;

#[test]
fn default_file_names_follow_the_runners() {
    assert!(python_name("test_more.py") && python_name("more_test.py"));
    assert!(!python_name("helpers.py") && !python_name("test_more.txt"));
    assert!(javascript_name("a.test.ts") && javascript_name("a.spec.js"));
    assert!(!javascript_name("a.ts") && !javascript_name("test.ts"));
}

#[test]
fn path_modules_take_only_cfg_test_and_allow_beside_the_path() {
    let text = "#[cfg(test)]\n#[path = \"split_tests.rs\"]\nmod split;\n\
                #[cfg(feature = \"x\")]\n#[path = \"gated.rs\"]\nmod gated;\n\
                // #[path = \"commented.rs\"] mod commented;\n\
                #[allow(dead_code)] #[path = \"../up.rs\"] pub mod up;\n";
    let found = path_modules(text);
    assert_eq!(
        found,
        vec![
            ("split".to_string(), "split_tests.rs".to_string()),
            ("up".to_string(), "../up.rs".to_string()),
        ]
    );
}

#[test]
fn resolve_joins_and_refuses_escapes() {
    assert_eq!(resolve("src", "a.rs").as_deref(), Some("src/a.rs"));
    assert_eq!(resolve("src/x", "../a.rs").as_deref(), Some("src/a.rs"));
    assert_eq!(resolve("", "../a.rs"), None);
    assert_eq!(resolve("src", "/a.rs"), None);
}
