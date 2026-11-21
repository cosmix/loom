//! Cargo inside the macOS boundary, below a project and with a host Cargo
//! home that both hold `config.toml` files the boundary may not read. The
//! profile hides them as the completion profile does, so Cargo finds its
//! package instead of failing on an unreadable config. Skips as its
//! neighbours do. Fixture files only.
use std::ffi::OsString;
use std::fs;
use std::os::unix::net::UnixListener;
use std::path::{Path, PathBuf};

use serial_test::serial;

use super::{built_loom, run_released, verdict, FIXTURE_AUTH};
use crate::native_worker::boundary::BoundarySpec;
use crate::process::sandbox_probe::skip_unless;
use crate::stage_cache::StageCache;

const MANIFEST: &str = "[package]\nname = \"fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n";

/// Locates the package from the worktree with the toolchain's own `cargo`.
const PAYLOAD: &str = r#"cd "$1" || exit 9
"$2" locate-project --message-format plain
echo "cargo_exit=$?""#;

fn write(path: &Path, text: &str) {
    fs::create_dir_all(path.parent().expect("a parent")).expect("a fixture directory");
    fs::write(path, text).expect("a fixture file");
}

/// `<root>/repo` with its own manifest and `.cargo/config.toml`, a worktree
/// at `<root>/repo/.worktrees/s1` and a Cargo home holding `config.toml`.
fn project(root: &Path) -> (PathBuf, PathBuf) {
    let repo = root.join("repo");
    let worktree = repo.join(".worktrees/s1");
    write(&repo.join("Cargo.toml"), "[workspace]\nmembers = []\n");
    write(&repo.join(".cargo/config.toml"), "[build]\n");
    write(&worktree.join("Cargo.toml"), MANIFEST);
    write(&worktree.join("src/lib.rs"), "");
    fs::create_dir_all(worktree.join(".git")).expect("git metadata");
    let cargo_home = root.join("cargo-home");
    write(&cargo_home.join("config.toml"), "[build]\n");
    write(&root.join("home/.codex/auth.json"), FIXTURE_AUTH);
    (worktree, cargo_home)
}

/// The boundary for `worktree`, with a proxy stand-in the caller keeps bound.
fn boundary(root: &Path, worktree: &Path, loom: PathBuf) -> (BoundarySpec, UnixListener) {
    let proxy_socket = root.join("proxy/p.sock");
    fs::create_dir_all(root.join("proxy")).expect("the proxy directory");
    let proxy = UnixListener::bind(&proxy_socket).expect("the proxy stand-in");
    let cache = StageCache::resolve(&root.join("repo"), "s1", root, Some(&root.join("cache")))
        .expect("a stage cache");
    let spec = BoundarySpec {
        worktree: worktree.to_path_buf(),
        codex_home: root.join("home/.codex"),
        codex_program: PathBuf::from("/bin/sh"),
        proxy_socket,
        loom_program: loom,
        stage_cache: Some(cache),
        invocation: crate::native_worker::ids::mint_invocation_id().expect("an invocation id"),
    };
    (spec, proxy)
}

#[test]
#[serial]
fn native_macos_cargo_runs_below_a_project_and_a_cargo_home_with_config_files() {
    let name = "native_macos_cargo_runs_below_a_project_and_a_cargo_home_with_config_files";
    let Some(loom) = built_loom(name) else {
        return;
    };
    let cargo = std::env::var_os("CARGO").map(PathBuf::from);
    let cargo = cargo.and_then(|path| path.canonicalize().ok());
    if skip_unless(
        cargo.is_some(),
        name,
        "no CARGO names the toolchain's cargo",
    ) {
        return;
    }
    let dir = tempfile::tempdir().expect("a temp dir");
    let root = dir.path().canonicalize().expect("canonical");
    let (worktree, cargo_home) = project(&root);
    let (spec, _proxy) = boundary(&root, &worktree, loom);
    let mut host: Vec<(OsString, OsString)> = std::env::vars_os()
        .filter(|(key, _)| key != "CARGO_HOME")
        .collect();
    host.push(("CARGO_HOME".into(), cargo_home.into()));
    let args: Vec<OsString> = vec![
        "-c".into(),
        PAYLOAD.into(),
        "sh".into(),
        worktree.clone().into(),
        cargo.expect("checked above").into(),
    ];
    let output = run_released(&spec, &args, &host);
    assert_eq!(verdict(&output, "cargo_exit"), "0", "{output}");
    let manifest = worktree.join("Cargo.toml");
    assert!(
        output.lines().any(|line| Path::new(line) == manifest),
        "cargo did not locate {}:\n{output}",
        manifest.display()
    );
}
