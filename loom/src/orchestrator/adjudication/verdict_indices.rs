//! Which entries an accepted criterion verdict may change: the ones its
//! dispute filed. A replace or delete of any other entry would amend a
//! criterion nobody disputed (an accepted dispute of artifact 0 deleting
//! artifact 1); an insert removes nothing, so it may name any index.

use crate::models::dispute::{CriterionField, DisputeVerdict};
use crate::plan::amendment::AmendmentPatch;

use super::plan_patch;
use super::verdict::ValidationOutcome;

/// The patches of an accept that amend the disputed entry's own list: the only
/// patches the filed indices bind. A patch to another list the dispute admits
/// (an acceptance dispute amending `wiring`) indexes that list, so it is not
/// checked against them. `None` for any other outcome.
pub(super) fn disputed_list_patches(
    outcome: &ValidationOutcome,
    disputed: CriterionField,
) -> Option<Vec<AmendmentPatch>> {
    let ValidationOutcome::Verdict(DisputeVerdict::Accept { plan_patch, .. }) = outcome else {
        return None;
    };
    let (field, patches, _) = plan_patch::normalize(&plan_patch.inner).ok()?;
    (field == disputed.amendment_field()).then_some(patches)
}

/// The index of the first replace or delete patch of `patches` that names an
/// entry not in `filed`; `None` when every one names a filed entry. An insert
/// is never unfiled.
pub(super) fn unfiled_patch(patches: &[AmendmentPatch], filed: &[usize]) -> Option<usize> {
    patches
        .iter()
        .filter(|patch| !matches!(patch, AmendmentPatch::Insert { .. }))
        .map(AmendmentPatch::index)
        .find(|index| !filed.contains(index))
}

/// The question for an accept that patches `index`, which the dispute did not
/// file: it names the filed indices.
pub(super) fn unfiled_question(index: usize, filed: &[usize]) -> String {
    let list: Vec<String> = filed.iter().map(usize::to_string).collect();
    format!(
        "Accept verdict 'plan_patch' patches index {index}, which this dispute did not file; \
         replace or delete only the filed indices {}, or insert a new entry",
        list.join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn replace(index: usize) -> AmendmentPatch {
        AmendmentPatch::Replace {
            index,
            value: "x".to_string(),
        }
    }

    #[test]
    fn the_first_replace_or_delete_outside_the_filed_set_is_named() {
        let patches = [replace(0), AmendmentPatch::Delete { index: 3 }, replace(5)];
        assert_eq!(unfiled_patch(&patches, &[0, 1]), Some(3));
        assert_eq!(unfiled_patch(&patches, &[0, 3, 5]), None);
        assert_eq!(unfiled_patch(&[], &[0]), None);
    }

    #[test]
    fn an_insert_is_never_unfiled() {
        let insert = AmendmentPatch::Insert {
            index: 9,
            value: "x".to_string(),
        };
        assert_eq!(unfiled_patch(&[insert], &[0]), None);
    }
}
