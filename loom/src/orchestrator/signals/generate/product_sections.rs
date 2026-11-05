//! The integration-verify signal's `## Product Sections` block: the product sections the plan's
//! `Implements:` line names, and the duty to check the build against them.
//!
//! It lives in the dynamic suffix, emitted only when the plan names sections: the duty has
//! nothing to act on otherwise.

use std::path::{Path, PathBuf};

use crate::fs::plan_review::SPEC_CONFORMANCE_PREFIX;
use crate::models::stage::{Stage, StageType};
use crate::planning::implements::implemented_section_paths;

/// Append the block for an integration-verify stage whose plan implements product sections.
pub(super) fn append_product_sections(content: &mut String, stage: &Stage, work_dir: &Path) {
    if !matches!(stage.stage_type, StageType::IntegrationVerify) {
        return;
    }
    if let Some(block) = product_sections_block(&plan_section_paths(work_dir)) {
        super::super::helpers::ensure_trailing_newline(content);
        content.push('\n');
        content.push_str(&block);
    }
}

/// The section paths the active plan implements; empty when there are none or the plan cannot
/// be read. A read failure never fails signal generation.
fn plan_section_paths(work_dir: &Path) -> Vec<PathBuf> {
    let rel = match crate::fs::get_source_path(work_dir) {
        Ok(Some(rel)) if rel.is_relative() => rel,
        Ok(_) => return Vec::new(),
        Err(error) => {
            tracing::warn!("product sections: cannot read the plan source path: {error:#}");
            return Vec::new();
        }
    };
    let path = match crate::fs::resolve_source_path(work_dir) {
        Ok(Some(path)) => path,
        Ok(None) => return Vec::new(),
        Err(error) => {
            tracing::warn!("product sections: cannot resolve the plan source: {error:#}");
            return Vec::new();
        }
    };
    match std::fs::read_to_string(&path) {
        Ok(text) => implemented_section_paths(&rel, &text),
        Err(error) => {
            tracing::warn!("product sections: cannot read {}: {error}", path.display());
            Vec::new()
        }
    }
}

/// Render the block, or `None` when the plan names no sections.
fn product_sections_block(paths: &[PathBuf]) -> Option<String> {
    if paths.is_empty() {
        return None;
    }
    let mut block = String::from(
        "## Product Sections\n\n\
         This plan implements these product sections. Paths are relative to the worktree root.\n\n",
    );
    for path in paths {
        block.push_str(&format!("- `{}`\n", path.display()));
    }
    let tag = SPEC_CONFORMANCE_PREFIX;
    block.push_str(&format!(
        "\n**Spec conformance.** Read every section above. For each behaviour a section states \
         that the build lacks or contradicts, fix the code. When the build is right and the \
         section is wrong, edit the section to match the build and record \
         `loom memory decision \"{tag} <section path>: <what changed and why>\"`. A gap you \
         neither fix nor correct becomes `loom memory question \"{tag} <section path>: <the \
         gap>\"`.\n"
    ));
    Some(block)
}

#[cfg(test)]
mod tests {
    use super::*;

    const HEADING: &str = "## Product Sections";

    fn stage(stage_type: StageType) -> Stage {
        Stage {
            id: "integration-verify".to_string(),
            stage_type,
            ..Stage::default()
        }
    }

    #[test]
    fn the_block_lists_each_path_and_the_tagged_duty() {
        let paths = [PathBuf::from("doc/product/alpha/section_02.md")];
        let block = product_sections_block(&paths).unwrap();
        assert!(block.starts_with(&format!("{HEADING}\n\n")), "{block}");
        assert!(block.contains("\n- `doc/product/alpha/section_02.md`\n"));
        let decision = "`loom memory decision \"spec-conformance: <section path>: <what changed";
        let question = "`loom memory question \"spec-conformance: <section path>: <the gap>\"`.";
        assert!(block.contains(decision), "{block}");
        assert!(block.ends_with(&format!("{question}\n")), "{block}");
        assert!(product_sections_block(&[]).is_none());
    }

    #[test]
    fn only_an_integration_verify_stage_with_a_feature_plan_gets_the_block() {
        let root = tempfile::tempdir().unwrap();
        let work = root.path().join(".loom/work");
        std::fs::create_dir_all(&work).unwrap();
        let plan = "doc/plans/alpha/IN_PROGRESS-PLAN-x.md";
        let config = format!("[plan]\nplan_id = \"PLAN-x\"\nsource_path = \"{plan}\"\n");
        std::fs::write(work.join("config.toml"), config).unwrap();
        std::fs::create_dir_all(root.path().join("doc/plans/alpha")).unwrap();
        let text = "# X\n\nImplements: section_03, section_01\n";
        std::fs::write(root.path().join(plan), text).unwrap();

        let mut content = String::from("signal");
        let verify = stage(StageType::IntegrationVerify);
        append_product_sections(&mut content, &verify, &work);
        let expected = "- `doc/product/alpha/section_03.md`\n- `doc/product/alpha/section_01.md`\n";
        assert!(
            content.starts_with(&format!("signal\n\n{HEADING}\n")),
            "{content}"
        );
        assert!(content.contains(expected), "{content}");

        let mut standard = String::new();
        append_product_sections(&mut standard, &stage(StageType::Standard), &work);
        assert!(standard.is_empty(), "{standard}");
    }

    #[test]
    fn nothing_without_config() {
        let work = tempfile::tempdir().unwrap();
        let mut content = String::new();
        let verify = stage(StageType::IntegrationVerify);
        append_product_sections(&mut content, &verify, work.path());
        assert!(content.is_empty(), "{content}");
    }
}
