//! Unit tests for `sandbox/control_surfaces.rs`: the session-writable roots
//! and the propagation filter.

use super::*;

fn inputs(allow_write: &[String], codex_licensed: bool) -> WritableRootInputs<'_> {
    WritableRootInputs {
        repo_root: Path::new("/repo"),
        allow_write,
        codex_licensed,
        scratch_root: Path::new("/home/op/.cache/loom/scratch"),
        home: Some(Path::new("/home/op")),
        tmpdir: Some(Path::new("/var/tmp/op")),
    }
}

#[test]
fn writable_roots_cover_every_input() {
    let allow_write = vec![
        "/srv/out/**".to_string(),
        "~/data".to_string(),
        "//abs/grant".to_string(),
        "loom/target".to_string(),
    ];
    let roots = session_writable_roots(&inputs(&allow_write, true));
    for expected in [
        "/repo",
        "/srv/out",
        "/home/op/data",
        "/abs/grant",
        "/repo/loom/target",
        "/home/op/.cargo/registry",
        "/home/op/.bun/install/cache",
        "/home/op/.codex",
        "/home/op/.claude/plugins/data/codex-openai-codex",
        "/home/op/.cache/loom/scratch",
        "/tmp",
        "/var/tmp/op",
    ] {
        assert!(
            roots.contains(&PathBuf::from(expected)),
            "missing {expected}: {roots:?}"
        );
    }
}

#[test]
fn writable_roots_omit_the_codex_paths_unless_the_lane_is_licensed() {
    let roots = session_writable_roots(&inputs(&[], false));
    assert!(!roots.contains(&PathBuf::from("/home/op/.codex")));
    assert!(roots.contains(&PathBuf::from("/home/op/.cargo/registry")));
}

#[test]
fn grant_root_reads_every_spelling() {
    let base = Path::new("/repo");
    let home = Some(Path::new("/home/op"));
    assert_eq!(
        grant_root("doc/loom/knowledge/**", base, home),
        Some(PathBuf::from("/repo/doc/loom/knowledge"))
    );
    assert_eq!(
        grant_root("**/*.rs", base, home),
        Some(PathBuf::from("/repo"))
    );
    assert_eq!(grant_root("~/cache/x", base, None), None);
    assert_eq!(grant_root("../../escape", base, home), None);
    assert_eq!(grant_root("  ", base, home), None);
}

fn surfaces() -> ControlSurfaces {
    ControlSurfaces::new(
        Path::new("/repo/.loom/work"),
        Some(Path::new("/run/user/1000/loom/scratch")),
        &[PathBuf::from("/opt/loom-hooks")],
        Some(Path::new("/home/op")),
    )
}

#[test]
fn names_every_control_surface() {
    let surfaces = surfaces();
    for rule in [
        "Edit(.loom/work/handoffs/**)",
        "Edit(.work/signals/x.md)",
        "Read(//repo/.loom/work/config.toml)",
        "Edit(.worktrees/s1/**)",
        "Edit(.claude/settings.json)",
        "Write(~/.claude/hooks/loom/x.sh)",
        "Edit(//opt/loom-hooks/loom-relay.sh)",
        "Edit(~/.loom/config.toml)",
        "Edit(~/.claude.json)",
        "Edit(//run/user/1000/loom/scratch/session-1/**)",
        "Edit(~/.codex/hooks.json)",
        "Edit(~/.codex/**)",
        "Edit(**)",
        "Edit(../**)",
        "Edit",
        "Bash(cp x .claude/settings.json)",
        "Bash(rm -rf /opt/loom-hooks/old)",
    ] {
        assert!(surfaces.names(rule), "{rule} must be dropped");
    }
}

#[test]
fn names_every_control_component_regardless_of_case() {
    let surfaces = surfaces();
    for rule in [
        "Edit(.LOOM/work/handoffs/**)",
        "Edit(.Work/signals/x.md)",
        "Edit(.WorkTrees/s1/**)",
        "Edit(.Claude/settings.json)",
    ] {
        assert!(surfaces.names(rule), "{rule} must be dropped");
    }
}

#[test]
fn names_the_repository_git_directory() {
    let surfaces = surfaces();
    for rule in [
        "Edit(.git/**)",
        "Edit(.git/config)",
        "Edit(.git/worktrees/s1/config.worktree)",
        "Edit(.GIT/hooks/pre-commit)",
    ] {
        assert!(surfaces.names(rule), "{rule} must be dropped");
    }
}

#[test]
fn leaves_git_adjacent_dotfiles_alone() {
    let surfaces = surfaces();
    for rule in [
        "Edit(.gitignore)",
        "Edit(.gitattributes)",
        "Edit(.github/**)",
        "Edit(.github/workflows/ci.yml)",
        "Edit(src/git/runner.rs)",
    ] {
        assert!(!surfaces.names(rule), "{rule} must be kept");
    }
}

#[test]
fn names_a_root_prefix_regardless_of_case() {
    // Neither "opt", "loom-hooks", "run", "user" nor "scratch" is a
    // `CONTROL_COMPONENTS` entry, so these can only be caught by the
    // root-prefix comparison, not the component check above.
    let surfaces = surfaces();
    for rule in [
        "Edit(//OPT/loom-hooks/loom-relay.sh)",
        "Edit(//opt/LOOM-HOOKS/loom-relay.sh)",
        "Edit(//RUN/user/1000/loom/scratch/session-1/**)",
    ] {
        assert!(surfaces.names(rule), "{rule} must be dropped");
    }
}

#[test]
fn leaves_ordinary_rules_alone() {
    let surfaces = surfaces();
    for rule in [
        "Bash(cargo test:*)",
        "Edit(loom/src/**)",
        "WebFetch(domain:docs.rs)",
        "Read(//usr/share/doc/**)",
        "Edit(~/.cargo/registry/**)",
        "mcp__github__search",
        "Bash",
    ] {
        assert!(!surfaces.names(rule), "{rule} must be kept");
    }
}

#[test]
fn exposes_the_executable_dirs_and_home_it_was_built_with() {
    let surfaces = surfaces();
    assert_eq!(
        surfaces.executable_dirs(),
        [PathBuf::from("/opt/loom-hooks")]
    );
    assert_eq!(surfaces.home(), Some(Path::new("/home/op")));
}
