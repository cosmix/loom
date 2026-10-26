//! The checklist's items: one per expected document, unlisted document and untested plan.

use std::path::{Path, PathBuf};

use super::{offers, ChecklistItem, Facts, ItemStatus, Offer, Step, VISION};

impl Facts<'_> {
    /// Every item in step order.
    pub(super) fn items(&self) -> Vec<ChecklistItem> {
        let mut items = vec![self.vision_item()];
        items.extend(self.section_items());
        items.extend(self.plan_items());
        items.extend(self.untested.iter().map(|plan| {
            let label = format!("plan {} has no pressure-test record", plan.slug);
            let offer = Some(Offer::PressureTest {
                slug: plan.slug.clone(),
            });
            let path = Some(plan.path.clone());
            item(Step::PressureTest, ItemStatus::Untested, path, label, offer)
        }));
        items
    }

    fn vision_item(&self) -> ChecklistItem {
        let status = if self.vision {
            ItemStatus::Present
        } else {
            ItemStatus::Gap
        };
        let offer = (!self.vision).then_some(Offer::StartVision);
        let path = Some(PathBuf::from(VISION));
        item(
            Step::Vision,
            status,
            path,
            "product vision".to_string(),
            offer,
        )
    }

    fn section_items(&self) -> Vec<ChecklistItem> {
        let section = |status, path: PathBuf, label, offer| {
            item(Step::ProductSpec, status, Some(path), label, offer)
        };
        let mut items = Vec::new();
        match self.rows {
            Some(rows) => items.extend(rows.iter().map(|row| {
                let number = row.number;
                let label = format!("section {number:02}: {}", row.name);
                if self.missing_sections.contains(&number) {
                    let offer = Some(Offer::WriteSection { number });
                    section(ItemStatus::Gap, offers::section_path(number), label, offer)
                } else {
                    section(
                        ItemStatus::Present,
                        offers::section_path(number),
                        label,
                        None,
                    )
                }
            })),
            None if self.vision => {
                let label = "the vision lists no sections in a form the app can read".to_string();
                let offer = Some(Offer::FixVisionTable);
                items.push(section(
                    ItemStatus::Gap,
                    PathBuf::from(VISION),
                    label,
                    offer,
                ));
            }
            None => {}
        }
        for &(number, path) in &self.unlisted_sections {
            let label = format!("section {number:02} is not in the vision's table");
            let offer = self.vision.then_some(Offer::AddSectionToVision { number });
            items.push(section(
                ItemStatus::Unlisted,
                path.to_path_buf(),
                label,
                offer,
            ));
        }
        items
    }

    fn plan_items(&self) -> Vec<ChecklistItem> {
        let metaplans = self.parsed.metaplans.iter().map(|(path, _)| {
            let label = format!("metaplan {}", path.display());
            item(
                Step::Metaplan,
                ItemStatus::Present,
                Some(path.clone()),
                label,
                None,
            )
        });
        let mut items: Vec<ChecklistItem> = metaplans.collect();
        if !self.has_metaplan() {
            let offer = Some(Offer::StartMetaplan);
            let label = "no metaplan".to_string();
            items.push(item(Step::Metaplan, ItemStatus::Gap, None, label, offer));
        }
        items.extend(
            self.entries
                .iter()
                .map(|&(metaplan, slug)| self.entry_item(metaplan, slug)),
        );
        for plan in &self.unlisted_plans {
            let label = format!("plan {} is not in a metaplan", plan.slug);
            let slug = plan.slug.clone();
            let offer = self
                .has_metaplan()
                .then_some(Offer::AddPlanToMetaplan { slug });
            let path = Some(plan.path.clone());
            items.push(item(Step::Plans, ItemStatus::Unlisted, path, label, offer));
        }
        items
    }

    /// The item for one metaplan entry: the plan when written, else an offer to write it.
    fn entry_item(&self, metaplan: &Path, slug: &str) -> ChecklistItem {
        match self.tree.plan(slug) {
            Some(plan) => {
                let path = Some(plan.path.clone());
                item(
                    Step::Plans,
                    ItemStatus::Present,
                    path,
                    format!("plan {slug}"),
                    None,
                )
            }
            None => {
                let named_in = metaplan.display();
                let label = format!("plan {slug} named in {named_in} is not written");
                let offer = Offer::WritePlan {
                    metaplan: metaplan.to_path_buf(),
                    slug: slug.to_string(),
                };
                let path = Some(offers::plan_path(slug));
                item(Step::Plans, ItemStatus::Gap, path, label, Some(offer))
            }
        }
    }
}

fn item(
    step: Step,
    status: ItemStatus,
    path: Option<PathBuf>,
    label: String,
    offer: Option<Offer>,
) -> ChecklistItem {
    ChecklistItem {
        step,
        status,
        path,
        label,
        offer,
    }
}
