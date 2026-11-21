//! Reading a generated confined policy, and the host fixtures its tests share
//! with `kernel_tests`.

use super::write_file;
use crate::process::sandbox_probe::skip_unless;
use crate::stage_cache::StageCache;
use crate::verify::criteria::{confined_policy, host_launcher, CommandSpec, ConfinedPolicy};
use std::path::{Path, PathBuf};

/// The policy for a trivial command. `None` after a `SKIP` line when the OS
/// launcher is absent (`skip_unless` panics instead under
/// `LOOM_TEST_REQUIRE_SANDBOX_FREE=1`). Any other failure to generate the policy
/// panics: it is a regression in policy generation, not an environment gap.
pub(in crate::stage_cache) fn policy_for(
    test: &str,
    tree: &Path,
    cache: &StageCache,
) -> Option<ConfinedPolicy> {
    if host_launcher().is_none() {
        skip_unless(false, test, "the OS launcher is absent");
        return None;
    }
    Some(
        confined_policy(&CommandSpec::shell("true"), tree, cache)
            .expect("generate the confined policy"),
    )
}

/// The Seatbelt profile, the argument after `-p`.
pub(super) fn profile(policy: &ConfinedPolicy) -> &str {
    let at = policy.args.iter().position(|arg| arg == "-p");
    &policy.args[at.expect("sandbox-exec takes -p") + 1]
}

/// Each path a `head` rule names as `(subpath "..")`, with the rule's offset.
pub(super) fn profile_rules(profile: &str, head: &str) -> Vec<(usize, PathBuf)> {
    let mut found = Vec::new();
    for (at, _) in profile.match_indices(head) {
        let mut rest = &profile[at + head.len()..];
        while let Some(quoted) = rest.strip_prefix("(subpath ") {
            let mut values = serde_json::Deserializer::from_str(quoted).into_iter::<String>();
            let Some(Ok(path)) = values.next() else { break };
            found.push((at, PathBuf::from(path)));
            rest = quoted[values.byte_offset()..].trim_start_matches(')');
        }
    }
    found
}

/// Each source `flag` binds into the Bubblewrap sandbox, with its position.
pub(super) fn binds(policy: &ConfinedPolicy, flag: &str) -> Vec<(usize, PathBuf)> {
    let args = &policy.args;
    let at = (0..args.len()).filter(|&at| args[at] == flag && at + 1 < args.len());
    at.map(|at| (at, PathBuf::from(&args[at + 1]))).collect()
}

/// Every root the policy lets the command read or write.
pub(in crate::stage_cache) fn granted(policy: &ConfinedPolicy) -> Vec<PathBuf> {
    let paths: Vec<(usize, PathBuf)> = if cfg!(target_os = "macos") {
        let read = profile_rules(profile(policy), "(allow file-read* ");
        read.into_iter()
            .chain(profile_rules(profile(policy), "(allow file-write* "))
            .collect()
    } else {
        let bound = binds(policy, "--bind");
        bound
            .into_iter()
            .chain(binds(policy, "--ro-bind"))
            .collect()
    };
    paths.into_iter().map(|(_, path)| path).collect()
}

/// The three host seed directories and the directories holding a sentinel.
pub(in crate::stage_cache) fn seed_fixture(home: &Path) -> ([PathBuf; 3], [PathBuf; 3]) {
    let seeds = [
        "go/pkg/mod/cache/download",
        ".m2/repository",
        ".gradle/caches",
    ];
    let seeds = seeds.map(|seed| home.join(seed));
    let dirs = [
        seeds[0].clone(),
        seeds[1].clone(),
        seeds[2].join("modules-2"),
    ];
    for dir in &dirs {
        write_file(&dir.join("sentinel"), "SENTINEL\n");
    }
    (seeds, dirs)
}

/// Two executables below `home`, one reached through a symlink on `PATH`;
/// returns the targets and the `PATH` entries.
pub(in crate::stage_cache) fn toolchain_fixture(home: &Path) -> ([PathBuf; 3], [PathBuf; 2]) {
    use std::os::unix::fs::PermissionsExt;
    let install = home.join(".bun/install/global/node_modules/x/bin/faketool");
    let other = home.join("tools/bin/othertool");
    for (tool, word) in [(&install, "TOOL_OK"), (&other, "OTHER_OK")] {
        write_file(tool, &format!("#!/bin/sh\necho {word}\n"));
        std::fs::set_permissions(tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    }
    let local_bin = home.join(".local/bin");
    std::fs::create_dir_all(&local_bin).unwrap();
    std::os::unix::fs::symlink(&install, local_bin.join("faketool")).unwrap();
    let linked = local_bin.join("faketool");
    (
        [install, other, linked],
        [local_bin, home.join("tools/bin")],
    )
}

/// `PATH` entries naming two credential directories and a control directory.
pub(in crate::stage_cache) fn credential_fixture(home: &Path) -> [PathBuf; 3] {
    write_file(&home.join(".ssh/key"), "SECRET_KEY\n");
    std::fs::create_dir_all(home.join(".aws/bin")).unwrap();
    write_file(&home.join("tools/bin/marker"), "MARKER_OK\n");
    [".ssh", ".aws/bin", "tools/bin"].map(|entry| home.join(entry))
}
