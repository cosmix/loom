use super::*;

fn foreign_filter_repo(root: &Path) -> std::path::PathBuf {
    let foreign = root.join("foreign");
    fs::create_dir(&foreign).unwrap();
    git(&foreign, &["init", "-b", "main"]);
    git(&foreign, &["config", "user.name", "Test"]);
    git(&foreign, &["config", "user.email", "test@example.invalid"]);
    let marker = root.join("filter-ran");
    let filter = format!("sh -c 'touch \"{}\"; cat'", marker.display());
    git(&foreign, &["config", "filter.trap.clean", &filter]);
    write(&foreign, ".gitattributes", "*.md filter=trap\n");
    write(&foreign, PLAN, "foreign plan\n");
    git(&foreign, &["add", "."]);
    git(&foreign, &["commit", "-m", "foreign"]);
    fs::remove_file(&marker).unwrap();
    // Same size as the committed text: git reports a size change as modified
    // without hashing, and only the hash runs the clean filter.
    write(&foreign, PLAN, "FOREIGN PLAN\n");
    git(&foreign, &["status", "--porcelain"]);
    assert!(
        marker.exists(),
        "positive control must run the configured filter"
    );
    fs::remove_file(&marker).unwrap();
    foreign
}

#[test]
fn linked_checkout_input_checks_ignore_a_replaced_git_pointer() {
    let (temp, _work) = fixture();
    let root = temp.path();
    commit_inputs(root);
    let linked = root.join("linked");
    git(
        root,
        &[
            "worktree",
            "add",
            "-b",
            "operator",
            linked.to_str().unwrap(),
        ],
    );
    let linked_work = WorkDir::new(&linked).unwrap();
    fs::create_dir_all(linked_work.root()).unwrap();
    configure(&linked_work, &linked.join(PLAN));
    let mut config = fs::read_to_string(linked_work.root().join("config.toml"))
        .unwrap()
        .parse::<toml_edit::DocumentMut>()
        .unwrap();
    config["plan"]["base_branch"] = toml_edit::value("operator");
    crate::fs::work_dir::write_config(linked_work.root(), &config).unwrap();
    crate::git::worktree::capture_main_identity(&linked, linked_work.root()).unwrap();
    let foreign = foreign_filter_repo(root);
    fs::write(
        linked.join(".git"),
        format!("gitdir: {}\n", foreign.join(".git").display()),
    )
    .unwrap();
    require_committed_plan(&linked_work).unwrap();
    assert!(
        !root.join("filter-ran").exists(),
        "startup must not load foreign Git configuration"
    );
}

#[test]
fn startup_rejects_missing_identity_before_reading_git() {
    let (temp, work) = fixture();
    commit_inputs(temp.path());
    fs::remove_file(work.root().join("main-checkout-identity.json")).unwrap();
    let error = require_committed_plan(&work).unwrap_err();
    assert!(format!("{error:#}").contains("checkout identity missing"));
}
