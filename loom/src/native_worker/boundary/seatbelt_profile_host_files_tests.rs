//! The toolchain file rules of the macOS boundary profile, rendered on every
//! host: the directories above the worktree listable, the Cargo files there
//! hidden, each `package.json` there readable, the host Cargo home's state
//! files readable and its config hidden, the per-user Git files readable;
//! all after the read and write allows and before every protected or hidden
//! deny. Fixture files only.
use std::fs;
use std::path::{Path, PathBuf};

use super::super::seatbelt_profile::{profile, profile_with, quote, HostFiles};
use super::{at, scratch};

fn write(path: &Path) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("a fixture directory");
    fs::write(path, "[build]\n").expect("a fixture file");
}

/// `(allow file-read* (literal <path>))` for `path`.
fn read_literal(path: &Path) -> String {
    format!(
        "(allow file-read* (literal {}))",
        quote(path).expect("quoted")
    )
}

/// The metadata deny that hides `path`.
fn hidden_literal(path: &Path) -> String {
    let path = quote(path).expect("quoted");
    format!("(deny file-read-metadata file-test-existence (literal {path}))")
}

/// A scratch whose root, above the worktree, holds a manifest, a
/// `.cargo/config.toml` and a `package.json`, with a Cargo home holding
/// `config.toml` and its package-cache lock and a home with a Git ignore file.
fn fixture() -> (super::Scratch, HostFiles) {
    let fixture = scratch();
    let root = &fixture.root;
    for file in ["Cargo.toml", ".cargo/config.toml", "package.json"] {
        write(&root.join(file));
    }
    let cargo_home = root.join("cargo-home");
    write(&cargo_home.join("config.toml"));
    write(&cargo_home.join(".package-cache"));
    let home = root.join("home");
    write(&home.join(".config/git/ignore"));
    let host = HostFiles {
        cargo_home: Some(cargo_home),
        home: Some(home),
    };
    (fixture, host)
}

#[test]
fn the_toolchain_files_follow_the_allows_in_the_completion_profiles_order() {
    let (fixture, host) = fixture();
    let text = profile_with(&fixture.spec, &host).expect("a sound spec");
    let root = &fixture.root;
    let cargo_home = host.cargo_home.as_ref().expect("a Cargo home");
    let home = host.home.as_ref().expect("a home");
    let order = [
        at(&text, "(allow file-write-data (literal \"/dev/null\"))"),
        at(&text, &read_literal(root)),
        at(&text, &hidden_literal(&root.join("Cargo.toml"))),
        at(&text, &hidden_literal(&root.join(".cargo/config.toml"))),
        at(&text, &read_literal(&root.join("package.json"))),
        at(&text, &read_literal(&cargo_home.join(".package-cache"))),
        at(&text, &hidden_literal(&cargo_home.join("config.toml"))),
        at(&text, &read_literal(&home.join(".config/git/ignore"))),
        at(&text, "(deny file-write* (subpath"),
        at(&text, "(deny file-read* file-write*"),
    ];
    for pair in order.windows(2) {
        assert!(pair[0] < pair[1], "rules out of order {order:?}:\n{text}");
    }
}

#[test]
fn the_worktree_and_its_contents_are_never_listed_as_ancestors() {
    let (fixture, host) = fixture();
    let text = profile_with(&fixture.spec, &host).expect("a sound spec");
    let worktree = &fixture.spec.worktree;
    assert!(!text.contains(&read_literal(worktree)), "{text}");
    assert!(
        !text.contains(&hidden_literal(&worktree.join("Cargo.toml"))),
        "{text}"
    );
}

#[test]
fn without_host_homes_no_cargo_home_or_git_file_is_named() {
    let (fixture, host) = fixture();
    let text = profile(&fixture.spec).expect("a sound spec");
    let cargo_home: &PathBuf = host.cargo_home.as_ref().expect("a Cargo home");
    assert!(!text.contains(&*cargo_home.to_string_lossy()), "{text}");
    assert!(!text.contains(".config/git"), "{text}");
    assert!(
        text.contains(&hidden_literal(&fixture.root.join(".cargo/config.toml"))),
        "the ancestors are derived from the worktree alone:\n{text}"
    );
}
