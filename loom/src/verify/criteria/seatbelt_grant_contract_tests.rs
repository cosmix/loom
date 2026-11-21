//! Contracts `signals-only-own-sandbox`, `binds-only-localhost` and
//! `ancestors-listed-not-read`: the macOS Seatbelt profile as the portable
//! builder renders it on any host (`confine::host::seatbelt_profile`), read
//! rule by rule. Seatbelt applies the last rule that matches an operation, so
//! a rule's position decides what it grants. On macOS the profile the real
//! launcher would receive carries the same grants. Nothing here starts a
//! sandbox.

use std::path::{Path, PathBuf};
use std::process::Command;

use tempfile::TempDir;

use super::confine::host::{seatbelt_profile, NetworkAccess};

const SIGNAL_RULE: &str = "(allow signal (target same-sandbox))";
const LOCALHOST_LISTENER: &str = "(allow network-bind network-inbound (local ip \"localhost:*\"))";

/// A canonical `<tmp>/outer/mid/worktree` whose `outer` holds `Cargo.toml`
/// and `.cargo/config.toml`; returns the guard, `outer` and the tree.
fn nested_tree() -> (TempDir, PathBuf, PathBuf) {
    let root = TempDir::new().unwrap();
    let outer = root.path().canonicalize().unwrap().join("outer");
    std::fs::create_dir_all(outer.join(".cargo")).unwrap();
    std::fs::write(outer.join("Cargo.toml"), "[workspace]\n").unwrap();
    std::fs::write(outer.join(".cargo/config.toml"), "[build]\n").unwrap();
    let tree = outer.join("mid/worktree");
    std::fs::create_dir_all(&tree).unwrap();
    (root, outer, tree)
}

/// The profile a confined `/bin/sh` in `tree` runs under with `network`.
fn profile(tree: &Path, network: NetworkAccess) -> String {
    let mut original = Command::new("/bin/sh");
    original.current_dir(tree);
    crate::process::apply_stage_environment(&mut original);
    seatbelt_profile(&original, tree, network).expect("render the Seatbelt profile")
}

/// The profile's top-level forms in order; a string literal may hold parens.
fn rules(profile: &str) -> Vec<&str> {
    let (mut rules, mut depth, mut start) = (Vec::new(), 0usize, 0usize);
    let (mut quoted, mut escaped) = (false, false);
    for (at, c) in profile.char_indices() {
        if quoted {
            match (escaped, c) {
                (true, _) => escaped = false,
                (false, '\\') => escaped = true,
                (false, '"') => quoted = false,
                _ => {}
            }
            continue;
        }
        match c {
            '"' => quoted = true,
            '(' => {
                if depth == 0 {
                    start = at;
                }
                depth += 1;
            }
            ')' => {
                depth = depth.saturating_sub(1);
                if depth == 0 {
                    rules.push(&profile[start..=at]);
                }
            }
            _ => {}
        }
    }
    rules
}

fn quoted(path: &Path) -> String {
    serde_json::to_string(&path.to_string_lossy()).unwrap()
}

/// Whether the allow `rule` grants some read of `file`: an unfiltered file
/// read rule, a literal naming it, or a subpath holding it.
fn grants_read_of(rule: &str, file: &Path) -> bool {
    let read = ["(allow file-read", "(allow file-test"];
    if !read.iter().any(|prefix| rule.starts_with(prefix)) {
        return false;
    }
    if !rule[1..].contains('(') {
        return true;
    }
    rule.contains(&format!("(literal {})", quoted(file)))
        || file
            .ancestors()
            .any(|dir| rule.contains(&format!("(subpath {})", quoted(dir))))
}

/// The profile the real launcher would receive, where it is installed.
#[cfg(target_os = "macos")]
fn launched_profile(tree: &Path) -> Option<String> {
    use crate::verify::criteria::{confined_policy, host_launcher, CommandSpec};
    host_launcher()?;
    let (repo, home) = (TempDir::new().unwrap(), TempDir::new().unwrap());
    let (repo_root, home_root) = (repo.path().canonicalize(), home.path().canonicalize());
    let cache = crate::stage_cache::StageCache::resolve(
        &repo_root.unwrap(),
        "s",
        &home_root.unwrap(),
        None,
    )
    .expect("resolve a stage cache");
    let policy = confined_policy(&CommandSpec::shell("true"), tree, &cache)
        .expect("build the launcher policy");
    let at = policy.args.iter().position(|arg| arg == "-p");
    Some(policy.args[at.expect("the launcher takes the profile after -p") + 1].clone())
}

#[test]
fn a_confined_command_may_signal_only_its_own_sandbox() {
    let (_root, _outer, tree) = nested_tree();
    for network in [NetworkAccess::Denied, NetworkAccess::Provision] {
        let profile = profile(&tree, network);
        let rules = rules(&profile);
        let deny = rules.iter().position(|rule| *rule == "(deny default)");
        let signal = rules.iter().position(|rule| *rule == SIGNAL_RULE);
        assert!(deny.is_some(), "the profile denies by default");
        assert!(
            signal > deny,
            "a confined command may signal its own sandbox"
        );
        let signals: Vec<&str> = rules
            .iter()
            .copied()
            .filter(|rule| rule.starts_with("(allow signal") || rule.starts_with("(allow default"))
            .collect();
        assert_eq!(signals, [SIGNAL_RULE], "no other signal grant exists");
        assert!(
            rules.contains(&LOCALHOST_LISTENER),
            "a confined command may listen on localhost"
        );
    }
    #[cfg(target_os = "macos")]
    if let Some(launched) = launched_profile(&tree) {
        let launched = rules(&launched);
        assert!(
            launched.contains(&SIGNAL_RULE) && launched.contains(&LOCALHOST_LISTENER),
            "the launcher receives the same signal and listener grants"
        );
    }
}

#[test]
fn a_confined_command_may_bind_only_localhost() {
    let (_root, _outer, tree) = nested_tree();
    let denied = profile(&tree, NetworkAccess::Denied);
    let denied = rules(&denied);
    assert!(
        denied.contains(&LOCALHOST_LISTENER),
        "a denied-network command may listen on localhost"
    );
    for rule in denied
        .iter()
        .filter(|rule| rule.starts_with("(allow network"))
    {
        let unix_only =
            rule.contains("unix-socket") && !rule.contains(" ip ") && !rule.contains("tcp");
        assert!(
            *rule == LOCALHOST_LISTENER || unix_only,
            "denied network grants localhost listening and Unix sockets only: {rule}"
        );
    }
    let provision = profile(&tree, NetworkAccess::Provision);
    let provision = rules(&provision);
    assert!(
        provision.contains(&LOCALHOST_LISTENER) && provision.contains(&"(allow network-outbound)"),
        "provisioning listens on localhost and keeps its outbound access"
    );
    let listeners = provision.iter().filter(|rule| {
        rule.starts_with("(allow")
            && (rule.contains("network-bind") || rule.contains("network-inbound"))
    });
    for rule in listeners {
        assert!(
            *rule == LOCALHOST_LISTENER || rule.contains("unix-socket"),
            "provisioning binds no address beyond localhost: {rule}"
        );
    }
}

/// Every Cargo file in `files` is denied for reads, metadata and existence,
/// and no rule after the first deny grants it again.
fn assert_hidden(rules: &[&str], files: &[PathBuf]) {
    for file in files {
        let path = quoted(file);
        let read = format!("(deny file-read* (literal {path}))");
        let metadata = format!("(deny file-read-metadata file-test-existence (literal {path}))");
        let denies = [read, metadata].map(|deny| rules.iter().position(|rule| *rule == deny));
        assert!(
            denies.iter().all(Option::is_some),
            "{} is denied",
            file.display()
        );
        let first_deny = denies.iter().flatten().min().copied().unwrap_or_default();
        assert!(
            !rules[first_deny..]
                .iter()
                .any(|rule| grants_read_of(rule, file)),
            "no later rule makes {} visible",
            file.display()
        );
    }
}

#[test]
fn ancestor_directories_are_listed_but_cargo_files_above_stay_hidden() {
    let (root, outer, tree) = nested_tree();
    let profile = profile(&tree, NetworkAccess::Denied);
    let rules = rules(&profile);
    for dir in tree
        .ancestors()
        .skip(1)
        .filter(|dir| dir.parent().is_some())
    {
        let listing = format!("(allow file-read* (literal {}))", quoted(dir));
        assert!(
            rules.contains(&listing.as_str()),
            "{} can be listed",
            dir.display()
        );
    }
    let fixture = root.path().canonicalize().unwrap();
    for dir in tree
        .ancestors()
        .skip(1)
        .take_while(|dir| dir.starts_with(&fixture))
    {
        let subpath = format!("(subpath {})", quoted(dir));
        assert!(
            !rules
                .iter()
                .any(|rule| rule.starts_with("(allow") && rule.contains(&subpath)),
            "nothing grants the files below {}",
            dir.display()
        );
    }
    assert_hidden(
        &rules,
        &[outer.join("Cargo.toml"), outer.join(".cargo/config.toml")],
    );
}
