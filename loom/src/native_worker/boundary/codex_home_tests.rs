//! The private Codex home: only `sessions/`, the auth files and `AGENTS.md`
//! reach the host's `~/.codex`, and `W/.claude` stays read-only.
use std::ffi::OsString;
use std::path::{Path, PathBuf};
use std::time::Duration;

use tempfile::TempDir;

use super::linux::Plan;
use super::spawn_payload_with_codex_home;
use super::tests::{argv, bind_sources, canonical_tempdir, finish, host, no_boundary, no_bwrap};
use super::tests::{position, sh};
use super::BoundarySpec;

/// A HOME with a full `.codex`, a worktree with `.claude`, a native-looking
/// codex package, a launcher file and a proxy socket directory.
pub(super) struct RealFixture {
    _dirs: Vec<TempDir>,
    pub(super) home: PathBuf,
    pub(super) w: PathBuf,
    spec: BoundarySpec,
}

impl RealFixture {
    pub(super) fn new() -> Self {
        let (home_tmp, home) = canonical_tempdir();
        let (w_tmp, w) = canonical_tempdir();
        let (pkg_tmp, pkg) = canonical_tempdir();
        let codex = home.join(".codex");
        std::fs::create_dir_all(codex.join("sessions")).unwrap();
        std::fs::create_dir_all(codex.join("rules")).unwrap();
        for (name, text) in [
            ("auth.json", "{}\n"),
            ("AGENTS.md", "# agents\n"),
            ("config.toml", "model = \"x\"\n"),
            ("hooks.json", "{}\n"),
        ] {
            std::fs::write(codex.join(name), text).unwrap();
        }
        std::fs::create_dir(w.join(".claude")).unwrap();
        std::fs::create_dir_all(pkg.join("bin")).unwrap();
        std::fs::write(pkg.join("bin/codex"), b"\x7fELF").unwrap();
        std::fs::write(pkg.join("loom"), b"\x7fELF").unwrap();
        let spec = BoundarySpec {
            worktree: w.clone(),
            codex_home: codex,
            codex_program: pkg.join("bin/codex"),
            proxy_socket: pkg.join("proxy/proxy.sock"),
            loom_program: pkg.join("loom"),
        };
        Self {
            _dirs: vec![home_tmp, w_tmp, pkg_tmp],
            home,
            w,
            spec,
        }
    }

    pub(super) fn argv(&self) -> Vec<OsString> {
        let host = host(&self.home);
        argv(&Plan::real(&self.spec, &host).unwrap(), &host)
    }

    /// Host directory of the proxy socket, bound into the boundary.
    pub(super) fn proxy_dir(&self) -> &Path {
        self.spec.proxy_socket.parent().unwrap()
    }

    /// The fixture's own host directories: HOME, the worktree and the package.
    pub(super) fn host_dirs(&self) -> [&Path; 3] {
        [&self.home, &self.w, self.proxy_dir().parent().unwrap()]
    }
}

#[test]
fn argv_codex_home_is_private() {
    if no_bwrap("argv_codex_home_is_private") {
        return;
    }
    let fixture = RealFixture::new();
    let args = fixture.argv();
    let codex = fixture.home.join(".codex");
    let sources = bind_sources(&args);
    assert!(
        sources.iter().all(|(_, source)| *source != codex),
        "{sources:?}"
    );
    for (_, source) in &sources {
        let name = source.file_name().unwrap_or_default();
        assert!(name != "config.toml" && name != "hooks.json" && name != "rules");
    }
    let tmpfs = position(&args, &["--tmpfs".as_ref(), codex.as_ref()]).expect("private tmpfs");
    let size = super::linux::CODEX_HOME_TMPFS_BYTES.to_string();
    let sized = [
        "--size".as_ref(),
        size.as_ref(),
        "--tmpfs".as_ref(),
        codex.as_ref(),
    ];
    assert_eq!(position(&args, &sized), Some(tmpfs - 2), "{args:?}");
    assert_eq!(size, (1u64 << 30).to_string());
    for (flag, name) in [("--bind", "sessions"), ("--bind", "auth.json")] {
        let path = codex.join(name);
        let bind = position(&args, &[flag.as_ref(), path.as_ref(), path.as_ref()]);
        assert!(tmpfs < bind.unwrap_or_else(|| panic!("{name}: {args:?}")));
    }
    let agents = codex.join("AGENTS.md");
    let agents = position(
        &args,
        &["--ro-bind".as_ref(), agents.as_ref(), agents.as_ref()],
    );
    assert!(tmpfs < agents.expect("AGENTS.md read-only"));
    let claude = fixture.w.join(".claude");
    let ro_claude = ["--ro-bind".as_ref(), claude.as_ref(), claude.as_ref()];
    assert!(position(&args, &ro_claude).is_some(), "{args:?}");
}

#[test]
#[serial_test::serial]
fn payload_codex_home_policy() {
    if no_boundary("payload_codex_home_policy") {
        return;
    }
    let home = crate::fs::permissions::scratch_home::ScratchHome::set();
    let codex = home.path().canonicalize().unwrap().join(".codex");
    std::fs::create_dir_all(codex.join("sessions")).unwrap();
    std::fs::write(codex.join("hooks.json"), "host\n").unwrap();
    let (_tmp, w) = canonical_tempdir();
    std::fs::create_dir(w.join(".claude")).unwrap();
    let script = format!(
        "echo planted > \"$HOME/.codex/hooks.json\" || true; \
         echo canary > \"$HOME/.codex/sessions/canary\" || true; \
         echo planted > '{}/.claude/x' 2>/dev/null || true",
        w.display()
    );
    let boundary =
        spawn_payload_with_codex_home(&w, &codex, Path::new("/bin/sh"), &sh(&script)).unwrap();
    finish(boundary, Duration::from_secs(10));
    assert_eq!(
        std::fs::read_to_string(codex.join("hooks.json")).unwrap(),
        "host\n"
    );
    assert!(codex.join("sessions/canary").is_file());
    assert!(!w.join(".claude/x").exists());
}

#[test]
fn symlinked_codex_home_source_is_refused() {
    if no_bwrap("symlinked_codex_home_source_is_refused") {
        return;
    }
    for name in ["sessions", "auth.json", "AGENTS.md", ".claude"] {
        let (_home_tmp, home) = canonical_tempdir();
        let (_w_tmp, w) = canonical_tempdir();
        let (_target_tmp, target) = canonical_tempdir();
        let codex = home.join(".codex");
        std::fs::create_dir_all(&codex).unwrap();
        if name != "sessions" {
            std::fs::create_dir(codex.join("sessions")).unwrap();
        }
        if name != ".claude" {
            std::fs::create_dir(w.join(".claude")).unwrap();
        }
        let link = if name == ".claude" {
            w.join(name)
        } else {
            codex.join(name)
        };
        std::os::unix::fs::symlink(&target, &link).unwrap();
        let error = spawn_payload_with_codex_home(&w, &codex, Path::new("/bin/true"), &[])
            .err()
            .unwrap_or_else(|| panic!("a symlinked {name} was accepted"));
        assert!(
            format!("{error:#}").contains("symlink"),
            "{name}: {error:#}"
        );
    }
}

#[test]
fn system_codex_config_is_covered_by_an_empty_read_only_tmpfs() {
    use super::mounts::hide_system_config;
    let (_tmp, root) = canonical_tempdir();
    let dir = root.join("codex");
    let args = |dir: &Path| {
        let mut command = std::process::Command::new("bwrap");
        hide_system_config(&mut command, dir)
            .map(|()| command.get_args().map(OsString::from).collect::<Vec<_>>())
    };
    assert!(args(&dir).unwrap().is_empty(), "absent: nothing to cover");
    std::fs::create_dir(&dir).unwrap();
    let expected: Vec<OsString> = vec![
        "--tmpfs".into(),
        dir.clone().into(),
        "--remount-ro".into(),
        dir.clone().into(),
    ];
    assert_eq!(args(&dir).unwrap(), expected);
    let link = root.join("link");
    std::os::unix::fs::symlink(&dir, &link).unwrap();
    assert!(format!("{:#}", args(&link).unwrap_err()).contains("symlink"));
    assert_eq!(super::mounts::SYSTEM_CODEX_CONFIG, "/etc/codex");
}

#[test]
fn argv_project_codex_dir_is_read_only() {
    if no_bwrap("argv_project_codex_dir_is_read_only") {
        return;
    }
    let fixture = RealFixture::new();
    let codex = fixture.w.join(".codex");
    let ro = ["--ro-bind".as_ref(), codex.as_ref(), codex.as_ref()];
    assert!(position(&fixture.argv(), &ro).is_none());
    std::fs::create_dir(&codex).unwrap();
    let args = fixture.argv();
    assert!(position(&args, &ro).is_some(), "{args:?}");
    std::fs::remove_dir(&codex).unwrap();
    std::fs::write(&codex, "x").unwrap();
    let host = host(&fixture.home);
    let plan = Plan::real(&fixture.spec, &host).unwrap();
    let error = super::linux::command(&plan, &host, 3)
        .err()
        .expect("a file .codex");
    assert!(
        format!("{error:#}").contains("is not a directory"),
        "{error:#}"
    );
}
