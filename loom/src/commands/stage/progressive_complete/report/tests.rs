use super::*;

fn blocked(block: MergeBlock) -> Stage {
    let mut stage = Stage {
        id: "s".to_string(),
        ..Stage::default()
    };
    stage.merge.block = Some(block);
    stage
}

#[test]
fn a_held_quarantine_keeps_the_recovery_remedy_instead_of_retry() {
    let dir = tempfile::tempdir().unwrap();
    let stage = blocked(MergeBlock::CheckoutRestoreIncomplete {
        quarantine: dir.path().to_path_buf(),
        failures: vec![],
        detail: None,
    });

    let text = blocked_exit(&stage);

    assert!(text.contains("Recover the quarantined files"), "{text}");
    assert!(text.contains("do not run loom stage retry s"), "{text}");
    assert!(!text.contains("run: loom stage retry"), "{text}");
}

#[test]
fn other_blocks_name_loom_stage_retry() {
    let dir = tempfile::tempdir().unwrap();
    for block in [
        MergeBlock::TargetMoved,
        MergeBlock::CheckoutRestoreIncomplete {
            quarantine: dir.path().join("gone"),
            failures: vec![],
            detail: None,
        },
    ] {
        let text = blocked_exit(&blocked(block));
        assert!(
            text.ends_with("Fix the issue and run: loom stage retry s"),
            "{text}"
        );
    }
}

const MOVE_BACK: &str =
    "Move each file in it back to its path in the checkout, delete the directory";

#[test]
fn a_block_that_states_its_remedy_omits_the_delete_the_directory_line() {
    let dir = tempfile::tempdir().unwrap();
    let block = MergeBlock::CheckoutRestoreIncomplete {
        quarantine: dir.path().to_path_buf(),
        failures: vec![],
        detail: Some("move each back, then retry".to_string()),
    };
    assert!(block.states_its_remedy());

    let lines = merge_block_lines("s", &block);

    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(
        lines[1].contains("No merge of this stage runs while"),
        "{lines:?}"
    );
    assert!(lines.iter().all(|l| !l.contains(MOVE_BACK)), "{lines:?}");
}

#[test]
fn a_block_with_failures_prints_the_move_back_and_delete_line() {
    let dir = tempfile::tempdir().unwrap();
    let block = MergeBlock::CheckoutRestoreIncomplete {
        quarantine: dir.path().to_path_buf(),
        // `RestoreFailure` is not exported from `git::merge`; build it from its wire form.
        failures: serde_json::from_str(r#"[{"path":"a.txt","reason":"destination occupied"}]"#)
            .unwrap(),
        detail: None,
    };
    assert!(!block.states_its_remedy());

    let lines = merge_block_lines("s", &block);

    assert_eq!(lines.len(), 3, "{lines:?}");
    assert!(lines[2].contains(MOVE_BACK), "{lines:?}");
    assert!(lines[2].contains("loom stage merge s"), "{lines:?}");
}

#[test]
fn other_blocks_print_the_daemon_retry_line() {
    let lines = merge_block_lines("s", &MergeBlock::TargetMoved);

    assert_eq!(lines.len(), 2, "{lines:?}");
    assert!(lines[1].contains("A running daemon retries"), "{lines:?}");
}
