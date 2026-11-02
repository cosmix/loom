use super::*;

#[test]
#[serial]
fn daemon_reconcile_unattributed_merge_logs_only() {
    let repo = init_repo();
    let root = repo.path();
    let work_dir = make_work_dir(root);

    // Manufacture MERGE_HEAD with bogus SHA.
    fs::write(
        root.join(".git").join("MERGE_HEAD"),
        "deadbeefcafebabe1234567890abcdef12345678\n",
    )
    .unwrap();

    let outcome = reconcile_main_repo_active_merge(root, &work_dir).unwrap();
    assert_eq!(outcome, ReconciliationOutcome::UnattributedLogged);

    fs::remove_file(root.join(".git").join("MERGE_HEAD")).ok();
}
