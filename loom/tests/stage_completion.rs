//! Integration tests for host-authoritative stage completion.
//!
//! Stage 4 of the container security hardening plan replaces the `.git`
//! bind mount with a container-private clone. Completion becomes
//! host-authoritative: the in-container `loom stage complete` sends
//! `Request::CompleteStageContainer` to the daemon, which extracts a
//! bundle from the LIVE container, validates it, and imports it before
//! killing the session.
//!
//! These tests exercise the host-side validation + import flow against
//! real git repositories without requiring a container runtime. The
//! actual daemon RPC handshake is exercised by the unit tests in
//! `daemon/server/client.rs`; here we focus on the end-to-end bundle
//! lifecycle that the daemon handler invokes internally.

mod container_rpc {
    use loom::orchestrator::terminal::container::git_bridge;
    use std::path::Path;
    use std::process::Command;
    use tempfile::TempDir;

    /// Initialize a git repo at `path` with one commit on `main`.
    /// Returns the OID of HEAD.
    fn init_simple_repo(path: &Path) -> String {
        Command::new("git")
            .args(["init", "-b", "main"])
            .current_dir(path)
            .output()
            .expect("git init");
        Command::new("git")
            .args(["config", "user.email", "t@t"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["config", "user.name", "T"])
            .current_dir(path)
            .output()
            .unwrap();
        std::fs::write(path.join("README.md"), "hi").unwrap();
        Command::new("git")
            .args(["add", "README.md"])
            .current_dir(path)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "first"])
            .current_dir(path)
            .output()
            .unwrap();
        let out = Command::new("git")
            .args(["rev-parse", "HEAD"])
            .current_dir(path)
            .output()
            .unwrap();
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    /// Produce a real bundle from `repo` for the given branch,
    /// anchored on `base_oid`. Adds a single commit on the target
    /// branch first so the bundle has new content.
    fn make_real_bundle(
        repo: &Path,
        bundle_path: &Path,
        target_branch: &str,
        base_oid: &str,
        content: &str,
    ) {
        Command::new("git")
            .args(["checkout", "-B", target_branch])
            .current_dir(repo)
            .output()
            .unwrap();
        std::fs::write(repo.join("new.txt"), content).unwrap();
        Command::new("git")
            .args(["add", "new.txt"])
            .current_dir(repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "n"])
            .current_dir(repo)
            .output()
            .unwrap();
        let out = Command::new("git")
            .arg("-C")
            .arg(repo)
            .args(["bundle", "create"])
            .arg(bundle_path)
            .arg(target_branch)
            .arg(format!("^{base_oid}"))
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "git bundle create failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// End-to-end: validate + import a well-formed bundle from a
    /// per-stage bare mirror initialized by [`init_bare_mirror`].
    /// Mimics the host-authoritative flow performed by the daemon
    /// handler after extracting a bundle from a live container.
    #[test]
    fn validate_then_import_round_trip() {
        let tmp = TempDir::new().unwrap();
        let host_repo = tmp.path().join("host");
        std::fs::create_dir_all(&host_repo).unwrap();
        let base = init_simple_repo(&host_repo);

        let work_dir = tmp.path().join(".work");
        std::fs::create_dir_all(&work_dir).unwrap();

        let mirror = git_bridge::bare_mirror_path(&work_dir, "rpc-stage");
        git_bridge::init_bare_mirror(&host_repo, &mirror, "main", Some(50), &[])
            .expect("init_bare_mirror");

        let bundle = git_bridge::bundle_staging_path(&work_dir, "rpc-stage");
        // git_bridge::bundle_staging_path lives next to the mirror
        // — ensure the parent dir exists before we write the bundle.
        if let Some(parent) = bundle.parent() {
            std::fs::create_dir_all(parent).unwrap();
        }
        make_real_bundle(
            &host_repo,
            &bundle,
            "loom/rpc-stage",
            &base,
            "rpc-content\n",
        );

        let v = git_bridge::validate_bundle(&bundle, &base, "loom/rpc-stage")
            .expect("validate_bundle should accept a well-formed bundle");
        assert!(!v.new_tip.is_empty());

        git_bridge::import_bundle(&mirror, &bundle, "loom/rpc-stage").expect("import_bundle");

        // After import, the bare mirror must contain the imported ref.
        let out = Command::new("git")
            .arg("-C")
            .arg(&mirror)
            .args(["rev-parse", "refs/heads/loom/rpc-stage"])
            .output()
            .unwrap();
        assert!(
            out.status.success(),
            "imported ref must be resolvable: {}",
            String::from_utf8_lossy(&out.stderr)
        );
    }

    /// A bundle whose target branch differs from what the daemon
    /// expects must be REJECTED with no side effects.
    #[test]
    fn rejects_branch_mismatch() {
        let tmp = TempDir::new().unwrap();
        let host_repo = tmp.path().join("host");
        std::fs::create_dir_all(&host_repo).unwrap();
        let base = init_simple_repo(&host_repo);

        let bundle = tmp.path().join("mismatch.bundle");
        make_real_bundle(&host_repo, &bundle, "loom/other", &base, "x\n");

        let err = git_bridge::validate_bundle(&bundle, &base, "loom/expected")
            .expect_err("must reject mismatched branch");
        assert!(
            err.to_string().contains("does not export expected ref"),
            "error must mention missing target ref: {err}"
        );
    }

    /// A bundle that omits the spawn-time base OID as a prerequisite
    /// (i.e., the agent force-rebased) must be REJECTED.
    #[test]
    fn rejects_missing_base_oid_prerequisite() {
        let tmp = TempDir::new().unwrap();
        let host_repo = tmp.path().join("host");
        std::fs::create_dir_all(&host_repo).unwrap();
        let _base = init_simple_repo(&host_repo);

        // Bundle without any prerequisite (we don't pass `^<base>`).
        Command::new("git")
            .args(["checkout", "-B", "loom/anchored"])
            .current_dir(&host_repo)
            .output()
            .unwrap();
        std::fs::write(host_repo.join("z.txt"), "z").unwrap();
        Command::new("git")
            .args(["add", "z.txt"])
            .current_dir(&host_repo)
            .output()
            .unwrap();
        Command::new("git")
            .args(["commit", "-m", "z"])
            .current_dir(&host_repo)
            .output()
            .unwrap();
        let bundle = tmp.path().join("rebased.bundle");
        let out = Command::new("git")
            .arg("-C")
            .arg(&host_repo)
            .args(["bundle", "create"])
            .arg(&bundle)
            .arg("loom/anchored")
            .output()
            .unwrap();
        assert!(out.status.success());

        let fake_base = "0000000000000000000000000000000000000000";
        let err = git_bridge::validate_bundle(&bundle, fake_base, "loom/anchored")
            .expect_err("force-rebased bundle must be rejected");
        assert!(
            err.to_string().contains("expected base OID"),
            "error must mention missing prerequisite: {err}"
        );
    }

    /// Oversized bundles are rejected before any expensive parsing,
    /// so the daemon cannot be DoS'd by a runaway agent producing
    /// a huge bundle.
    #[test]
    fn rejects_bundle_exceeding_size_cap() {
        let tmp = TempDir::new().unwrap();
        let bundle = tmp.path().join("huge.bundle");
        let f = std::fs::File::create(&bundle).unwrap();
        f.set_len(git_bridge::MAX_BUNDLE_BYTES + 1).unwrap();

        let err = git_bridge::validate_bundle(&bundle, "anybase", "anybranch")
            .expect_err("oversized bundle must be rejected before parse");
        assert!(
            err.to_string().contains("MAX_BUNDLE_BYTES"),
            "error must mention MAX_BUNDLE_BYTES: {err}"
        );
    }

    /// The bare mirror produced by [`init_bare_mirror`] must be
    /// self-contained — no `objects/info/alternates` referring to
    /// host-side paths that wouldn't exist inside the container.
    #[test]
    fn bare_mirror_is_self_contained() {
        let tmp = TempDir::new().unwrap();
        let host_repo = tmp.path().join("host");
        std::fs::create_dir_all(&host_repo).unwrap();
        init_simple_repo(&host_repo);

        let work_dir = tmp.path().join(".work");
        let mirror = git_bridge::bare_mirror_path(&work_dir, "stage-self");
        git_bridge::init_bare_mirror(&host_repo, &mirror, "main", Some(50), &[])
            .expect("init_bare_mirror");

        let alts = mirror.join("objects/info/alternates");
        assert!(
            !alts.exists(),
            "bare mirror must not contain alternates pointing at host paths: {}",
            alts.display()
        );

        // `git fsck` inside the mirror should succeed without errors.
        let fsck = Command::new("git")
            .arg("-C")
            .arg(&mirror)
            .args(["fsck"])
            .output()
            .unwrap();
        assert!(
            fsck.status.success(),
            "git fsck failed inside bare mirror: stdout={} stderr={}",
            String::from_utf8_lossy(&fsck.stdout),
            String::from_utf8_lossy(&fsck.stderr)
        );
    }

    /// The container-mode detection helper used by `loom stage
    /// complete` keys solely on `LOOM_BACKEND=container`. Anything
    /// else means native completion.
    #[test]
    fn is_container_completion_detects_env_var() {
        use loom::commands::stage::complete::is_container_completion;

        // Default: var unset → false.
        std::env::remove_var("LOOM_BACKEND");
        assert!(!is_container_completion(), "no env var → native");

        // Set to "container" → true.
        std::env::set_var("LOOM_BACKEND", "container");
        assert!(is_container_completion(), "container value → true");

        // Case-insensitive — operators may set "Container" or "CONTAINER".
        std::env::set_var("LOOM_BACKEND", "Container");
        assert!(is_container_completion(), "mixed-case container → true");

        // Anything else → false.
        std::env::set_var("LOOM_BACKEND", "native");
        assert!(!is_container_completion(), "native value → false");

        std::env::remove_var("LOOM_BACKEND");
    }
}
