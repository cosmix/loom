//! The typed block that holds a stage's merge, as the sentence `loom status`
//! shows.

use crate::models::stage::{Stage, StageStatus};

/// The typed block's sentence, while the stage is `MergeBlocked` and carries one.
pub(super) fn merge_block_text(stage: &Stage) -> Option<String> {
    if stage.status != StageStatus::MergeBlocked {
        return None;
    }
    stage.merge.block.as_ref().map(ToString::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::git::MergeBlock;
    use crate::models::stage::MergeRecord;

    #[test]
    fn the_block_text_shows_only_while_the_stage_is_merge_blocked() {
        let mut stage = Stage {
            status: StageStatus::MergeBlocked,
            merge: MergeRecord {
                block: Some(MergeBlock::TargetMoved),
                ..MergeRecord::default()
            },
            ..Stage::default()
        };
        assert_eq!(
            merge_block_text(&stage),
            Some(MergeBlock::TargetMoved.to_string())
        );
        stage.status = StageStatus::Completed;
        assert_eq!(merge_block_text(&stage), None);
        stage.status = StageStatus::MergeBlocked;
        stage.merge.block = None;
        assert_eq!(merge_block_text(&stage), None);
    }
}
