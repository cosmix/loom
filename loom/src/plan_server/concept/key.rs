//! Key chips: the documents of an overview scope in reading order, with their colours.

use crate::plan_server::concept::page::{page_key_of, PageContext, PageKey, Scope};
use crate::plan_server::wire::concept::{DocLink, KeyChip};
use crate::planning::project::Project;
use crate::planning::summary::{evaluate::Evaluation, store};

/// The ordered key chips for one feature overview.
pub(crate) fn key_chips(
    project: &Project,
    eval: &Evaluation,
    feature: &str,
    scope: Scope,
    ctx: &PageContext,
) -> Vec<KeyChip> {
    let members = store::scope(project, feature);
    let ids = match scope {
        Scope::Product => &members.product,
        Scope::Engineering | Scope::Metaplan => &members.engineering,
    };
    let mut chips: Vec<KeyChip> =
        ids.iter()
            .filter_map(|id| {
                let path = store::source_of(project, id)?;
                let doc = project.docs.get(&path)?;
                let (label, page, no_plan) = chip_label(project, &members, scope, id)?;
                // A page key the web client would reject (a slug past 128 characters) drops the
                // chip instead of failing every page that embeds the key row.
                PageKey::parse(&page)?;
                Some(KeyChip {
                    doc: DocLink {
                        id: id.clone(),
                        title: doc.title.clone(),
                        path: path.to_string_lossy().into_owned(),
                        page,
                    },
                    // Placeholder; the colour is set by position below, once the list is filtered.
                    colour: 0,
                    label,
                    out_of_date: eval.summaries.get(id).and_then(|summary| {
                        super::state::out_of_date_reason(id, summary.state, ctx)
                    }),
                    no_plan,
                })
            })
            .collect();
    // Colours cycle 1..8 over the chips that remain, so a dropped member never skips a colour.
    for (index, chip) in chips.iter_mut().enumerate() {
        chip.colour = (index % 8 + 1) as u8;
    }
    chips
}

/// A chip's label, page key and "no plan" marker; `None` for an id outside the scope's form.
fn chip_label(
    project: &Project,
    members: &store::Scope,
    scope: Scope,
    id: &str,
) -> Option<(String, String, bool)> {
    match scope {
        Scope::Product => {
            let prefix = format!("{}/product/section_", members.feature);
            let number = id.strip_prefix(prefix.as_str())?;
            let feature = project.feature(&members.feature)?;
            let no_plan = !members.engineering.is_empty()
                && number
                    .parse::<u32>()
                    .ok()
                    .is_some_and(|n| !feature.implemented_by.contains_key(&n));
            Some((number.to_string(), id.to_owned(), no_plan))
        }
        Scope::Engineering | Scope::Metaplan => {
            let slug = id.strip_prefix("plans/")?;
            Some((slug.to_string(), page_key_of(project, id)?, false))
        }
    }
}

/// The colour assigned to a document in this key row.
pub(crate) fn colour_of(chips: &[KeyChip], doc_path: &str) -> Option<u8> {
    chips
        .iter()
        .find(|chip| chip.doc.path == doc_path)
        .map(|chip| chip.colour)
}
