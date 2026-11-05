//! Offers: what the app proposes next, their stable ids and targets, and which one is primary.

use std::collections::BTreeSet;
use std::path::PathBuf;

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

use super::{CurrentStep, Facts, Step};
use crate::planning::discover::DocTree;
use crate::planning::kinds::{plans_dir, product_dir, vision_path, DocKind};

/// One action the app proposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offer {
    /// Write the product vision.
    StartVision {
        /// The owning feature.
        feature: String,
    },
    /// Make the vision's `Specification sections` table readable.
    FixVisionTable {
        /// The owning feature.
        feature: String,
    },
    /// Write a section the vision lists.
    WriteSection {
        /// The owning feature.
        feature: String,
        /// The section number.
        number: u32,
    },
    /// List a section file in the vision's table.
    AddSectionToVision {
        /// The owning feature.
        feature: String,
        /// The section number.
        number: u32,
    },
    /// Write the first metaplan.
    StartMetaplan {
        /// The owning feature.
        feature: String,
    },
    /// Write a plan a metaplan names.
    WritePlan {
        /// The owning feature.
        feature: String,
        /// The metaplan naming it.
        metaplan: PathBuf,
        /// The plan's slug.
        slug: String,
    },
    /// Pressure-test a written plan.
    PressureTest {
        /// The owning feature.
        feature: String,
        /// The plan's slug.
        slug: String,
    },
    /// Run the plans.
    StartRun,
    /// Plan follow-up entries in the feature's metaplan.
    PlanFollowUps,
    /// Draft every missing section in one authoring session.
    DraftAllSections {
        /// The owning feature.
        feature: String,
    },
    /// Draft every missing metaplan plan in one authoring session.
    DraftAllPlans {
        /// The owning feature.
        feature: String,
    },
    /// Move a misplaced top-level document into a feature.
    MoveIntoFeature {
        /// The repository-relative document path.
        path: PathBuf,
    },
}

impl Offer {
    /// The stable id: the kebab-case kind, then `:<nn>` (two digits) for a section offer or
    /// `:<slug>` for a plan offer, or `:<path>` for a move offer.
    pub fn id(&self) -> String {
        match self {
            Offer::WriteSection { number, .. } | Offer::AddSectionToVision { number, .. } => {
                format!("{}:{number:02}", self.kind())
            }
            Offer::WritePlan { slug, .. } | Offer::PressureTest { slug, .. } => {
                format!("{}:{slug}", self.kind())
            }
            Offer::MoveIntoFeature { path } => format!("{}:{}", self.kind(), path.display()),
            _ => self.kind().to_string(),
        }
    }

    /// The feature's section, plan or vision document, or the document being moved.
    /// Offers that start a session without a single document target return `None`.
    pub fn target(&self) -> Option<PathBuf> {
        match self {
            Offer::WriteSection { feature, number }
            | Offer::AddSectionToVision { feature, number } => {
                Some(product_dir(feature).join(format!("section_{number:02}.md")))
            }
            Offer::WritePlan { feature, slug, .. } | Offer::PressureTest { feature, slug } => {
                Some(plans_dir(Some(feature)).join(format!("PLAN-{slug}.md")))
            }
            Offer::FixVisionTable { feature } => Some(vision_path(feature)),
            Offer::MoveIntoFeature { path } => Some(path.clone()),
            _ => None,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Offer::StartVision { .. } => "start-vision",
            Offer::FixVisionTable { .. } => "fix-vision-table",
            Offer::WriteSection { .. } => "write-section",
            Offer::AddSectionToVision { .. } => "add-section-to-vision",
            Offer::StartMetaplan { .. } => "start-metaplan",
            Offer::WritePlan { .. } => "write-plan",
            Offer::PressureTest { .. } => "pressure-test",
            Offer::StartRun => "start-run",
            Offer::PlanFollowUps => "plan-follow-ups",
            Offer::DraftAllSections { .. } => "draft-all-sections",
            Offer::DraftAllPlans { .. } => "draft-all-plans",
            Offer::MoveIntoFeature { .. } => "move-into-feature",
        }
    }

    /// The owning feature, when the offer is scoped to one.
    pub fn feature(&self) -> Option<&str> {
        match self {
            Offer::StartVision { feature }
            | Offer::FixVisionTable { feature }
            | Offer::WriteSection { feature, .. }
            | Offer::AddSectionToVision { feature, .. }
            | Offer::StartMetaplan { feature }
            | Offer::WritePlan { feature, .. }
            | Offer::PressureTest { feature, .. }
            | Offer::DraftAllSections { feature }
            | Offer::DraftAllPlans { feature } => Some(feature),
            _ => None,
        }
    }

    /// The human-readable action shown by planning clients and authoring sessions.
    pub fn label(&self) -> String {
        match self {
            Offer::StartVision { .. } => "Start the vision".into(),
            Offer::FixVisionTable { .. } => "Fix the section table".into(),
            Offer::WriteSection { number, .. } => format!("Write section {number:02}"),
            Offer::AddSectionToVision { number, .. } => {
                format!("Add section {number:02} to the vision")
            }
            Offer::StartMetaplan { .. } => "Start the metaplan".into(),
            Offer::WritePlan { slug, .. } => format!("Write plan {slug}"),
            Offer::PressureTest { slug, .. } => format!("Pressure-test {slug}"),
            Offer::StartRun => "Start a run".into(),
            Offer::PlanFollowUps => "Plan follow-ups".into(),
            Offer::DraftAllSections { .. } => "Draft all with a team".into(),
            Offer::DraftAllPlans { .. } => "Draft all with a team".into(),
            Offer::MoveIntoFeature { path } => {
                format!("Move {} into a feature", path.to_string_lossy())
            }
        }
    }
}

/// An object with `id`, `kind`, `target` and the variant's fields (`number`, `slug`,
/// `metaplan`, `feature`, `path`).
impl Serialize for Offer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id())?;
        map.serialize_entry("kind", self.kind())?;
        if let Some(feature) = self.feature() {
            map.serialize_entry("feature", feature)?;
        }
        match self {
            Offer::WriteSection { number, .. } | Offer::AddSectionToVision { number, .. } => {
                map.serialize_entry("number", number)?;
            }
            Offer::WritePlan { metaplan, slug, .. } => {
                map.serialize_entry("metaplan", metaplan)?;
                map.serialize_entry("slug", slug)?;
            }
            Offer::PressureTest { slug, .. } => {
                map.serialize_entry("slug", slug)?;
            }
            Offer::MoveIntoFeature { path } => map.serialize_entry("path", path)?,
            _ => {}
        }
        map.serialize_entry("target", &self.target())?;
        map.end()
    }
}

/// Offers to move top-level visions, product sections and metaplans, sorted by path.
pub fn move_offers(tree: &DocTree) -> Vec<Offer> {
    let mut paths: Vec<_> = tree
        .docs
        .iter()
        .filter(|doc| {
            matches!(
                &doc.kind,
                DocKind::Vision { feature: None }
                    | DocKind::ProductSection { feature: None, .. }
                    | DocKind::Metaplan { feature: None }
            )
        })
        .map(|doc| doc.path.clone())
        .collect();
    paths.sort();
    paths
        .into_iter()
        .map(|path| Offer::MoveIntoFeature { path })
        .collect()
}

/// The primary offer and the secondaries, each offer once (by id).
pub(super) fn choose(current: &CurrentStep, facts: &Facts) -> (Option<Offer>, Vec<Offer>) {
    let mut offers: Vec<Offer> = match current {
        CurrentStep::Step(Step::PressureTest) => {
            let mut tests = pressure_tests(facts).into_iter();
            let first = tests.next();
            first
                .into_iter()
                .chain([Offer::StartRun])
                .chain(tests)
                .collect()
        }
        CurrentStep::Step(_) => gap_offers(facts),
        CurrentStep::Ready => vec![Offer::StartRun],
        CurrentStep::ReadyAllDone => vec![Offer::PlanFollowUps],
    };
    offers.extend(extras(facts));
    let mut seen = BTreeSet::new();
    let mut unique = offers.into_iter().filter(|offer| seen.insert(offer.id()));
    let primary = if matches!(current, CurrentStep::ReadyAllDone) {
        None
    } else {
        unique.next()
    };
    (primary, unique.collect())
}

/// The offers of every gap of steps 1 to 4, in step order. The first is the current step's
/// primary: the current step is the earliest gap, and a gap with no offer of its own (step 2
/// without a vision, step 4 without a metaplan) always sits behind an earlier gap that has one.
fn gap_offers(facts: &Facts) -> Vec<Offer> {
    let mut offers = Vec::new();
    let feature = facts.feature;
    if !facts.vision {
        offers.push(Offer::StartVision {
            feature: feature.into(),
        });
    } else if facts.rows.is_none() {
        offers.push(Offer::FixVisionTable {
            feature: feature.into(),
        });
    }
    for (index, &number) in facts.missing_sections.iter().enumerate() {
        offers.push(Offer::WriteSection {
            feature: feature.into(),
            number,
        });
        if index == 0 && facts.missing_sections.len() >= 2 {
            offers.push(Offer::DraftAllSections {
                feature: feature.into(),
            });
        }
    }
    if !facts.has_metaplan() {
        offers.push(Offer::StartMetaplan {
            feature: feature.into(),
        });
    }
    for (index, &(metaplan, slug)) in facts.missing_plans.iter().enumerate() {
        offers.push(Offer::WritePlan {
            feature: feature.into(),
            metaplan: metaplan.to_path_buf(),
            slug: slug.to_string(),
        });
        if index == 0 && facts.missing_plans.len() >= 2 {
            offers.push(Offer::DraftAllPlans {
                feature: feature.into(),
            });
        }
    }
    offers
}

/// `PressureTest` for every untested written plan, in reading order.
fn pressure_tests(facts: &Facts) -> Vec<Offer> {
    let slugs = facts.untested.iter().map(|plan| plan.slug.clone());
    slugs
        .map(|slug| Offer::PressureTest {
            feature: facts.feature.into(),
            slug,
        })
        .collect()
}

/// What every state also offers: pressure tests and unlisted sections when a vision exists.
fn extras(facts: &Facts) -> Vec<Offer> {
    let mut offers = pressure_tests(facts);
    if facts.vision {
        let numbers = facts.unlisted_sections.iter().map(|&(number, _)| number);
        offers.extend(numbers.map(|number| Offer::AddSectionToVision {
            feature: facts.feature.into(),
            number,
        }));
    }
    offers
}
