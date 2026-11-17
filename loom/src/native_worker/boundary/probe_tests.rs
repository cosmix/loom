use std::ffi::OsString;
use std::os::unix::fs::PermissionsExt;
use std::os::unix::process::ExitStatusExt;
use std::path::{Path, PathBuf};
use std::process::ExitStatus;

use super::host::{codex_bwrap, RealHost};
use super::{qualify_from, HostProbes, Nesting, NESTING_REQUIRED, SIZE_REQUIRED};

const SIZED_HELP: &str =
    "    --size BYTES                 Set size of next argument (only for --tmpfs)\n";
const UNSIZED_HELP: &str = "    --perms OCTAL                Set permissions of next argument\n    --tmpfs DEST                 Mount new tmpfs on DEST\n";

/// A host whose every probe answers as configured.
struct FakeHost {
    help: &'static str,
    exit: i32,
    namespace: u64,
    nested: bool,
    bwrap: Option<PathBuf>,
}

impl FakeHost {
    fn qualified() -> Self {
        Self {
            help: SIZED_HELP,
            exit: 0,
            namespace: 2,
            nested: true,
            bwrap: Some("/usr/bin/bwrap".into()),
        }
    }
}

impl HostProbes for FakeHost {
    fn bwrap_help(&self) -> Result<String, String> {
        Ok(self.help.to_string())
    }

    fn boundary(&self) -> Result<(ExitStatus, u64), String> {
        Ok((ExitStatus::from_raw(self.exit << 8), self.namespace))
    }

    fn own_pid_namespace(&self) -> Result<PathBuf, String> {
        Ok("pid:[1]".into())
    }

    fn nesting(&self) -> Result<Nesting, String> {
        Ok(Nesting {
            ran: self.nested,
            bwrap: self.bwrap.clone(),
            stderr: "bwrap: setting up uid map: Permission denied\n".into(),
        })
    }
}

#[test]
fn qualified_host_passes_every_gate() {
    assert_eq!(qualify_from(&FakeHost::qualified()), Ok(()));
}

#[test]
fn bubblewrap_without_size_is_refused() {
    let host = FakeHost {
        help: UNSIZED_HELP,
        ..FakeHost::qualified()
    };
    assert_eq!(qualify_from(&host), Err(SIZE_REQUIRED.to_string()));
    assert!(SIZE_REQUIRED.contains("bubblewrap 0.10 or later required"));
}

#[test]
fn host_without_nesting_is_refused_naming_its_bubblewrap() {
    let host = FakeHost {
        nested: false,
        ..FakeHost::qualified()
    };
    let error = qualify_from(&host).unwrap_err();
    assert!(error.starts_with(NESTING_REQUIRED), "{error}");
    assert!(error.contains("runs bubblewrap /usr/bin/bwrap"), "{error}");
    assert!(error.ends_with("codex sandbox stderr: bwrap: setting up uid map: Permission denied"));
    let host = FakeHost {
        nested: false,
        bwrap: None,
        ..FakeHost::qualified()
    };
    let error = qualify_from(&host).unwrap_err();
    assert!(error.contains("no bubblewrap is visible"), "{error}");
    assert!(error.contains("codex-resources/bwrap"), "{error}");
}

#[test]
fn failing_or_shared_boundary_is_refused() {
    let failed = FakeHost {
        exit: 1,
        ..FakeHost::qualified()
    };
    assert!(qualify_from(&failed)
        .unwrap_err()
        .contains("failed to run true"));
    let shared = FakeHost {
        namespace: 1,
        ..FakeHost::qualified()
    };
    assert!(qualify_from(&shared)
        .unwrap_err()
        .contains("shares this process's"));
}

fn file(path: &Path, mode: u32) {
    std::fs::create_dir_all(path.parent().unwrap()).unwrap();
    std::fs::write(path, b"\x7fELF").unwrap();
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// PATH first, but only an executable inside the boundary's visible roots;
/// then the release's bundled bubblewrap; a lone executable has none.
#[test]
fn codex_bwrap_is_the_visible_path_one_then_the_bundled_one() {
    let root = tempfile::tempdir().unwrap();
    let (hidden, shown, release) = (
        root.path().join("hidden"),
        root.path().join("shown"),
        root.path().join("release"),
    );
    file(&hidden.join("bwrap"), 0o755);
    file(&shown.join("bwrap"), 0o644);
    file(&release.join("codex-resources/bwrap"), 0o755);
    let path = std::env::join_paths([&hidden, &shown]).unwrap();
    let visible = [shown.clone(), release.clone()];
    let bundled = release.join("codex-resources/bwrap");
    assert_eq!(codex_bwrap(&path, &visible, &release), Some(bundled));
    std::fs::set_permissions(shown.join("bwrap"), std::fs::Permissions::from_mode(0o755)).unwrap();
    assert_eq!(
        codex_bwrap(&path, &visible, &release),
        Some(shown.join("bwrap"))
    );
    let lone = release.join("codex-resources/bwrap");
    assert_eq!(codex_bwrap(&OsString::new(), &visible, &lone), None);
}

/// On a host that runs boundaries and has codex, Codex's own sandbox runs
/// inside one exactly as a turn would start it.
#[test]
#[serial_test::serial]
fn real_codex_sandbox_runs_inside_a_payload_boundary() {
    if super::super::tests::no_boundary("real_codex_sandbox_runs_inside_a_payload_boundary") {
        return;
    }
    if crate::codex::find_codex_path().is_err() {
        eprintln!("skipping real_codex_sandbox_runs_inside_a_payload_boundary: no codex on PATH");
        return;
    }
    let nesting = RealHost.nesting().unwrap();
    assert!(nesting.ran, "{nesting:?}");
    assert!(nesting.bwrap.is_some(), "{nesting:?}");
}
