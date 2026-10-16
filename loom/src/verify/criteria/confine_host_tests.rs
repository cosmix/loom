use crate::models::stage::CommandConfinement;
use crate::verify::criteria::{run_spec_with_timeout, CommandSpec};
use std::time::Duration;

#[test]
fn host_canary_child() {
    let cwd = std::env::current_dir().unwrap();
    let Ok(port) = std::fs::read_to_string(cwd.join("canary-port")) else {
        return;
    };
    println!("HOST_CANARY_STARTED");
    std::fs::write(cwd.join("allowed"), "inside").unwrap();
    let sibling = cwd.parent().unwrap().join("secret");
    assert!(
        std::fs::read(&sibling).is_err(),
        "sibling read escaped confinement"
    );
    assert!(
        std::fs::write(&sibling, "escaped").is_err(),
        "sibling write escaped confinement"
    );
    let address = format!("127.0.0.1:{}", port.trim()).parse().unwrap();
    assert!(
        std::net::TcpStream::connect_timeout(&address, Duration::from_secs(1)).is_err(),
        "network escaped confinement"
    );
    println!("HOST_CANARY_DENIED");
}

#[test]
fn actual_host_launcher_denies_sibling_read_write_and_network() {
    if crate::process::sandbox_probe::skip_unless(
        super::launcher().is_some(),
        "actual_host_launcher_denies_sibling_read_write_and_network",
        "OS sandbox launcher absent",
    ) {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().join("worktree");
    std::fs::create_dir(&cwd).unwrap();
    let secret = root.path().join("secret");
    std::fs::write(&secret, "untouched").unwrap();
    assert_eq!(std::fs::read_to_string(&secret).unwrap(), "untouched");
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    let address = listener.local_addr().unwrap();
    let _positive_control = std::net::TcpStream::connect(address).unwrap();
    std::fs::write(cwd.join("canary-port"), address.port().to_string()).unwrap();
    let result = run_canary(&cwd);
    assert!(
        result.success,
        "stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(result.stdout.contains("HOST_CANARY_STARTED"));
    assert!(result.stdout.contains("HOST_CANARY_DENIED"));
    assert_eq!(std::fs::read_to_string(secret).unwrap(), "untouched");
    assert_eq!(
        std::fs::read_to_string(cwd.join("allowed")).unwrap(),
        "inside"
    );
}

fn run_canary(cwd: &std::path::Path) -> crate::verify::criteria::CriterionResult {
    // Copy only this test executable into the allowed tree: no sibling grant.
    let executable = cwd.join("probe");
    std::fs::copy(std::env::current_exe().unwrap(), &executable).unwrap();
    let spec = CommandSpec::program(
        executable.to_str().unwrap(),
        [
            "--exact",
            "verify::criteria::confine::host::tests::host_canary_child",
            "--nocapture",
        ],
    );
    run_spec_with_timeout(
        &spec,
        Some(cwd),
        Duration::from_secs(15),
        CommandConfinement::Confined,
    )
    .unwrap()
}

#[test]
fn actual_host_launcher_builds_cargo_offline_with_private_cache_state() {
    if crate::process::sandbox_probe::skip_unless(
        super::launcher().is_some(),
        "actual_host_launcher_builds_cargo_offline_with_private_cache_state",
        "OS sandbox launcher absent",
    ) {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().join("worktree");
    std::fs::create_dir(&cwd).unwrap();
    std::fs::create_dir(cwd.join("src")).unwrap();
    std::fs::write(
        cwd.join("Cargo.toml"),
        "[package]\nname = \"confined-fixture\"\nversion = \"0.1.0\"\nedition = \"2021\"\n",
    )
    .unwrap();
    std::fs::write(cwd.join("src/main.rs"), "fn main() {}\n").unwrap();
    let result = run_spec_with_timeout(
        &CommandSpec::program("cargo", ["build", "--offline"]),
        Some(&cwd),
        Duration::from_secs(60),
        CommandConfinement::Confined,
    )
    .unwrap();
    assert!(
        result.success,
        "stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(cwd.join("target/debug/confined-fixture").is_file());
    assert!(!std::fs::read_dir(&cwd).unwrap().any(|entry| entry
        .unwrap()
        .file_name()
        .to_string_lossy()
        .starts_with(".loom-cargo-")));
}

#[cfg(unix)]
#[test]
fn private_cargo_home_uses_read_only_sources_and_hides_credentials() {
    use crate::verify::criteria::confine::{host, prepare_confined};
    if crate::process::sandbox_probe::skip_unless(
        super::launcher().is_some(),
        "private_cargo_home_uses_read_only_sources_and_hides_credentials",
        "OS sandbox launcher absent",
    ) {
        return;
    }
    let root = tempfile::tempdir().unwrap();
    let cwd = root.path().join("worktree");
    let cargo = root.path().join("cargo");
    std::fs::create_dir(&cwd).unwrap();
    std::fs::create_dir_all(cargo.join("registry")).unwrap();
    std::fs::write(cargo.join("registry/sentinel"), "untouched").unwrap();
    std::fs::write(cargo.join("credentials.toml"), "secret").unwrap();
    let line = format!(
        "cat \"$CARGO_HOME/registry/sentinel\" && ! echo changed > \"$CARGO_HOME/registry/sentinel\" && ! cat '{}' && touch \"$CARGO_HOME/.package-cache\" \"$CARGO_HOME/.global-cache\"",
        cargo.join("credentials.toml").display());
    let spec = CommandSpec::shell(line);
    let mut prepared = prepare_confined(&spec, Some(&cwd), CommandConfinement::Inherit).unwrap();
    crate::process::apply_stage_environment(&mut prepared.command);
    prepared.command.env("CARGO_HOME", &cargo);
    let (wrapped, scratch) = host::wrap(&prepared.command).unwrap();
    prepared.wrapped = Some(wrapped);
    prepared.scratch = Some(scratch);
    let result = crate::verify::criteria::executor::run_prepared_with_timeout(
        prepared,
        Duration::from_secs(15),
    )
    .unwrap();
    assert!(
        result.success,
        "stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert_eq!(
        std::fs::read_to_string(cargo.join("registry/sentinel")).unwrap(),
        "untouched"
    );
    assert!(!cargo.join(".package-cache").exists());
    assert!(!cargo.join(".global-cache").exists());
}
