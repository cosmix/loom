//! The macOS boundary profile beyond its frozen contract: spellings, the
//! refusals, rule order and the network grants, on every host; on macOS the
//! profile compiles under `sandbox-exec`, and the kernel tests in
//! `seatbelt_profile_kernel_tests.rs` plus [`native_macos_worker_lifecycle`]
//! apply it. Fixture credentials only.
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};

use super::seatbelt_profile::{profile, quote, spellings, SeatbeltBoundarySpec};

const PORT: u16 = 40123;

/// A canonical scratch with a worktree, a private root and a fake main
/// checkout whose state root the worktree's `.loom/work` links to.
pub(super) struct Scratch {
    _dir: tempfile::TempDir,
    pub(super) root: PathBuf,
    pub(super) spec: SeatbeltBoundarySpec,
}

pub(super) fn scratch() -> Scratch {
    let dir = tempfile::tempdir().expect("a temp dir");
    let root = dir.path().canonicalize().expect("canonical");
    let worktree = root.join("worktree");
    let codex_home = root.join("private/codex-home");
    let main_loom = root.join("main/.loom");
    for path in [
        worktree.join("src"),
        worktree.join(".loom"),
        worktree.join(".git"),
        codex_home.join("sessions"),
        root.join("private/tmp"),
        main_loom.join("work/context"),
        main_loom.join("work/secrets"),
        main_loom.join("cache/context-v1"),
        root.join("proxy"),
    ] {
        fs::create_dir_all(&path).expect("a fixture directory");
    }
    symlink(main_loom.join("work"), worktree.join(".loom/work")).expect("state link");
    let spec = SeatbeltBoundarySpec {
        protected: [".git", ".loom", ".claude", ".codex"]
            .map(|name| worktree.join(name))
            .into(),
        hidden: vec![worktree.join(".loom/work")],
        tmp: root.join("private/tmp"),
        codex_home,
        read_roots: vec![PathBuf::from("/usr"), PathBuf::from("/bin")],
        proxy_socket: root.join("proxy/p.sock"),
        bridge_port: PORT,
        worktree,
    };
    Scratch {
        _dir: dir,
        root,
        spec,
    }
}

fn refused(spec: &SeatbeltBoundarySpec, needle: &str) {
    let error = profile(spec).expect_err("the spec must be refused");
    assert!(format!("{error:#}").contains(needle), "{error:#}");
}

#[test]
fn spellings_cover_both_directions_and_the_data_firmlink() {
    let forms = spellings(Path::new("/var/folders/x"));
    for form in [
        "/var/folders/x",
        "/private/var/folders/x",
        "/System/Volumes/Data/var/folders/x",
        "/System/Volumes/Data/private/var/folders/x",
    ] {
        assert!(forms.contains(&PathBuf::from(form)), "{form}: {forms:?}");
    }
    let etc = spellings(Path::new("/private/etc"));
    assert!(etc.contains(&PathBuf::from("/etc")), "{etc:?}");
    let plain = spellings(Path::new("/Users/u/w"));
    assert_eq!(
        plain,
        [
            PathBuf::from("/Users/u/w"),
            PathBuf::from("/System/Volumes/Data/Users/u/w")
        ]
    );
    let variable = spellings(Path::new("/variable/x"));
    assert_eq!(
        variable.len(),
        2,
        "a prefix that is not a component: {variable:?}"
    );
}

#[test]
fn quoting_refuses_control_characters() {
    assert!(quote(Path::new("/tmp/a\nb")).is_err());
    assert_eq!(
        quote(Path::new("/a \"b\"")).expect("quoted"),
        "\"/a \\\"b\\\"\""
    );
}

#[test]
fn a_symlinked_or_missing_writable_root_is_refused() {
    let mut fixture = scratch();
    let alias = fixture.root.join("alias");
    symlink(&fixture.spec.worktree, &alias).expect("alias");
    fixture.spec.worktree = alias;
    refused(&fixture.spec, "not canonical");
    let fixture = scratch();
    let sessions = fixture.spec.codex_home.join("sessions");
    fs::remove_dir(&sessions).expect("unlink sessions");
    symlink(&fixture.spec.tmp, &sessions).expect("link sessions");
    refused(&fixture.spec, "not canonical");
}

#[test]
fn overlapping_writable_roots_are_refused() {
    let mut fixture = scratch();
    fixture.spec.tmp = fixture.spec.worktree.join("src");
    refused(&fixture.spec, "overlap");
}

#[test]
fn a_protected_or_hidden_path_covering_a_writable_root_is_refused() {
    let mut fixture = scratch();
    fixture.spec.protected.push(fixture.root.join("private"));
    refused(&fixture.spec, "covers");
    let mut fixture = scratch();
    fixture.spec.hidden.push(fixture.root.clone());
    refused(&fixture.spec, "covers");
}

#[test]
fn a_proxy_socket_below_a_writable_root_or_an_unbound_port_is_refused() {
    let mut fixture = scratch();
    fixture.spec.proxy_socket = fixture.spec.tmp.join("p.sock");
    refused(&fixture.spec, "proxy socket");
    let mut fixture = scratch();
    fixture.spec.bridge_port = 0;
    refused(&fixture.spec, "bridge port");
}

/// The byte offset of `needle` in `text`, which must hold it.
fn at(text: &str, needle: &str) -> usize {
    text.find(needle)
        .unwrap_or_else(|| panic!("{needle} missing from:\n{text}"))
}

#[test]
fn denies_follow_the_allows_they_refine() {
    let fixture = scratch();
    let text = profile(&fixture.spec).expect("a sound spec");
    let worktree = fixture.spec.worktree.to_string_lossy().into_owned();
    let write_allow = at(&text, "(allow file-write*");
    let protected = at(
        &text,
        &format!("(deny file-write* (subpath \"{worktree}/.git\")"),
    );
    assert!(write_allow < protected, "{text}");
    let target = fixture.root.join("main/.loom/work");
    let hidden = at(&text, "(deny file-read* file-write*");
    let named_target = at(&text, &format!("\"{}\"", target.display()));
    assert!(
        hidden < named_target,
        "the canonical state root is hidden:\n{text}"
    );
    let overlay = target.join("context");
    let map = at(&text, &format!("(subpath \"{}\")", overlay.display()));
    assert!(
        hidden < map,
        "the map store is re-allowed after the hide:\n{text}"
    );
    let cache = fixture.root.join("main/.loom/cache/context-v1");
    assert!(
        text.contains(&format!("(subpath \"{}\")", cache.display())),
        "{text}"
    );
    assert!(!text.contains("secrets"), "{text}");
}

#[test]
fn the_network_grants_name_one_port_and_bind_nothing() {
    let fixture = scratch();
    let text = profile(&fixture.spec).expect("a sound spec");
    assert!(!text.contains("network-bind"), "{text}");
    assert!(!text.contains("localhost:*"), "{text}");
    assert!(!text.contains("(allow network*"), "{text}");
    assert!(!text.contains("(allow network-outbound)"), "{text}");
    assert_eq!(
        text.matches(&format!("localhost:{PORT}")).count(),
        2,
        "{text}"
    );
    let socket = fixture.spec.proxy_socket.to_string_lossy().into_owned();
    assert!(
        text.contains(&format!("(path-literal \"{socket}\")")),
        "{text}"
    );
    assert!(!text.contains("(allow signal)"), "{text}");
    assert!(!text.contains("(allow mach-lookup)"), "{text}");
}

/// `sandbox-exec` compiles the profile before applying it, and reports a
/// compile error differently from an apply refusal (a nested sandbox), so
/// the syntax is checked even inside a stage session.
#[cfg(target_os = "macos")]
#[test]
fn the_profile_compiles_under_sandbox_exec() {
    let fixture = scratch();
    let text = profile(&fixture.spec).expect("a sound spec");
    let output = std::process::Command::new("/usr/bin/sandbox-exec")
        .arg("-p")
        .arg(&text)
        .arg("/usr/bin/true")
        .output()
        .expect("run sandbox-exec");
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        output.status.success() || stderr.contains("sandbox_apply"),
        "the profile does not compile: {stderr}\n{text}"
    );
}

/// The production launch, supervisor and teardown against an installed
/// Codex with fixture credentials and an offline initialize and session
/// operation (`supervisor::lifecycle_tests::run_lifecycle`). Ignored by
/// default: hosted macOS CI has no Codex. The operator's run includes it
/// and fails without one.
#[cfg(target_os = "macos")]
#[test]
#[ignore = "needs an installed Codex and a plain macOS shell"]
#[serial_test::serial]
fn native_macos_worker_lifecycle() {
    if !crate::process::sandbox_probe::confined_commands_runnable() {
        return;
    }
    let codex = crate::codex::find_codex_path()
        .and_then(|path| Ok(path.canonicalize()?))
        .expect("the operator run of this test needs an installed codex");
    let report = crate::native_worker::supervisor::lifecycle_tests::run_lifecycle(&codex);
    assert!(
        report.exited_within_bound,
        "the worker did not exit within its bound"
    );
    assert!(
        report.ownership_released,
        "the worker's ownership was not released"
    );
    assert!(
        report.private_root_removed,
        "the private invocation root was left behind"
    );
}

#[cfg(target_os = "macos")]
#[path = "seatbelt_profile_kernel_tests.rs"]
mod kernel;

#[path = "seatbelt_profile_host_files_tests.rs"]
mod host_files;
