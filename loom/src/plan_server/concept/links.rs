//! Implements links: a plan's product sections and a section's implementing plans.

use crate::plan_server::concept::page::{page_key_of, PageKey};
use crate::plan_server::wire::concept::{DocLink, ImplementsView, PlanLink, SectionLink};
use crate::planning::implements::Implements;
use crate::planning::kinds::product_dir;
use crate::planning::project::Project;
use crate::planning::slug::PlanState;
use crate::planning::summary::store::summary_id_for;

/// The product sections named by a plan, resolved within its feature.
pub(crate) fn implements_view(project: &Project, slug: &str) -> ImplementsView {
    let Some(line) = project.implements.get(slug) else {
        return ImplementsView::NotStated;
    };
    let feature = project
        .tree
        .plan(slug)
        .and_then(|plan| plan.feature.as_deref());
    match &line.value {
        Implements::Unmapped => ImplementsView::NotStated,
        Implements::NoneDeclared => ImplementsView::Sections { links: Vec::new() },
        Implements::Sections(numbers) => {
            let mut links: Vec<_> = numbers
                .iter()
                .map(|&n| section_link(project, feature, n))
                .collect();
            links.extend(line.bad_tokens.iter().map(|token| SectionLink {
                number: None,
                name: token.clone(),
                doc: None,
            }));
            ImplementsView::Sections { links }
        }
    }
}

fn section_link(project: &Project, feature: Option<&str>, number: u32) -> SectionLink {
    let name = format!("section_{number:02}");
    let path = feature.map(|feature| product_dir(feature).join(format!("{name}.md")));
    let doc = path.as_ref().and_then(|path| {
        let document = project.docs.get(path)?;
        let id = summary_id_for(project, path)?;
        Some(DocLink {
            page: id.clone(),
            id,
            title: document.title.clone(),
            path: path.to_string_lossy().into_owned(),
        })
    });
    SectionLink {
        number: Some(number),
        name: doc.as_ref().map_or(name, |doc| doc.title.clone()),
        doc,
    }
}

/// The wire spelling of a plan lifecycle state.
pub(crate) fn plan_state_name(state: PlanState) -> &'static str {
    match state {
        PlanState::Written => "written",
        PlanState::Running => "running",
        PlanState::Done => "done",
        PlanState::Retired => "retired",
    }
}

/// The plans implementing section `number` that have a concept page: a plan with no summary id
/// (an archived plan no metaplan names) or whose slug is not a valid page key is left out, so
/// every link resolves.
pub(crate) fn implemented_by(project: &Project, feature: &str, number: u32) -> Vec<PlanLink> {
    let mut links: Vec<_> = project
        .feature(feature)
        .and_then(|feature| feature.implemented_by.get(&number))
        .into_iter()
        .flatten()
        .filter_map(|slug| {
            let plan = project.tree.plan(slug)?;
            let id = summary_id_for(project, &plan.path)?;
            PageKey::parse(&page_key_of(project, &id)?)?;
            let document = project.docs.get(&plan.path)?;
            Some(PlanLink {
                slug: slug.clone(),
                title: document.title.clone(),
                state: plan_state_name(plan.state).to_owned(),
            })
        })
        .collect();
    links.sort_by(|a, b| a.slug.cmp(&b.slug));
    links
}
