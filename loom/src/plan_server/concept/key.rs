//! Key chips: the documents of an overview scope in reading order, with their colours.

use crate::plan_server::concept::page::{page_key_of, PageContext, Scope};
use crate::plan_server::wire::concept::{DocLink, KeyChip};
use crate::planning::project::Project;
use crate::planning::summary::{evaluate::Evaluation, store};

pub(crate) fn key_chips(
    project: &Project,
    eval: &Evaluation,
    scope: Scope,
    ctx: &PageContext,
) -> Vec<KeyChip> {
    let ids = match scope {
        Scope::Product => &eval.scope.product,
        Scope::Engineering => &eval.scope.engineering,
    };
    ids.iter()
        .enumerate()
        .filter_map(|(index, id)| {
            let path = store::source_of(project, id)?;
            let doc = project.docs.get(&path)?;
            let (label, page, no_plan) = match scope {
                Scope::Product => {
                    let number = id.strip_prefix("product/section_")?;
                    let no_plan = !eval.scope.engineering.is_empty()
                        && number
                            .parse::<u32>()
                            .ok()
                            .is_some_and(|n| !project.implemented_by.contains_key(&n));
                    (number.to_string(), id.clone(), no_plan)
                }
                Scope::Engineering => {
                    let slug = id.strip_prefix("plans/")?;
                    (slug.to_string(), page_key_of(id), false)
                }
            };
            Some(KeyChip {
                doc: DocLink {
                    id: id.clone(),
                    title: doc.title.clone(),
                    path: path.to_string_lossy().into_owned(),
                    page,
                },
                colour: (index % 8 + 1) as u8,
                label,
                out_of_date: eval
                    .summaries
                    .get(id)
                    .and_then(|summary| super::state::out_of_date_reason(id, summary.state, ctx)),
                no_plan,
            })
        })
        .collect()
}

pub(crate) fn colour_of(chips: &[KeyChip], doc_path: &str) -> Option<u8> {
    chips
        .iter()
        .find(|chip| chip.doc.path == doc_path)
        .map(|chip| chip.colour)
}
