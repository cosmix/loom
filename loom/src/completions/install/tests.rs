use super::*;
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
