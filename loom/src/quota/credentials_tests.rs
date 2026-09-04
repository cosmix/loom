use super::*;
use tempfile::tempdir;

fn write_credentials(home: &Path, body: &str) {
    let dir = home.join(".claude");
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join(".credentials.json"), body).unwrap();
}

#[test]
fn keychain_argv_is_exact() {
    let (program, args) = keychain_argv();
    assert_eq!(program, "security");
    assert_eq!(args[0], "find-generic-password");
    assert_eq!(args[1], "-s");
    assert_eq!(args[2], "Claude Code-credentials");
    assert_eq!(args[3], "-w");
}

#[test]
fn a_token_present_in_the_credentials_file_is_returned() {
    let home = tempdir().unwrap();
    write_credentials(
        home.path(),
        r#"{"claudeAiOauth":{"accessToken":"sk-test-token"}}"#,
    );
    assert_eq!(access_token(home.path()).unwrap(), "sk-test-token");
}

#[test]
fn a_missing_credentials_file_is_an_error() {
    let home = tempdir().unwrap();
    assert!(access_token(home.path()).is_err());
}

#[test]
fn a_credentials_file_missing_the_access_token_key_is_an_error() {
    let home = tempdir().unwrap();
    write_credentials(home.path(), r#"{"claudeAiOauth":{"other":"value"}}"#);
    assert!(access_token(home.path()).is_err());
}

#[test]
fn a_credentials_file_with_the_wrong_shape_is_an_error() {
    let home = tempdir().unwrap();
    write_credentials(home.path(), r#"{"unrelated":true}"#);
    assert!(access_token(home.path()).is_err());
}

#[test]
fn a_credentials_file_with_an_empty_token_is_an_error() {
    let home = tempdir().unwrap();
    write_credentials(home.path(), r#"{"claudeAiOauth":{"accessToken":""}}"#);
    assert!(access_token(home.path()).is_err());
}
