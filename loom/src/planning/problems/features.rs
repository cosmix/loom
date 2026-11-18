//! Feature layout and feature-scoped vision problems.

use std::collections::BTreeSet;
use std::path::Path;

use super::{problem, unlocked, Problem};
use crate::planning::kinds::{feature_dir_name, is_feature_name, vision_path, DocKind};
use crate::planning::project::Project;
use crate::planning::vision::VisionProblem;

pub(super) fn collect(project: &Project, found: &mut Vec<Problem>) {
    layout_problems(project, found);
    vision_problems(project, found);
    section_problems(project, found);
}

fn layout_problems(project: &Project, found: &mut Vec<Problem>) {
    let mut bad_names = BTreeSet::new();
    for doc in &project.tree.docs {
        let needs_feature = matches!(
            doc.kind,
            DocKind::Vision { feature: None }
                | DocKind::ProductSection { feature: None, .. }
                | DocKind::Metaplan { feature: None }
        );
        if needs_feature && unlocked(project, &doc.path) {
            let message = "the document belongs in a feature directory; move it into \
                doc/product/<feature>/ or doc/plans/<feature>/";
            found.push(problem(
                "needs-feature",
                &doc.path,
                None,
                message.to_string(),
            ));
        }
        let Some(name) = feature_dir_name(&doc.path) else {
            continue;
        };
        if !is_feature_name(name) && bad_names.insert(name) {
            let base = if doc.path.starts_with("doc/product") {
                "doc/product"
            } else {
                "doc/plans"
            };
            let message = format!("directory `{name}` is not a valid feature name");
            found.push(problem(
                "bad-feature-name",
                &Path::new(base).join(name),
                None,
                message,
            ));
        }
    }
}

/// The numbers listed by this feature's vision (none when it cannot be read).
pub(super) fn listed_sections(project: &Project, feature: &str) -> BTreeSet<u32> {
    project
        .feature(feature)
        .into_iter()
        .flat_map(|feature| feature.vision_rows.iter().flatten())
        .map(|row| row.number)
        .collect()
}

fn vision_problems(project: &Project, found: &mut Vec<Problem>) {
    for feature in &project.features {
        let vision = vision_path(&feature.name);
        let Err(why) = &feature.vision_rows else {
            continue;
        };
        if !unlocked(project, &vision) {
            continue;
        }
        let line = match why {
            VisionProblem::NoHeading => None,
            VisionProblem::NoTable { heading_line }
            | VisionProblem::NoNumberedRows { heading_line } => Some(heading_line + 1),
        };
        let message = "the vision has no Specification sections table with a numbered row";
        found.push(problem(
            "vision-table-unreadable",
            &vision,
            line,
            message.to_string(),
        ));
    }
}

fn section_problems(project: &Project, found: &mut Vec<Problem>) {
    for feature in &project.features {
        let listed = listed_sections(project, &feature.name);
        for doc in project.tree.feature_docs(&feature.name) {
            let DocKind::ProductSection { number, .. } = &doc.kind else {
                continue;
            };
            if !listed.contains(number) && unlocked(project, &doc.path) {
                let message = format!("section {number:02} is not listed in the vision's table");
                found.push(problem("unlisted-section", &doc.path, None, message));
            }
        }
    }
}
