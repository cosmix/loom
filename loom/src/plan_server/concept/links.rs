//! Implements links: a plan's product sections and a section's implementing plans.

use std::path::Path;

use crate::plan_server::wire::concept::{DocLink, ImplementsView, PlanLink, SectionLink};
use crate::planning::implements::Implements;
use crate::planning::project::Project;
use crate::planning::slug::PlanState;
use crate::planning::summary::store::summary_id_for;

pub(crate) fn implements_view(project: &Project, slug: &str) -> ImplementsView {
    let Some(line) = project.implements.get(slug) else {
        return ImplementsView::NotStated;
    };
    match &line.value {
        Implements::Unmapped => ImplementsView::NotStated,
        Implements::NoneDeclared => ImplementsView::Sections { links: Vec::new() },
        Implements::Sections(numbers) => {
            let mut links: Vec<_> = numbers.iter().map(|&n| section_link(project, n)).collect();
            links.extend(line.bad_tokens.iter().map(|token| SectionLink {
                number: None,
                name: token.clone(),
                doc: None,
            }));
            ImplementsView::Sections { links }
        }
    }
}

fn section_link(project: &Project, number: u32) -> SectionLink {
    let name = format!("section_{number:02}");
    let path = Path::new("doc/product").join(format!("{name}.md"));
    let doc = project.docs.get(&path).and_then(|document| {
        let id = summary_id_for(project, &path)?;
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

pub(crate) fn plan_state_name(state: PlanState) -> &'static str {
    match state {
        PlanState::Written => "written",
        PlanState::Running => "running",
        PlanState::Done => "done",
        PlanState::Retired => "retired",
    }
}

pub(crate) fn implemented_by(project: &Project, number: u32) -> Vec<PlanLink> {
    let mut links: Vec<_> = project
        .implemented_by
        .get(&number)
        .into_iter()
        .flatten()
        .filter_map(|slug| {
            let plan = project.tree.plan(slug)?;
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
