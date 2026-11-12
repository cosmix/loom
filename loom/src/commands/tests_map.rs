use super::*;
use clap::{CommandFactory, Parser};

#[derive(Debug, Parser)]
struct MapHarness {
    #[command(flatten)]
    args: MapArgs,
}

#[test]
fn map_flags_parse_with_the_contract_defaults() {
    let parsed = MapHarness::try_parse_from(["map", "--impact", "target"]).unwrap();

    assert_eq!(parsed.args.depth, 3);
    assert_eq!(parsed.args.limit, 50);
    assert_eq!(parsed.args.min_confidence, 0.0);
    assert_eq!(
        parsed.args.kinds,
        vec![
            SourceEdgeKind::Calls,
            SourceEdgeKind::References,
            SourceEdgeKind::Implements,
            SourceEdgeKind::Extends,
        ]
    );
    assert_eq!(parsed.args.window_lines, 60);
    assert!(parsed.args.evidence.is_empty());
    assert!(!parsed.args.json);
    assert!(!parsed.args.timings);
}

#[test]
fn kinds_accept_the_stable_comma_separated_names() {
    let parsed =
        MapHarness::try_parse_from(["map", "--impact", "target", "--kinds", "calls,references"])
            .unwrap();

    assert_eq!(
        parsed.args.kinds,
        vec![SourceEdgeKind::Calls, SourceEdgeKind::References]
    );
}

#[test]
fn kinds_reject_an_unknown_name_and_list_every_valid_kind() {
    let error =
        MapHarness::try_parse_from(["map", "--impact", "target", "--kinds", "calls,unknown"])
            .expect_err("an unknown edge kind must be a clap error")
            .to_string();

    assert!(error.contains("unknown edge kind 'unknown'"));
    assert!(error.contains(EDGE_KIND_NAMES));
}

#[test]
fn map_without_a_view_flag_names_all_available_views() {
    let parsed = MapHarness::try_parse_from(["map"]).unwrap();

    let error = require_view(&parsed.args).unwrap_err().to_string();

    assert_eq!(
        error,
        "loom map needs a view flag: --outline <PATH>, --find-all <SYMBOL>, \
         --impact <SYMBOL_OR_PATH>, --callers <SYMBOL>, --callees <SYMBOL>, \
         --references <SYMBOL>, --window <ID>, --census, or --eval-edges <DIR>"
    );
}

#[test]
fn a_markdown_outline_is_read_directly_only_when_it_is_the_only_view() {
    let root = Path::new(".");
    let alone = MapHarness::try_parse_from(["map", "--outline", "x.md"]).unwrap();
    assert!(markdown_outline_output(&alone.args, root).is_some());

    let combined =
        MapHarness::try_parse_from(["map", "--outline", "x.md", "--find-all", "sym"]).unwrap();
    assert!(markdown_outline_output(&combined.args, root).is_none());

    let source = MapHarness::try_parse_from(["map", "--outline", "x.rs"]).unwrap();
    assert!(markdown_outline_output(&source.args, root).is_none());
}

#[test]
fn every_new_view_flag_counts_as_a_view() {
    for argv in [
        vec!["map", "--references", "x"],
        vec!["map", "--window", "src/a.rs#function:a"],
        vec!["map", "--census"],
    ] {
        let parsed = MapHarness::try_parse_from(argv.clone()).unwrap();
        assert!(require_view(&parsed.args).is_ok(), "{argv:?}");
    }
}

#[test]
fn evidence_and_lang_parse_registry_names() {
    let parsed = MapHarness::try_parse_from([
        "map",
        "--impact",
        "t",
        "--evidence",
        "import,receiver",
        "--lang",
        "python",
    ])
    .unwrap();

    assert_eq!(
        parsed.args.evidence,
        vec![EdgeProvenance::Import, EdgeProvenance::Receiver]
    );
    assert_eq!(parsed.args.lang.as_deref(), Some("python"));
    assert!(MapHarness::try_parse_from(["map", "--impact", "t", "--evidence", "guess"]).is_err());
    assert!(MapHarness::try_parse_from(["map", "--impact", "t", "--lang", "cobol"]).is_err());
}

#[test]
fn census_takes_roots_and_no_other_view() {
    let parsed =
        MapHarness::try_parse_from(["map", "--census", "--root", "../a", "--root", "../b"])
            .unwrap();
    assert_eq!(parsed.args.root.len(), 2);

    let stray = MapHarness::try_parse_from(["map", "--root", "../a", "--find-all", "x"]).unwrap();
    assert_eq!(
        require_view(&stray.args).unwrap_err().to_string(),
        "--root only applies to --census"
    );
    assert!(MapHarness::try_parse_from(["map", "--census", "--find-all", "x"]).is_err());
}

#[test]
fn help_describes_direct_one_hop_neighbours_and_lists_the_new_flags() {
    let help = MapHarness::command().render_long_help().to_string();

    assert!(help.contains("one hop over call edges"), "{help}");
    assert!(!help.contains("transitive callers"), "{help}");
    for flag in [
        "--references",
        "--window",
        "--window-lines",
        "--census",
        "--root",
        "--lang",
        "--timings",
        "--evidence",
    ] {
        assert!(help.contains(flag), "help lacks {flag}: {help}");
    }
}

#[test]
fn eval_edges_alone_is_a_view_flag() {
    let parsed = MapHarness::try_parse_from(["map", "--eval-edges", "corpora"]).unwrap();

    assert!(require_view(&parsed.args).is_ok());
    assert_eq!(requested_views(&parsed.args), 1);
    assert_eq!(
        parsed.args.eval_edges.as_deref(),
        Some(Path::new("corpora"))
    );
}

#[test]
fn thresholds_need_eval_edges() {
    assert!(
        MapHarness::try_parse_from(["map", "--find-all", "x", "--thresholds", "t.yaml"]).is_err()
    );

    let parsed =
        MapHarness::try_parse_from(["map", "--eval-edges", "d", "--thresholds", "t.yaml"]).unwrap();
    assert_eq!(parsed.args.thresholds.as_deref(), Some(Path::new("t.yaml")));
}

#[test]
fn eval_edges_conflicts_with_census_and_takes_no_root() {
    assert!(MapHarness::try_parse_from(["map", "--census", "--eval-edges", "d"]).is_err());

    let stray = MapHarness::try_parse_from(["map", "--eval-edges", "d", "--root", "r"]).unwrap();
    assert_eq!(
        require_view(&stray.args).unwrap_err().to_string(),
        "--root only applies to --census"
    );
}

/// Run one git setup command with ambient global/system config neutralized
/// (same isolation as `context::refresh::tests_source_graph`).
fn git_setup(root: &Path, args: &[&str]) {
    let out = std::process::Command::new("git")
        .args(args)
        .current_dir(root)
        .env("GIT_CONFIG_GLOBAL", root.join(".loom-test-no-global"))
        .env("GIT_CONFIG_SYSTEM", root.join(".loom-test-no-system"))
        .env("GIT_CONFIG_NOSYSTEM", "1")
        .output()
        .unwrap();
    assert!(
        out.status.success(),
        "git {args:?} failed: {}",
        String::from_utf8_lossy(&out.stderr)
    );
}

fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(mode)).unwrap();
}

/// `loom map` run where the sandbox denies writes under `.loom` and the cache directory was
/// never created: the view still resolves from memory and nothing appears on disk.
#[test]
fn load_graph_answers_when_the_cache_directory_cannot_be_created() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();
    git_setup(root, &["init", "-b", "main"]);
    git_setup(root, &["config", "user.email", "t@t.com"]);
    git_setup(root, &["config", "user.name", "t"]);
    std::fs::write(root.join("src.rs"), "fn main() {}\n").unwrap();
    git_setup(root, &["add", "src.rs"]);
    git_setup(root, &["commit", "-m", "seed"]);
    let work = root.join(".loom").join("work");
    std::fs::create_dir_all(&work).unwrap();
    crate::git::worktree::capture_main_identity(root, &work).unwrap();
    let work_dir = WorkDir::new(root).unwrap();

    let loom_dir = root.join(".loom");
    set_mode(&work, 0o555);
    set_mode(&loom_dir, 0o555);
    if std::fs::write(loom_dir.join(".write-probe"), b"x").is_ok() {
        set_mode(&loom_dir, 0o755);
        set_mode(&work, 0o755);
        eprintln!(
            "SKIP load_graph_answers_when_the_cache_directory_cannot_be_created: this \
             environment does not enforce 0o555 directory permissions"
        );
        return;
    }

    let loaded = load_graph(root, &work_dir, &mut Timings::default());

    // Unlock before asserting so a failure never leaves an undeletable TempDir.
    set_mode(&loom_dir, 0o755);
    set_mode(&work, 0o755);
    let loaded = loaded.expect("a read-only .loom must not fail the view");
    assert!(
        loaded.graph.files.contains_key("src.rs"),
        "files: {:?}",
        loaded.graph.files.keys().collect::<Vec<_>>()
    );
    assert!(!loom_dir.join("cache").exists());
}

/// With no graph and no base to serve, `loom map` fails instead of printing
/// empty views as if the file had no symbols.
#[test]
fn load_graph_fails_when_the_source_graph_is_unavailable() {
    let temp = tempfile::TempDir::new().unwrap();
    let root = temp.path();
    let work_dir = WorkDir::new(root).unwrap();

    let error = match load_graph(root, &work_dir, &mut Timings::default()) {
        Ok(_) => panic!("a directory without git has no graph to answer from"),
        Err(error) => format!("{error:#}"),
    };

    assert!(error.starts_with("source graph unavailable: "), "{error}");
}
