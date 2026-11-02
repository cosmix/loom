use super::*;

#[test]
fn same_model_siblings_join_only_their_exact_tuple() -> Result<()> {
    let root = tempfile::tempdir()?;
    let left_id = identity("agent-left", "tool-left", "stage-a");
    let right_id = identity("agent-right", "tool-right", "stage-a");
    write_receipt(
        root.path(),
        &left_id,
        ForwardBackend::Companion,
        "job-left",
        None,
    )?;
    write_receipt(
        root.path(),
        &right_id,
        ForwardBackend::Companion,
        "job-right",
        None,
    )?;
    let join = ForwardJoin::load(Some(root.path()));
    let (mut left, mut right) = (transcript(&left_id), transcript(&right_id));

    join.join_transcript(&mut left);
    join.join_transcript(&mut right);

    assert_eq!(left.forward_receipt.unwrap().backend_id, "job-left");
    assert_eq!(right.forward_receipt.unwrap().backend_id, "job-right");
    Ok(())
}

#[test]
fn codex_thread_joins_one_unique_receipt_and_redacts_private_fields() -> Result<()> {
    let root = tempfile::tempdir()?;
    let id = identity("agent-1", "tool-1", "stage-a");
    write_receipt(
        root.path(),
        &id,
        ForwardBackend::Companion,
        "private-job",
        Some("thread-a"),
    )?;
    let mut ledger = codex_ledger(&["thread-a"])?;

    ForwardJoin::load(Some(root.path())).join_ledger(&mut ledger);

    let value = serde_json::to_value(&ledger.rows[0])?;
    assert_eq!(value["forward_receipt"]["receipt_id"], id.receipt_id());
    assert_eq!(value["forward_receipt"]["backend"], "companion");
    assert_eq!(value["forward_receipt"]["state"], "running");
    assert!(!value.to_string().contains("private-job"));
    Ok(())
}

#[test]
fn codex_thread_shared_by_receipts_is_ambiguous() -> Result<()> {
    let root = tempfile::tempdir()?;
    write_receipt(
        root.path(),
        &identity("agent-a", "tool-a", "stage-a"),
        ForwardBackend::Companion,
        "job-a",
        Some("same-thread"),
    )?;
    write_receipt(
        root.path(),
        &identity("agent-b", "tool-b", "stage-b"),
        ForwardBackend::Companion,
        "job-b",
        Some("same-thread"),
    )?;
    let mut ledger = codex_ledger(&["same-thread"])?;

    ForwardJoin::load(Some(root.path())).join_ledger(&mut ledger);

    assert!(ledger.rows[0].forward_receipt.is_none());
    assert_eq!(
        ledger.providers[0].diagnostics.forward_identity_ambiguous,
        1
    );
    Ok(())
}

#[test]
fn codex_absent_or_conflicting_thread_is_diagnostic() -> Result<()> {
    let root = tempfile::tempdir()?;
    let join = ForwardJoin::load(Some(root.path()));
    let mut absent = codex_ledger(&[])?;
    let mut conflicting = codex_ledger(&["thread-a", "thread-b"])?;

    join.join_ledger(&mut absent);
    join.join_ledger(&mut conflicting);

    assert_eq!(absent.providers[0].diagnostics.forward_identity_absent, 1);
    assert_eq!(
        conflicting.providers[0]
            .diagnostics
            .forward_identity_conflicts,
        1
    );
    Ok(())
}

#[test]
fn joining_changes_no_tokens_counts_or_usage_receipts() -> Result<()> {
    let root = tempfile::tempdir()?;
    let id = identity("agent-1", "tool-1", "stage-a");
    write_receipt(root.path(), &id, ForwardBackend::Direct, "thread-a", None)?;
    let mut ledger = codex_ledger(&["thread-a"])?;
    let mut usage_receipt = ledger.rows[0].clone();
    usage_receipt.source_kind = SourceKind::ExecutionReceipt;
    usage_receipt.codex_thread_id = None;
    ledger.rows.push(usage_receipt);
    let receipt_before = serde_json::to_vec(&ledger.rows[1])?;
    let row_count = ledger.rows.len();
    let before = serde_json::to_value((
        &ledger.rows[0].tokens,
        &ledger.providers[0].coverage,
        &ledger.providers[0].totals,
    ))?;

    ForwardJoin::load(Some(root.path())).join_ledger(&mut ledger);

    let after = serde_json::to_value((
        &ledger.rows[0].tokens,
        &ledger.providers[0].coverage,
        &ledger.providers[0].totals,
    ))?;
    assert_eq!(before, after);
    assert_eq!(receipt_before, serde_json::to_vec(&ledger.rows[1])?);
    assert_eq!(row_count, ledger.rows.len());
    Ok(())
}
