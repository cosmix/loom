//! Launch-level proof that the session wrapper and capsule written by the real
//! launch path carry the stage cache, resolved the way `LaunchHost::stage_cache`
//! resolves it for the stage.

use super::tests_launch_capsule::{capsule_of, fixture, launch, Fixture};
use super::*;
use crate::orchestrator::terminal::launch::tests_session_settings::ALL_KINDS;

/// The root `LaunchHost::stage_cache` resolves for `stage-1`, as a string.
fn stage_root(fixture: &Fixture) -> String {
    fixture
        .host
        .stage_cache(&fixture.work_dir, "stage-1")
        .unwrap()
        .root()
        .display()
        .to_string()
}

/// The value the wrapper's `exec env -i` line assigns to `name`. Export lines
/// are `    'NAME=value' \` (single quotes only when the value needs them).
fn exported(wrapper: &str, name: &str) -> Option<String> {
    let prefix = format!("{name}=");
    wrapper.lines().find_map(|line| {
        let assignment = line.trim().strip_suffix('\\')?.trim();
        let assignment = assignment
            .strip_prefix('\'')
            .and_then(|rest| rest.strip_suffix('\''))
            .unwrap_or(assignment);
        assignment.strip_prefix(&prefix).map(str::to_string)
    })
}

fn allow_write(fixture: &Fixture, session: &Session) -> Vec<String> {
    capsule_of(fixture, session)["sandbox"]["filesystem"]["allowWrite"]
        .as_array()
        .unwrap()
        .iter()
        .map(|entry| entry.as_str().unwrap().to_string())
        .collect()
}

#[test]
fn a_stage_session_wrapper_exports_the_go_and_xdg_caches_under_the_stage_cache_root() {
    let fixture = fixture();
    let root = stage_root(&fixture);
    let (_, wrapper) = launch(&fixture, SessionType::Stage);
    assert_eq!(
        exported(&wrapper, "GOCACHE"),
        Some(format!("{root}/go-build")),
        "{wrapper}"
    );
    assert_eq!(
        exported(&wrapper, "XDG_CACHE_HOME"),
        Some(format!("{root}/xdg")),
        "{wrapper}"
    );
}

#[test]
fn a_stage_session_capsule_allows_writing_the_stage_cache_root_and_the_root_exists() {
    let fixture = fixture();
    let root = stage_root(&fixture);
    let (session, _) = launch(&fixture, SessionType::Stage);
    assert!(allow_write(&fixture, &session).contains(&root));
    assert!(
        Path::new(&root).is_dir(),
        "Claude Code skips an allowWrite path that does not exist"
    );
}

/// Wrapper only: under `cfg(test)` the cache root sits in the repository's
/// `.loom/work`, which is inside the cwd of every non-Stage kind, so the
/// capsule builder drops the redundant grant for them
/// (`sandbox::grant_paths::is_inside_cwd`). A real root is outside the repo.
#[test]
fn every_session_kind_of_a_stage_exports_that_stages_cache_root_in_its_wrapper() {
    let fixture = fixture();
    let root = stage_root(&fixture);
    for kind in ALL_KINDS {
        let (_, wrapper) = launch(&fixture, kind);
        assert_eq!(
            exported(&wrapper, "GOCACHE"),
            Some(format!("{root}/go-build")),
            "{kind}: {wrapper}"
        );
        assert_eq!(
            exported(&wrapper, "XDG_CACHE_HOME"),
            Some(format!("{root}/xdg")),
            "{kind}"
        );
    }
}

#[test]
fn sessions_of_different_stages_get_different_cache_roots() {
    let fixture = fixture();
    let other = fixture
        .host
        .stage_cache(&fixture.work_dir, "stage-2")
        .unwrap();
    assert_ne!(other.root().display().to_string(), stage_root(&fixture));
}
