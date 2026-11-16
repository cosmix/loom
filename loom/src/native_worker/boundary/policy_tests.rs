//! Mount policy without a running boundary: the codex package bind, the
//! hidden state root, `.loom` refusal, the Cargo scratch probe and the
//! spawn-failure classification.
use std::ffi::{OsStr, OsString};
use std::path::{Path, PathBuf};
use std::process::Command;

use super::linux::{scratch_probe, StartedThenFailed};
use super::mounts::{self, codex_package};
use super::spawn_started;
use super::tests::canonical_tempdir;

fn args(command: &Command) -> Vec<OsString> {
    command.get_args().map(OsStr::to_os_string).collect()
}

fn worktree_args(w: &Path) -> anyhow::Result<Vec<OsString>> {
    let mut command = Command::new("bwrap");
    mounts::worktree(&mut command, w, false)?;
    Ok(args(&command))
}

fn has_run(args: &[OsString], run: &[&Path]) -> bool {
    args.windows(run.len()).any(|window| {
        window
            .iter()
            .zip(run)
            .all(|(arg, item)| arg == item.as_os_str())
    })
}

#[test]
fn local_bin_codex_binds_only_the_executable() {
    let (_tmp, home) = canonical_tempdir();
    let bin = home.join(".local/bin");
    std::fs::create_dir_all(&bin).unwrap();
    std::fs::create_dir_all(home.join(".local/share/keyrings")).unwrap();
    std::fs::write(home.join(".local/share/keyrings/login"), "secret").unwrap();
    std::fs::create_dir_all(home.join(".local/codex-path")).unwrap();
    std::fs::write(bin.join("codex"), b"\x7fELF").unwrap();
    let package = codex_package(&bin.join("codex")).unwrap();
    assert_eq!(package.root, bin.join("codex"));
    assert_eq!(package.program, bin.join("codex"));
    assert_eq!(package.helper_dir, None);
    let exposed = [home.clone(), home.join(".codex")];
    mounts::refuse_wide_package(&package.root, exposed.iter()).unwrap();
}

#[test]
fn standalone_release_binds_the_release_directory() {
    let (_tmp, release) = canonical_tempdir();
    std::fs::create_dir_all(release.join("bin")).unwrap();
    std::fs::create_dir_all(release.join("codex-path")).unwrap();
    std::fs::write(release.join("bin/codex"), b"\x7fELF").unwrap();
    std::fs::write(release.join("codex-package.json"), "{}").unwrap();
    let package = codex_package(&release.join("bin/codex")).unwrap();
    assert_eq!(package.root, release);
    assert_eq!(package.helper_dir, Some(release.join("codex-path")));
}

#[test]
fn real_state_root_is_hidden_under_an_empty_tmpfs() {
    let (_tmp, w) = canonical_tempdir();
    let work = w.join(".loom/work");
    std::fs::create_dir_all(work.join("native-workers")).unwrap();
    std::fs::write(work.join("admin.token"), "token").unwrap();
    let args = worktree_args(&w).unwrap();
    let loom = w.join(".loom");
    let ro_loom = Path::new("--ro-bind");
    assert!(has_run(&args, &[ro_loom, &loom, &loom]), "{args:?}");
    let tmpfs = [
        Path::new("--tmpfs"),
        &work,
        Path::new("--remount-ro"),
        &work,
    ];
    assert!(has_run(&args, &tmpfs), "{args:?}");
    let bound: Vec<PathBuf> = args
        .windows(2)
        .filter(|pair| pair[0] == "--bind" || pair[0] == "--ro-bind")
        .map(|pair| PathBuf::from(&pair[1]))
        .collect();
    assert!(
        !bound.iter().any(|path| path.starts_with(&work)),
        "{args:?}"
    );
}

#[test]
fn state_root_link_into_the_worktree_is_refused() {
    let (_tmp, w) = canonical_tempdir();
    std::fs::create_dir_all(w.join(".loom")).unwrap();
    std::fs::create_dir_all(w.join("state")).unwrap();
    std::os::unix::fs::symlink(w.join("state"), w.join(".loom/work")).unwrap();
    let error = worktree_args(&w).unwrap_err();
    assert!(
        format!("{error:#}").contains("links into the worktree"),
        "{error:#}"
    );
}

#[test]
fn state_root_link_outside_is_kept_and_not_bound() {
    let (_main_tmp, main) = canonical_tempdir();
    let (_w_tmp, w) = canonical_tempdir();
    std::fs::create_dir_all(main.join(".loom/work")).unwrap();
    std::fs::create_dir_all(w.join(".loom")).unwrap();
    std::os::unix::fs::symlink(main.join(".loom/work"), w.join(".loom/work")).unwrap();
    let args = worktree_args(&w).unwrap();
    assert!(!args.iter().any(|arg| arg == "--tmpfs"), "{args:?}");
    assert!(
        !args.iter().any(|arg| *arg == main.join(".loom/work")),
        "{args:?}"
    );
}

#[test]
fn symlinked_loom_is_refused() {
    let (_outside_tmp, outside) = canonical_tempdir();
    let (_w_tmp, w) = canonical_tempdir();
    std::os::unix::fs::symlink(&outside, w.join(".loom")).unwrap();
    let error = worktree_args(&w).unwrap_err();
    assert!(format!("{error:#}").contains("symlink"), "{error:#}");
}

#[test]
fn scratch_probe_has_no_home_but_names_the_cargo_home() {
    let host: Vec<(OsString, OsString)> = vec![("HOME".into(), "/home/u".into())];
    let env = vec![
        (OsString::from("HOME"), OsString::from("/home/u")),
        (OsString::from("PATH"), OsString::from("/usr/bin")),
    ];
    let probe = scratch_probe(OsStr::new("/bin/true"), &env, &host);
    let envs: Vec<_> = probe.get_envs().collect();
    assert!(!envs.iter().any(|(key, _)| *key == "HOME"), "{envs:?}");
    let cargo = envs.iter().find(|(key, _)| *key == "CARGO_HOME");
    assert_eq!(
        cargo,
        Some(&(OsStr::new("CARGO_HOME"), Some(OsStr::new("/home/u/.cargo"))))
    );
}

#[test]
fn spawn_started_only_for_errors_after_bubblewrap_ran() {
    let started = anyhow::Error::from(StartedThenFailed("no info".into())).context("spawn");
    assert!(spawn_started(&started));
    assert!(!spawn_started(&anyhow::anyhow!("a symlinked .loom")));
}
