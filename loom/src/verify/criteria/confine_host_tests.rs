use crate::models::stage::CommandConfinement;
use crate::verify::criteria::{run_spec_with_timeout, CommandSpec};
use std::time::Duration;

// Use installed bash builtins so the canary does not depend on startup of a
// newly linked/copied Mach-O test executable. Both modes exercise the same I/O.
const HOST_CANARY: &str = r#"
mode=$1
port=$2
printf 'HOST_CANARY_STARTED\n'
printf inside > allowed || exit 1
read_secret() { IFS= read -r -n 1 value < ../secret && [[ $value == u ]]; }
write_secret() { printf escaped > ../secret; }
connect_network() { : > /dev/tcp/127.0.0.1/$port; }
check() {
    if "$@"; then
        [[ $mode == control ]] || exit 2
    else
        [[ $mode == denied ]] || exit 3
    fi
    printf '%s_CHECKED\n' "$1"
}
check read_secret
check write_secret
check connect_network
printf 'HOST_CANARY_%s\n' "$mode"
"#;

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
    let control = run_canary(&cwd, address.port(), CommandConfinement::Inherit);
    assert!(control.success, "positive control: {control:?}");
    assert!(control.stdout.contains("HOST_CANARY_control"));
    assert_eq!(std::fs::read_to_string(&secret).unwrap(), "escaped");
    std::fs::write(&secret, "untouched").unwrap();
    let result = run_canary(&cwd, address.port(), CommandConfinement::Confined);
    assert!(
        result.success,
        "stdout={} stderr={}",
        result.stdout, result.stderr
    );
    assert!(result.stdout.contains("HOST_CANARY_STARTED"));
    assert!(result.stdout.contains("HOST_CANARY_denied"));
    assert_eq!(std::fs::read_to_string(secret).unwrap(), "untouched");
    assert_eq!(
        std::fs::read_to_string(cwd.join("allowed")).unwrap(),
        "inside"
    );
}

fn run_canary(
    cwd: &std::path::Path,
    port: u16,
    confinement: CommandConfinement,
) -> crate::verify::criteria::CriterionResult {
    let mode = match confinement {
        CommandConfinement::Inherit => "control",
        CommandConfinement::Confined => "denied",
    };
    let port = port.to_string();
    let spec = CommandSpec::program(
        "/bin/bash",
        [
            "--noprofile",
            "--norc",
            "-c",
            HOST_CANARY,
            "canary",
            mode,
            &port,
        ],
    );
    run_spec_with_timeout(&spec, Some(cwd), Duration::from_secs(15), confinement).unwrap()
}

#[test]
#[serial_test::serial]
fn actual_host_launcher_builds_cargo_offline_with_private_cache_state() {
    let _toolchain = ToolchainReadRoot::from_runner();
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
    let (wrapped, scratch) = host::wrap(&prepared.command, host::NetworkAccess::Denied).unwrap();
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

/// The isolated harness uses an empty Rustup home. This fixture grants the
/// installed compiler tree read access through the normal confinement input.
struct ToolchainReadRoot(Option<Option<std::ffi::OsString>>);

impl ToolchainReadRoot {
    fn from_runner() -> Self {
        let saved = std::env::var_os("LOOM_TEST_TOOLCHAIN").map(|toolchain| {
            let previous = std::env::var_os("RUSTUP_HOME");
            std::env::set_var("RUSTUP_HOME", toolchain);
            previous
        });
        Self(saved)
    }
}

impl Drop for ToolchainReadRoot {
    fn drop(&mut self) {
        match &self.0 {
            Some(Some(previous)) => std::env::set_var("RUSTUP_HOME", previous),
            Some(None) => std::env::remove_var("RUSTUP_HOME"),
            None => {}
        }
    }
}
