//! Contract: the generated confined policy wraps the command in the OS
//! launcher and points it at the stage cache, without starting a process.

use crate::process::sandbox_probe::skip_unless;
use crate::stage_cache::StageCache;
use crate::verify::criteria::{confined_policy, host_launcher, CommandSpec};
use std::path::{Path, PathBuf};

/// A TempDir and its canonical path (macOS temp paths start at a symlink).
fn temp() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("create temp dir");
    let path = dir.path().canonicalize().expect("canonicalize temp dir");
    (dir, path)
}

#[test]
fn the_generated_policy_carries_the_stage_cache() {
    let name = "the_generated_policy_carries_the_stage_cache";
    if skip_unless(
        host_launcher().is_some(),
        name,
        "OS sandbox launcher absent",
    ) {
        return;
    }
    let ((_r, repo), (_h, home), (_t, tree)) = (temp(), temp(), temp());
    let cache = StageCache::resolve(&repo, "alpha", &home, None).expect("resolve stage cache");
    let spec = CommandSpec::shell("true");

    let policy = confined_policy(&spec, &tree, &cache).expect("build the confined policy");

    let launcher = policy.program.file_name().and_then(|n| n.to_str());
    assert!(
        matches!(launcher, Some("sandbox-exec" | "bwrap")),
        "the program must be the OS launcher, got {:?}",
        policy.program
    );
    let gocache = policy.env.get("GOCACHE").expect("GOCACHE is set");
    assert!(
        Path::new(gocache).starts_with(cache.root()),
        "GOCACHE {gocache} is not under {:?}",
        cache.root()
    );
    let root = cache.root().display().to_string();
    assert!(
        policy.args.iter().any(|arg| arg.contains(&root)),
        "no launcher argument names the stage cache root {root}: {:?}",
        policy.args
    );
}
