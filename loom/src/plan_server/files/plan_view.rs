//! Data for the plan panel of a document.

use std::collections::BTreeMap;
use std::fs::File;
use std::os::fd::AsRawFd;
use std::path::Path;

use chrono::{DateTime, SecondsFormat, Utc};

use crate::fs::safe_fs::{open_safely, safe_open_dirfd};
use crate::plan::parser::extract_yaml_metadata;
use crate::plan::schema::{detect_stage_type, LoomMetadata, StageDefinition, StageType};
use crate::plan_server::wire::files::{
    ImplementsLink, PlanStateView, PlanView, StageRow, ValidationView,
};
use crate::planning::discover::DocTree;
use crate::planning::implements::{self, Implements};
use crate::planning::kinds::DocKind;
use crate::planning::pressure;
use crate::planning::sections::DocSections;
use crate::planning::slug::{plan_slug, plan_state, PlanState};

/// Build a plan panel from the same metadata parser used by plan verification.
/// `product_sections` (from [`product_sections`]) resolves the `Implements:` links.
pub fn plan_view(
    root: &Path,
    rel: &Path,
    text: &str,
    sections: &DocSections,
    product_sections: &BTreeMap<u32, String>,
    validation: Option<ValidationView>,
) -> PlanView {
    let slug = rel
        .file_name()
        .and_then(|name| name.to_str())
        .and_then(plan_slug)
        .unwrap_or_default();
    let state = match plan_state(rel).unwrap_or(PlanState::Written) {
        PlanState::Written => PlanStateView::Written,
        PlanState::Running => PlanStateView::Running,
        PlanState::Done => PlanStateView::Done,
        PlanState::Retired => PlanStateView::Retired,
    };
    let yaml = extract_yaml_metadata(text).ok();
    let (stages, yaml_error) = parse_stages(yaml.as_deref(), sections);
    PlanView {
        slug,
        state,
        implements: implements_links(text, product_sections),
        pressure_date: latest_pressure_date(text),
        last_change: last_change(root, rel),
        validation,
        stages,
        yaml,
        yaml_error,
        metadata_line: sections
            .metadata
            .as_ref()
            .map(|span| (span.start_line + 1) as u32),
    }
}

fn parse_stages(yaml: Option<&str>, sections: &DocSections) -> (Vec<StageRow>, Option<String>) {
    match yaml.map(crate::yaml::from_str::<LoomMetadata>) {
        Some(Ok(metadata)) => (
            metadata
                .loom
                .stages
                .iter()
                .map(|stage| stage_row(stage, sections))
                .collect(),
            None,
        ),
        Some(Err(error)) => (Vec::new(), Some(format!("YAML parse error: {error}"))),
        None => (Vec::new(), None),
    }
}

fn stage_row(stage: &StageDefinition, sections: &DocSections) -> StageRow {
    let stage_type = match detect_stage_type(stage) {
        StageType::Standard => "standard",
        StageType::Knowledge => "knowledge",
        StageType::IntegrationVerify => "integration-verify",
        StageType::KnowledgeDistill => "knowledge-distill",
    };
    StageRow {
        id: stage.id.clone(),
        name: stage.name.clone(),
        stage_type: stage_type.into(),
        summary: stage.summary.clone(),
        dependencies: stage.dependencies.clone(),
        description: stage.description.clone().unwrap_or_default(),
        acceptance: stage
            .acceptance
            .iter()
            .map(|criterion| criterion.command().to_owned())
            .collect(),
        files: stage.files.clone(),
        line: sections
            .stages
            .iter()
            .find(|(id, _)| id == &stage.id)
            .map(|(_, span)| (span.start_line + 1) as u32),
    }
}

/// The latest date among the plan's pressure-test records; `pressure::records` keeps document
/// order, which need not be date order. The tree row and the plan panel both show this date.
pub(crate) fn latest_pressure_date(text: &str) -> Option<String> {
    pressure::records(text)
        .into_iter()
        .map(|record| record.date)
        .max()
}

/// Product section paths in the plan's feature, by number. Standalone plans have no links;
/// of two files with one number in a feature, the first discovered wins.
pub fn product_sections(tree: &DocTree, feature: Option<&str>) -> BTreeMap<u32, String> {
    tree.docs
        .iter()
        .rev()
        .filter_map(|doc| match &doc.kind {
            DocKind::ProductSection {
                number,
                feature: owner,
            } if feature.is_some() && owner.as_deref() == feature => {
                Some((*number, doc.path.to_string_lossy().into_owned()))
            }
            _ => None,
        })
        .collect()
}

fn implements_links(text: &str, product_sections: &BTreeMap<u32, String>) -> Vec<ImplementsLink> {
    let Implements::Sections(numbers) = implements::parse(text).value else {
        return Vec::new();
    };
    numbers
        .into_iter()
        .map(|number| ImplementsLink {
            number,
            path: product_sections.get(&number).cloned(),
        })
        .collect()
}

fn last_change(root: &Path, rel: &Path) -> Option<String> {
    let root_fd = safe_open_dirfd(root).ok()?;
    let fd = open_safely(
        root_fd.as_raw_fd(),
        rel,
        libc::O_RDONLY | libc::O_NONBLOCK,
        0,
    )
    .ok()?;
    let metadata = File::from(fd).metadata().ok()?;
    if !metadata.is_file() {
        return None;
    }
    let time = DateTime::<Utc>::from(metadata.modified().ok()?);
    Some(time.to_rfc3339_opts(SecondsFormat::Secs, true))
}
