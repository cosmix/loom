//! `loom plan summary` under a macOS Seatbelt profile shaped like the planning session's srt
//! profile: the planning root is read- and write-denied and only the session grants of the
//! project's runtime directory are carved back in. The runtime directory itself stays
//! unreadable, so the store must reach its grants without opening it.
#![cfg(target_os = "macos")]

#[path = "planning_summary_support/mod.rs"]
mod support;

use std::path::{Path, PathBuf};
use std::process::{Command, Output};

use loom::plan_session::sandbox::ensure_runtime_grants;
use loom::planning::runtime::RuntimePaths;
use support::*;

const SANDBOX_EXEC: &str = "/usr/bin/sandbox-exec";
const SESSION: &str = "LOOM_PLAN_SESSION";

/// Why Seatbelt cannot run here, or `None` when a trivial profile starts. A nested Seatbelt
/// (this test running inside another sandbox) refuses `sandbox_apply`.
fn seatbelt_unavailable() -> Option<String> {
    if !Path::new(SANDBOX_EXEC).exists() {
        return Some(format!("{SANDBOX_EXEC} is missing"));
    }
    let probe = Command::new(SANDBOX_EXEC)
        .args(["-p", "(version 1)(allow default)", "/usr/bin/true"])
        .output();
    match probe {
        Ok(out) if out.status.success() => None,
        Ok(out) => Some(format!(
            "sandbox-exec refused to start: {}",
            String::from_utf8_lossy(&out.stderr).trim()
        )),
        Err(error) => Some(format!("sandbox-exec did not run: {error}")),
    }
}

fn quoted(path: &Path) -> String {
    let text = path.to_str().expect("UTF-8 temp path");
    assert!(!text.contains('"') && !text.contains('\\'), "{text}");
    format!("\"{text}\"")
}

/// srt's macOS shape: deny the planning root, then allow each grant (the later rule wins).
/// Seatbelt matches resolved paths, so every path here is canonical (`/private/var/...`).
fn profile(rt: &RuntimePaths) -> String {
    let root = quoted(&rt.planning_root);
    let reads = [&rt.drafts, &rt.claims, &rt.summaries_lock, &rt.attachments];
    let writes = [&rt.drafts, &rt.claims, &rt.summaries_lock];
    let allow = |kind: &str, paths: &[&PathBuf]| -> String {
        paths
            .iter()
            .map(|p| format!("(allow {kind} (subpath {}))\n", quoted(p)))
            .collect()
    };
    format!(
        "(version 1)\n(allow default)\n(deny file-read* (subpath {root}))\n{}\
         (deny file-write* (subpath {root}))\n{}",
        allow("file-read*", &reads),
        allow("file-write*", &writes),
    )
}

/// A git project whose runtime grants exist as the server creates them before a launch;
/// returns the temp dir, the canonical project root, the canonical loom user dir and the
/// runtime paths.
fn session_project() -> (tempfile::TempDir, PathBuf, PathBuf, RuntimePaths) {
    let (t, root, home) = git_project(&[
        (S1, &section(1, "Alpha.", "Beta.")),
        (META, &metaplan(&["PLAN-x.md"])),
        (PLAN_X, "# Plan: X\n\nDoes x.\n"),
    ]);
    let (root, home) = (root.canonicalize().unwrap(), home.canonicalize().unwrap());
    let rt = paths(&root, &home);
    ensure_runtime_grants(&rt).expect("create the session grants");
    (t, root, home, rt)
}

fn confined(root: &Path, home: &Path, rt: &RuntimePaths, args: &[&str]) -> Output {
    let mut command = Command::new(SANDBOX_EXEC);
    for (key, _) in std::env::vars_os() {
        if key.to_string_lossy().starts_with("LOOM_") {
            command.env_remove(key);
        }
    }
    command
        .args([
            "-p",
            &profile(rt),
            env!("CARGO_BIN_EXE_loom"),
            "plan",
            "summary",
        ])
        .args(args)
        .current_dir(root)
        .env("LOOM_HOME", home)
        .env(SESSION, "seatbelt-1");
    command
        .output()
        .expect("run loom plan summary under sandbox-exec")
}

#[test]
fn summary_commands_reach_the_grants_when_the_runtime_dir_is_unreadable() {
    if let Some(reason) = seatbelt_unavailable() {
        eprintln!("skipped: {reason}");
        return;
    }
    let (_t, root, home, rt) = session_project();
    let claim = rt.claims.join("demo__product__section_01.json");

    let out = confined(&root, &home, &rt, &["status"]);
    assert!(out.status.success(), "status: {}", stderr(&out));

    let out = confined(&root, &home, &rt, &["brief", ID]);
    assert!(out.status.success(), "brief: {}", stderr(&out));
    assert!(claim.is_file(), "the brief wrote no claim");

    let out = confined(&root, &home, &rt, &["release", ID]);
    assert!(out.status.success(), "release: {}", stderr(&out));
    assert!(!claim.exists(), "the release kept the claim");
}

#[test]
fn a_session_seals_its_draft_when_the_runtime_dir_is_unreadable() {
    if let Some(reason) = seatbelt_unavailable() {
        eprintln!("skipped: {reason}");
        return;
    }
    let (_t, root, home, rt) = session_project();
    let out = confined(&root, &home, &rt, &["brief", "plans/x"]);
    assert!(out.status.success(), "brief: {}", stderr(&out));
    let draft = write_draft(&root, &home, "plans/x", DRAFT_X);

    let out = confined(&root, &home, &rt, &["seal", "plans/x"]);
    assert!(out.status.success(), "seal: {}", stderr(&out));
    assert!(
        root.join(summary_rel("plans/x")).is_file(),
        "nothing sealed"
    );
    assert!(!draft.exists(), "the sealed draft was kept");
    assert!(
        !rt.claims.join("plans__x.json").exists(),
        "the claim was kept"
    );
}

#[test]
fn the_profile_hides_the_runtime_dir_itself() {
    if let Some(reason) = seatbelt_unavailable() {
        eprintln!("skipped: {reason}");
        return;
    }
    let (_t, _root, _home, rt) = session_project();
    let probe = |path: &Path| {
        Command::new(SANDBOX_EXEC)
            .args(["-p", &profile(&rt), "/bin/ls", "-d"])
            .arg(path)
            .output()
            .expect("run ls under sandbox-exec")
            .status
            .success()
    };
    assert!(!probe(&rt.dir), "the runtime dir is visible");
    assert!(probe(&rt.claims), "the claims grant is hidden");
}
