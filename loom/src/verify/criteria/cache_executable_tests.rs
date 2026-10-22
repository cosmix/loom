//! Which programs a shell line runs, as the pass cache resolves them: `env`
//! that only adjusts the environment hands resolution to its command.

use std::ffi::OsStr;
use std::path::Path;

use super::{resolve_named, shell_command_names, Search, DEFAULT_EXEC_PATH};

fn names(line: &str) -> Option<Vec<(String, Search)>> {
    let names = shell_command_names(line)?;
    Some(
        names
            .into_iter()
            .map(|name| (name.word, name.search))
            .collect(),
    )
}

fn env_then(command: &str, search: Search) -> Option<Vec<(String, Search)>> {
    Some(vec![
        ("env".to_string(), Search::Command),
        (command.to_string(), search),
    ])
}

#[test]
fn env_that_only_adjusts_the_environment_resolves_its_command() {
    let unset = "env -u GIT_INDEX_FILE -u GIT_DIR --unset=GIT_WORK_TREE cargo test --lib";
    assert_eq!(names(unset), env_then("cargo", Search::Command));
    assert_eq!(
        names("env A=1 B=2 cargo"),
        env_then("cargo", Search::Command)
    );
    assert_eq!(
        names("env -i A=1 cargo"),
        env_then("cargo", Search::Default)
    );
    let ignore = "env --ignore-environment cargo";
    assert_eq!(names(ignore), env_then("cargo", Search::Default));
    assert_eq!(
        names("env -u PATH cargo"),
        env_then("cargo", Search::Default)
    );
    assert_eq!(names("env echo hi"), env_then("echo", Search::Command));
}

#[test]
fn env_doing_anything_else_stays_uncertain() {
    for line in [
        "env",
        "env -u X",
        "env -u X; cargo test",
        "env -S 'cargo test'",
        "env --split-string=cargo",
        "env -C / cargo",
        "env --chdir=/ cargo",
        "env -0",
        "env - cargo",
        "env -- cargo",
        // Options end at the first operand: this env runs a program `-i`.
        "env A=1 -i cargo",
        "env PATH=/tmp cargo",
        "env -i PATH=/tmp cargo",
        "env env cargo",
        "env time cargo",
    ] {
        assert_eq!(names(line), None, "{line}");
    }
}

#[test]
fn a_command_env_runs_without_path_resolves_in_the_default_search_path() {
    let default = Some(OsStr::new(DEFAULT_EXEC_PATH));
    let resolved = resolve_named(OsStr::new("sh"), default, Path::new("/")).unwrap();
    let directory = resolved.parent().unwrap();
    assert!(std::env::split_paths(DEFAULT_EXEC_PATH).any(|entry| entry == directory));
}
