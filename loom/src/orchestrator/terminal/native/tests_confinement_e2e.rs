//! The confinement e2e (`doc/plans/PLAN-loom-state-confinement.md`, section
//! 16): capsules built by the real launch path against a fake home and a temp
//! repository, each translated into an `srt` settings file and probed from
//! the session's working directory (`tests_confinement_srt.rs`).
//!
//! Linux only, and skipped unless `bwrap`, `socat` and `srt` are on PATH and a
//! trivial `srt -c true` runs: a nested sandbox can refuse the namespaces or
//! the sockets srt needs.
//!
//! The tests run `#[serial]`: each srt run starts its own proxies and bwrap,
//! and the latency test measures time, which concurrent srt runs distort.

use super::host::LaunchHost;
use super::*;
use crate::fs::permissions::install_loom_hooks_to;
use crate::fs::work_dir::write_remote_control_config;
use crate::models::stage::{Implementer, Implementers, StageType};
use crate::orchestrator::terminal::native::capsule::CapsuleSupport;
use crate::orchestrator::terminal::native::session_settings_path;
use crate::remote_control::{RemoteControlConfig, RemoteControlMode};
use crate::sandbox::control_surfaces::{session_writable_roots, WritableRootInputs};
use crate::sandbox::preflight::HostFacts;
use serde_json::Value;
use serial_test::serial;
use srt::{diagnostics, skip, Confined};
use std::os::unix::fs::PermissionsExt;
use std::process::{Command, Output, Stdio};
use tempfile::TempDir;

#[path = "tests_confinement_srt.rs"]
mod srt;

const STAGE_ID: &str = "stage-1";
/// Twenty timed runs of `/bin/true` inside one shell, one microsecond count
/// per line, so srt's own startup stays out of the samples.
const LATENCY_SCRIPT: &str = r#"for _ in 1 2 3 4 5 6 7 8 9 10 11 12 13 14 15 16 17 18 19 20; do
  start=${EPOCHREALTIME/[.,]/}
  /bin/true
  end=${EPOCHREALTIME/[.,]/}
  echo $((end - start))
done
"#;
/// Below this median, in microseconds, a spawn is not held to the 2x ratio.
/// Spawning `/bin/true` takes around a millisecond, where one scheduler
/// hiccup doubles a sample, so a ratio of two such medians measures noise
/// rather than the sandbox.
const LATENCY_FLOOR_MICROS: u64 = 5_000;

/// A fake home, a git repository with its state root, a stage worktree with
/// the state-root symlink, and a host whose facts point into them. All of it
/// lives under `target/`, never `/tmp` or `$TMPDIR`: those are
/// session-writable, so the spawn preflight refuses hooks or a loom binary
/// there, and the relay refuses a scratch root under `/tmp`.
struct Fixture {
    _temp: TempDir,
    base: PathBuf,
    home: PathBuf,
    repo: PathBuf,
    work_dir: PathBuf,
    worktree: PathBuf,
    host: LaunchHost,
}

fn tempdir_outside_tmp() -> TempDir {
    let base = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("target")
        .join("confinement-e2e-tmp");
    std::fs::create_dir_all(&base).unwrap();
    tempfile::Builder::new()
        .prefix("fixture-")
        .tempdir_in(&base)
        .unwrap()
}

fn fixture() -> Fixture {
    let temp = tempdir_outside_tmp();
    let base = temp.path().canonicalize().unwrap();
    let home = base.join("home");
    let repo = base.join("repo");
    let work_dir = repo.join(".loom/work");
    let worktree = repo.join(".worktrees").join(STAGE_ID);
    let (hooks_dir, loom_bin) = fake_home(&home);
    repo_with_worktree(&repo, &worktree);
    let scratch_root = base.join("scratch");
    let host = LaunchHost {
        claude_path: PathBuf::from("/usr/bin/claude"),
        capsule_support: CapsuleSupport {
            settings: true,
            setting_sources: true,
            strict_mcp_config: true,
            append_system_prompt_file: true,
        },
        repo_root: repo.clone(),
        facts: HostFacts {
            writable_roots: writable_roots(&repo, &scratch_root, &home),
            hooks_dir: Some(hooks_dir),
            loom_bin,
            hook_path: vec![PathBuf::from("/usr/bin"), PathBuf::from("/bin")],
            python3: None,
            python_hooks: Vec::new(),
        },
        scratch_root,
        // SAFETY: `getuid` has no preconditions and cannot fail.
        uid: unsafe { libc::getuid() },
        home: Some(home.clone()),
    };
    Fixture {
        _temp: temp,
        base,
        home,
        repo,
        work_dir,
        worktree,
        host,
    }
}

/// This build's hooks in `~/.claude/hooks/loom`, the loom binary alone in
/// `~/.local/bin` (the capsule denies writing `dirname(LOOM_BIN)`), and the
/// other surfaces and caches the probes name. Returns the hooks directory
/// and the binary.
fn fake_home(home: &Path) -> (PathBuf, PathBuf) {
    let hooks_dir = home.join(".claude/hooks/loom");
    for dir in [
        ".loom",
        ".codex",
        ".rustup/toolchains",
        ".cargo/registry",
        ".local/bin",
    ] {
        std::fs::create_dir_all(home.join(dir)).unwrap();
    }
    install_loom_hooks_to(&hooks_dir).unwrap();
    for (file, content) in [
        (".claude/settings.json", "{}"),
        (".loom/config.toml", "[update]\ncheck = false\n"),
        (".codex/hooks.json", "{}"),
        (".local/bin/loom", "#!/bin/sh\n"),
    ] {
        std::fs::write(home.join(file), content).unwrap();
    }
    let loom_bin = home.join(".local/bin/loom");
    std::fs::set_permissions(&loom_bin, std::fs::Permissions::from_mode(0o755)).unwrap();
    (hooks_dir, loom_bin)
}

/// A git checkout with its state root, its own local settings and Remote
/// Control off (so no launch runs a `claude --version` preflight), and a
/// stage worktree holding the state-root symlink the worktree code plants.
fn repo_with_worktree(repo: &Path, worktree: &Path) {
    for dir in [".loom/work", ".loom/cache", ".claude", "src"] {
        std::fs::create_dir_all(repo.join(dir)).unwrap();
    }
    std::fs::create_dir_all(worktree.join("src")).unwrap();
    std::fs::create_dir_all(worktree.join(".claude")).unwrap();
    let status = Command::new("git")
        .args(["init", "-q"])
        .arg(repo)
        .stdin(Stdio::null())
        .status()
        .expect("run git init");
    assert!(status.success(), "git init {}", repo.display());
    std::fs::create_dir_all(repo.join(".git/hooks")).unwrap();
    std::fs::write(repo.join(".claude/settings.local.json"), "{}").unwrap();
    std::fs::write(worktree.join(".claude/settings.json"), "{}").unwrap();
    let remote_control = RemoteControlConfig {
        mode: RemoteControlMode::Off,
    };
    write_remote_control_config(&repo.join(".loom/work"), &remote_control).unwrap();
    crate::git::worktree::ensure_work_symlink(worktree, repo).unwrap();
}

/// What the preflight checks the fixture's hooks and binary against: every
/// root a session of this fixture could write, the codex lane included.
fn writable_roots(repo: &Path, scratch_root: &Path, home: &Path) -> Vec<PathBuf> {
    let tmpdir = std::env::var_os("TMPDIR").map(PathBuf::from);
    session_writable_roots(&WritableRootInputs {
        repo_root: repo,
        allow_write: &[],
        codex_licensed: true,
        scratch_root,
        home: Some(home),
        tmpdir: tmpdir.as_deref(),
    })
}

fn stage(stage_type: StageType, lanes: Vec<Implementer>) -> Stage {
    Stage {
        id: STAGE_ID.to_string(),
        name: "Stage One".to_string(),
        stage_type,
        implementers: Implementers::new(lanes),
        ..Stage::default()
    }
}

/// Launch `kind` for `stage` through the real launch path, from where that
/// kind runs (the worktree for a Stage session, the repository otherwise),
/// translate the capsule it wrote, and check srt starts a shell under it.
fn confine(f: &Fixture, kind: SessionType, stage: &Stage) -> Confined {
    let cwd = if kind == SessionType::Stage {
        f.worktree.clone()
    } else {
        f.repo.clone()
    };
    let signal = f.work_dir.join("signals/sig.md");
    std::fs::create_dir_all(signal.parent().unwrap()).unwrap();
    std::fs::write(&signal, "# Assignment\n").unwrap();
    let session = match kind {
        SessionType::Knowledge => Session::new_knowledge(&stage.id),
        _ => Session::new(),
    };
    let (session, ..) =
        prepare_session_launch_with(&f.host, &f.work_dir, kind, stage, session, &signal, &cwd)
            .unwrap_or_else(|error| panic!("a {kind} launch must succeed: {error:#}"));
    let capsule = std::fs::read_to_string(session_settings_path(&f.work_dir, &session.id)).unwrap();
    let capsule: Value = serde_json::from_str(&capsule).unwrap();
    let settings = f.base.join(format!("{}.srt.json", session.id));
    srt::write_settings(&settings, &capsule, &cwd, &f.home);
    let confined = Confined {
        label: format!("{kind} capsule"),
        settings,
        cwd,
        scratch: f.host.scratch_root.join(&session.id),
    };
    // The control: fail fast when srt cannot start a shell under this
    // capsule at all, before any probe runs through it.
    confined.run_alive("true");
    confined
}

/// The paths no capsule may let a session write, whatever its kind.
fn control_surfaces(f: &Fixture) -> Vec<PathBuf> {
    vec![
        f.work_dir.join("x"),
        f.worktree.join(".loom/work/x"),
        f.repo.join(".loom/cache/x"),
        f.home.join(".claude/hooks/loom/x"),
        f.home.join(".claude/settings.json"),
        f.repo.join(".claude/settings.local.json"),
        f.worktree.join(".claude/settings.json"),
        f.home.join(".loom/config.toml"),
        f.home.join(".local/bin/x"),
        f.home.join(".rustup/toolchains/x"),
        f.repo.join(".git/hooks/x"),
    ]
}

#[test]
#[serial]
fn a_stage_capsule_refuses_every_control_surface_and_writes_its_worktree() {
    if skip("a_stage_capsule_refuses_every_control_surface_and_writes_its_worktree") {
        return;
    }
    let f = fixture();
    let lanes = vec![Implementer::Claude];
    let confined = confine(&f, SessionType::Stage, &stage(StageType::Standard, lanes));

    let mut missed = confined.refusals_missed(&control_surfaces(&f));
    missed.extend(confined.writes_missed(&[
        confined.scratch.join("x"),
        f.worktree.join("src/x"),
        f.home.join(".cargo/registry/x"),
    ]));

    assert!(missed.is_empty(), "{}", missed.join("\n"));
}

#[test]
#[serial]
fn a_knowledge_capsule_refuses_every_control_surface_and_writes_the_checkout() {
    if skip("a_knowledge_capsule_refuses_every_control_surface_and_writes_the_checkout") {
        return;
    }
    let f = fixture();
    let lanes = vec![Implementer::Claude];
    let confined = confine(
        &f,
        SessionType::Knowledge,
        &stage(StageType::Knowledge, lanes),
    );

    let mut missed = confined.refusals_missed(&control_surfaces(&f));
    missed.extend(confined.writes_missed(&[
        confined.scratch.join("x"),
        f.repo.join("src/x"),
        f.home.join(".cargo/registry/x"),
    ]));

    assert!(missed.is_empty(), "{}", missed.join("\n"));
}

#[test]
#[serial]
fn a_codex_lane_capsule_refuses_the_codex_hook_config_present_or_missing() {
    if skip("a_codex_lane_capsule_refuses_the_codex_hook_config_present_or_missing") {
        return;
    }
    let f = fixture();
    let lanes = vec![Implementer::Claude, Implementer::Codex];
    let confined = confine(&f, SessionType::Stage, &stage(StageType::Standard, lanes));
    let hooks_json = f.home.join(".codex/hooks.json");

    // The grant is live, so the refusal below comes from the deny.
    let mut missed = confined.writes_missed(&[f.home.join(".codex/x")]);
    missed.extend(confined.refusals_missed(std::slice::from_ref(&hooks_json)));
    // bwrap can only mount over a path that exists. For a missing deny entry
    // inside a granted directory srt binds `/dev/null` there for the
    // command's lifetime, so a write can land in `/dev/null` and exit 0: the
    // exit status says nothing, and what counts is that no file appears.
    // `write` has already proven the shell ran, so an absent file is not an
    // srt that never started.
    std::fs::remove_file(&hooks_json).unwrap();
    let probe = confined.write(&hooks_json);
    if let Ok(content) = std::fs::read_to_string(&hooks_json) {
        missed.push(format!(
            "{}: {} did not exist at session start and a session created it \
             (write exit code {:?}, content {content:?}): {}",
            confined.label,
            hooks_json.display(),
            probe.rc,
            diagnostics(&probe.output)
        ));
    }

    assert!(missed.is_empty(), "{}", missed.join("\n"));
}

#[test]
#[serial]
fn a_spawn_under_a_stage_capsule_stays_within_twice_the_unsandboxed_latency() {
    if skip("a_spawn_under_a_stage_capsule_stays_within_twice_the_unsandboxed_latency") {
        return;
    }
    let f = fixture();
    let lanes = vec![Implementer::Claude];
    let confined = confine(&f, SessionType::Stage, &stage(StageType::Standard, lanes));
    let script = f.base.join("latency.sh");
    std::fs::write(&script, LATENCY_SCRIPT).unwrap();

    let outside = Command::new("/bin/bash")
        .arg(&script)
        .current_dir(&confined.cwd)
        .stdin(Stdio::null())
        .output()
        .expect("run the latency loop");
    let script_arg = escape(Cow::Owned(script.display().to_string()));
    let inside = confined.run_alive(&format!("/bin/bash {script_arg}"));

    let outside = median_micros(&outside, "outside srt");
    let inside = median_micros(&inside, &format!("under the {}", confined.label));
    let ceiling = (2 * outside).max(LATENCY_FLOOR_MICROS);
    assert!(
        inside <= ceiling,
        "median `/bin/true` spawn: {inside}us under the capsule, {outside}us outside \
         (ceiling {ceiling}us)"
    );
}

/// The median of the latency loop's twenty samples, in microseconds, from
/// the loop's `run`.
fn median_micros(output: &Output, run: &str) -> u64 {
    let stdout = String::from_utf8_lossy(&output.stdout);
    assert!(
        output.status.success(),
        "the latency loop {run} failed: {}",
        diagnostics(output)
    );
    let mut samples: Vec<u64> = stdout
        .lines()
        .filter_map(|line| line.trim().parse().ok())
        .collect();
    assert_eq!(
        samples.len(),
        20,
        "the latency loop {run}: twenty samples expected: {}",
        diagnostics(output)
    );
    samples.sort_unstable();
    samples[samples.len() / 2]
}
