//! Feature-scoped fixtures using the planning summary test helpers.

pub(super) use crate::planning::summary::evaluate::fixtures::{
    fixture, load, metaplan, put, seal_document, seal_overview, section, vision, write_file,
    Fixture,
};

/// Its vision source.
pub(super) const VISION: &str = "doc/product/demo/index.md";
/// Its first section source.
pub(super) const S1: &str = "doc/product/demo/section_01.md";
/// Its second section source.
pub(super) const S2: &str = "doc/product/demo/section_02.md";
/// Its metaplan source.
pub(super) const META: &str = "doc/plans/demo/index.md";
/// Its first section summary.
pub(super) const ID1: &str = "demo/product/section_01";
/// Its second section summary.
pub(super) const ID2: &str = "demo/product/section_02";

/// A vision and two written sections.
pub(super) fn two_sections() -> Fixture {
    let (s1, s2) = (section(1, "Alpha.", "Beta."), section(2, "Alpha.", "Beta."));
    fixture(&[(VISION, &vision(&[1, 2])), (S1, &s1), (S2, &s2)])
}
