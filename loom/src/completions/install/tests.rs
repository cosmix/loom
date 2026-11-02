use super::*;
use serial_test::serial;
use tempfile::TempDir;

#[test]
fn bash_rc_uses_shell_quoted_completion_path() {
    let home = TempDir::new().unwrap();
    let completion_path = home.path().join("dir with spaces/loom;touch-pwned");

    ensure_bashrc_completion(home.path(), &completion_path).unwrap();

    let bashrc = std::fs::read_to_string(home.path().join(".bashrc")).unwrap();
    let quoted = quote_bash_path(&completion_path).unwrap();
    assert!(bashrc.contains(&format!("[ -f {quoted} ] && source {quoted}")));
}

#[test]
fn writable_probe_preserves_existing_files_and_symlink_targets() {
    use std::os::unix::fs::symlink;
    let dir = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let probe = dir.path().join(".loom_write_probe");
    std::fs::write(&probe, "keep").unwrap();
    assert!(is_writable(dir.path()));
    assert_eq!(std::fs::read_to_string(&probe).unwrap(), "keep");
    std::fs::remove_file(&probe).unwrap();
    let target = outside.path().join("target");
    std::fs::write(&target, "keep target").unwrap();
    symlink(&target, &probe).unwrap();
    assert!(is_writable(dir.path()));
    assert_eq!(std::fs::read_to_string(target).unwrap(), "keep target");
    assert!(std::fs::symlink_metadata(probe).unwrap().is_symlink());
}

#[test]
#[serial]
fn refresh_skips_default_and_xdg_symlink_escapes() {
    use std::os::unix::fs::symlink;
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let target = outside.path().join("completion");
    std::fs::write(&target, "keep").unwrap();
    let default = home.path().join(".zfunc/_loom");
    std::fs::create_dir_all(default.parent().unwrap()).unwrap();
    symlink(&target, &default).unwrap();
    let xdg = home.path().join("xdg");
    symlink(outside.path(), &xdg).unwrap();
    let xdg_target = outside.path().join("bash-completion/completions/loom");
    std::fs::create_dir_all(xdg_target.parent().unwrap()).unwrap();
    std::fs::write(&xdg_target, "keep xdg").unwrap();
    let _data = EnvVarGuard::set("XDG_DATA_HOME", &xdg);
    let _config = EnvVarGuard::unset("XDG_CONFIG_HOME");
    assert_eq!(refresh_existing_in(home.path()).unwrap(), 0);
    assert_eq!(std::fs::read_to_string(target).unwrap(), "keep");
    assert_eq!(std::fs::read_to_string(xdg_target).unwrap(), "keep xdg");
    std::fs::remove_file(&default).unwrap();
    std::fs::remove_dir(default.parent().unwrap()).unwrap();
    let external_zsh = outside.path().join("_loom");
    std::fs::write(&external_zsh, "keep parent").unwrap();
    symlink(outside.path(), default.parent().unwrap()).unwrap();
    assert_eq!(refresh_existing_in(home.path()).unwrap(), 0);
    assert_eq!(
        std::fs::read_to_string(external_zsh).unwrap(),
        "keep parent"
    );
}

/// Pins an environment variable for a test's duration and restores whatever
/// value (or absence) it had beforehand on drop. Mirrors the same-purpose
/// guard in `orchestrator/terminal/native/tests_wrapper_env.rs`, which is
/// private to that module and not reachable from here.
struct EnvVarGuard {
    key: &'static str,
    original: Option<std::ffi::OsString>,
}

impl EnvVarGuard {
    fn set(key: &'static str, value: impl AsRef<std::ffi::OsStr>) -> Self {
        let original = std::env::var_os(key);
        std::env::set_var(key, value);
        Self { key, original }
    }

    fn unset(key: &'static str) -> Self {
        let original = std::env::var_os(key);
        std::env::remove_var(key);
        Self { key, original }
    }
}

impl Drop for EnvVarGuard {
    fn drop(&mut self) {
        match &self.original {
            Some(value) => std::env::set_var(self.key, value),
            None => std::env::remove_var(self.key),
        }
    }
}

// `refresh_existing_in_skips_missing_files` and
// `refresh_existing_in_rewrites_existing_file` read `XDG_DATA_HOME` /
// `XDG_CONFIG_HOME` (via `refresh_candidates`) just like the three tests
// below that set those variables, so all five run `#[serial]` to avoid
// cross-test interference.

#[test]
#[serial]
fn refresh_existing_in_skips_missing_files() {
    let home = TempDir::new().unwrap();

    assert_eq!(refresh_existing_in(home.path()).unwrap(), 0);
    assert!(!home.path().join(".zfunc/_loom").exists());
}

#[test]
#[serial]
fn refresh_existing_in_rewrites_existing_file() {
    let home = TempDir::new().unwrap();
    let completion = home.path().join(".zfunc/_loom");
    std::fs::create_dir_all(completion.parent().unwrap()).unwrap();
    std::fs::write(&completion, "stale completion").unwrap();

    assert_eq!(refresh_existing_in(home.path()).unwrap(), 1);
    assert_eq!(
        std::fs::read_to_string(completion).unwrap(),
        super::super::scripts::ZSH_COMPLETION
    );
}

// The following three tests read/write XDG_DATA_HOME, a process-global
// environment variable, so each is `#[serial]` to avoid cross-test
// interference; each also sets and restores the variable itself rather than
// relying on a shared fixture.

#[test]
#[serial]
fn refresh_existing_in_rewrites_xdg_bash_completion_under_home() {
    let home = TempDir::new().unwrap();
    let xdg_data_home = home.path().join("xdg-data");
    let completion = xdg_data_home.join("bash-completion/completions/loom");
    std::fs::create_dir_all(completion.parent().unwrap()).unwrap();
    std::fs::write(&completion, "stale completion").unwrap();

    let _guard = EnvVarGuard::set("XDG_DATA_HOME", &xdg_data_home);
    let result = refresh_existing_in(home.path());

    assert_eq!(result.unwrap(), 1);
    assert_eq!(
        std::fs::read_to_string(completion).unwrap(),
        super::super::scripts::BASH_COMPLETION
    );
}

#[test]
#[serial]
fn refresh_existing_in_ignores_xdg_outside_home() {
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let completion = outside.path().join("bash-completion/completions/loom");
    std::fs::create_dir_all(completion.parent().unwrap()).unwrap();
    std::fs::write(&completion, "stale completion").unwrap();

    let _guard = EnvVarGuard::set("XDG_DATA_HOME", outside.path());
    let result = refresh_existing_in(home.path());

    assert_eq!(result.unwrap(), 0);
    assert_eq!(
        std::fs::read_to_string(&completion).unwrap(),
        "stale completion"
    );
}

/// `home/../<outside-basename>` lexically "starts with" `home`, but resolves
/// to a sibling directory once canonicalised. `TempDir::new()` creates both
/// directories directly under the same OS temp root, so `outside`'s basename
/// really is a sibling of `home`'s.
#[test]
#[serial]
fn refresh_existing_in_ignores_xdg_that_escapes_home_via_dot_dot() {
    let home = TempDir::new().unwrap();
    let outside = TempDir::new().unwrap();
    let completion = outside.path().join("bash-completion/completions/loom");
    std::fs::create_dir_all(completion.parent().unwrap()).unwrap();
    std::fs::write(&completion, "stale completion").unwrap();

    let escaping = home
        .path()
        .join("..")
        .join(outside.path().file_name().unwrap());
    let _guard = EnvVarGuard::set("XDG_DATA_HOME", escaping);
    let result = refresh_existing_in(home.path());

    assert_eq!(result.unwrap(), 0);
    assert_eq!(
        std::fs::read_to_string(&completion).unwrap(),
        "stale completion"
    );
}

#[test]
#[serial]
fn refresh_existing_in_default_bash_path_still_refreshed() {
    let home = TempDir::new().unwrap();
    let completion = home
        .path()
        .join(".local/share/bash-completion/completions/loom");
    std::fs::create_dir_all(completion.parent().unwrap()).unwrap();
    std::fs::write(&completion, "stale completion").unwrap();

    let _guard = EnvVarGuard::unset("XDG_DATA_HOME");
    let result = refresh_existing_in(home.path());

    assert_eq!(result.unwrap(), 1);
    assert_eq!(
        std::fs::read_to_string(completion).unwrap(),
        super::super::scripts::BASH_COMPLETION
    );
}
