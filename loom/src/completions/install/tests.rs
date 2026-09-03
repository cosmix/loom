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
fn refresh_existing_in_skips_missing_files() {
    let home = TempDir::new().unwrap();

    assert_eq!(refresh_existing_in(home.path()).unwrap(), 0);
    assert!(!home.path().join(".zfunc/_loom").exists());
}

#[test]
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

    std::env::set_var("XDG_DATA_HOME", &xdg_data_home);
    let result = refresh_existing_in(home.path());
    std::env::remove_var("XDG_DATA_HOME");

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

    std::env::set_var("XDG_DATA_HOME", outside.path());
    let result = refresh_existing_in(home.path());
    std::env::remove_var("XDG_DATA_HOME");

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

    std::env::remove_var("XDG_DATA_HOME");
    let result = refresh_existing_in(home.path());

    assert_eq!(result.unwrap(), 1);
    assert_eq!(
        std::fs::read_to_string(completion).unwrap(),
        super::super::scripts::BASH_COMPLETION
    );
}
