use super::*;
use tempfile::TempDir;

/// Run the written wrapper through `bash <path>` (never exec of the script, so
/// the ETXTBSY fork/exec race cannot occur) under an explicit, minimal host
/// environment, and return what the `env` session command printed.
#[test]
fn wrapper_exec_environment_keeps_user_and_drops_secrets() {
    let temp_dir = TempDir::new().unwrap();
    let wrapper = create_wrapper_script(
        temp_dir.path(),
        "loom-env-test",
        "env-stage",
        "session-env-1",
        "env",
        None,
        SessionType::Stage,
        100_000,
    )
    .unwrap();

    let output = std::process::Command::new("bash")
        .arg(&wrapper)
        .env_clear()
        .env("HOME", temp_dir.path())
        .env("PATH", "/usr/bin:/bin")
        .env("USER", "alice")
        .env("LOGNAME", "alice")
        .env("GITHUB_TOKEN", "canary")
        .output()
        .unwrap();
    assert!(output.status.success(), "wrapper exited {}", output.status);

    let stdout = String::from_utf8(output.stdout).unwrap();
    assert!(stdout.lines().any(|line| line == "USER=alice"), "{stdout}");
    assert!(
        stdout.lines().any(|line| line == "LOGNAME=alice"),
        "{stdout}"
    );
    assert!(!stdout.contains("GITHUB_TOKEN"), "{stdout}");
    assert!(!stdout.contains("canary"), "{stdout}");
}
