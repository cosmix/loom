//! Offers: what the app proposes next, their stable ids and targets, and which one is primary.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use serde::ser::SerializeMap;
use serde::{Serialize, Serializer};

use super::{CurrentStep, Facts, Step, VISION};

/// One action the app proposes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Offer {
    /// Write the product vision.
    StartVision,
    /// Make the vision's `Specification sections` table readable.
    FixVisionTable,
    /// Write a section the vision lists.
    WriteSection {
        /// The section number.
        number: u32,
    },
    /// List a section file in the vision's table.
    AddSectionToVision {
        /// The section number.
        number: u32,
    },
    /// Write the first metaplan.
    StartMetaplan,
    /// Write a plan a metaplan names.
    WritePlan {
        /// The metaplan naming it.
        metaplan: PathBuf,
        /// The plan's slug.
        slug: String,
    },
    /// Name an unlisted plan in a metaplan.
    AddPlanToMetaplan {
        /// The plan's slug.
        slug: String,
    },
    /// Pressure-test a written plan.
    PressureTest {
        /// The plan's slug.
        slug: String,
    },
    /// Run the plans.
    StartRun,
    /// Refine the product spec.
    RefineSpec,
    /// Start a new metaplan round.
    NewMetaplanRound,
}

impl Offer {
    /// The stable id: the kebab-case kind, then `:<nn>` (two digits) for a section offer or
    /// `:<slug>` for a plan offer (`write-section:05`, `pressure-test:alpha`).
    pub fn id(&self) -> String {
        match self {
            Offer::WriteSection { number } | Offer::AddSectionToVision { number } => {
                format!("{}:{number:02}", self.kind())
            }
            Offer::WritePlan { slug, .. }
            | Offer::AddPlanToMetaplan { slug }
            | Offer::PressureTest { slug } => format!("{}:{slug}", self.kind()),
            _ => self.kind().to_string(),
        }
    }

    /// The document the offer opens: the section file for a section offer, the plan file for
    /// `WritePlan` and `PressureTest` (`doc/plans/PLAN-<slug>.md`, the only path a written or
    /// unwritten plan has), the vision for `FixVisionTable`, else `None`. `AddPlanToMetaplan`
    /// edits a metaplan the user picks, so it has no target.
    pub fn target(&self) -> Option<PathBuf> {
        match self {
            Offer::WriteSection { number } | Offer::AddSectionToVision { number } => {
                Some(section_path(*number))
            }
            Offer::WritePlan { slug, .. } | Offer::PressureTest { slug } => Some(plan_path(slug)),
            Offer::FixVisionTable => Some(PathBuf::from(VISION)),
            _ => None,
        }
    }

    fn kind(&self) -> &'static str {
        match self {
            Offer::StartVision => "start-vision",
            Offer::FixVisionTable => "fix-vision-table",
            Offer::WriteSection { .. } => "write-section",
            Offer::AddSectionToVision { .. } => "add-section-to-vision",
            Offer::StartMetaplan => "start-metaplan",
            Offer::WritePlan { .. } => "write-plan",
            Offer::AddPlanToMetaplan { .. } => "add-plan-to-metaplan",
            Offer::PressureTest { .. } => "pressure-test",
            Offer::StartRun => "start-run",
            Offer::RefineSpec => "refine-spec",
            Offer::NewMetaplanRound => "new-metaplan-round",
        }
    }
}

/// An object with `id`, `kind`, `target` and the variant's fields (`number`, `slug`,
/// `metaplan`).
impl Serialize for Offer {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let mut map = serializer.serialize_map(None)?;
        map.serialize_entry("id", &self.id())?;
        map.serialize_entry("kind", self.kind())?;
        match self {
            Offer::WriteSection { number } | Offer::AddSectionToVision { number } => {
                map.serialize_entry("number", number)?;
            }
            Offer::WritePlan { metaplan, slug } => {
                map.serialize_entry("metaplan", metaplan)?;
                map.serialize_entry("slug", slug)?;
            }
            Offer::AddPlanToMetaplan { slug } | Offer::PressureTest { slug } => {
                map.serialize_entry("slug", slug)?;
            }
            _ => {}
        }
        map.serialize_entry("target", &self.target())?;
        map.end()
    }
}

/// `doc/product/section_<nn>.md`.
pub(super) fn section_path(number: u32) -> PathBuf {
    PathBuf::from(format!("doc/product/section_{number:02}.md"))
}

/// `doc/plans/PLAN-<slug>.md`.
pub(super) fn plan_path(slug: &str) -> PathBuf {
    Path::new("doc/plans").join(format!("PLAN-{slug}.md"))
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
        CurrentStep::Ready => vec![Offer::StartRun, Offer::RefineSpec],
        CurrentStep::ReadyAllDone => vec![Offer::RefineSpec, Offer::NewMetaplanRound],
    };
    offers.extend(extras(facts));
    let mut seen = BTreeSet::new();
    let mut unique = offers.into_iter().filter(|offer| seen.insert(offer.id()));
    (unique.next(), unique.collect())
}

/// The offers of every gap of steps 1 to 4, in step order. The first is the current step's
/// primary: the current step is the earliest gap, and a gap with no offer of its own (step 2
/// without a vision, step 4 without a metaplan) always sits behind an earlier gap that has one.
fn gap_offers(facts: &Facts) -> Vec<Offer> {
    let mut offers = Vec::new();
    if !facts.vision {
        offers.push(Offer::StartVision);
    } else if facts.rows.is_none() {
        offers.push(Offer::FixVisionTable);
    }
    let sections = facts.missing_sections.iter();
    offers.extend(sections.map(|&number| Offer::WriteSection { number }));
    if !facts.has_metaplan() {
        offers.push(Offer::StartMetaplan);
    }
    offers.extend(
        facts
            .missing_plans
            .iter()
            .map(|&(metaplan, slug)| Offer::WritePlan {
                metaplan: metaplan.to_path_buf(),
                slug: slug.to_string(),
            }),
    );
    offers
}

/// `PressureTest` for every untested written plan, in reading order.
fn pressure_tests(facts: &Facts) -> Vec<Offer> {
    let slugs = facts.untested.iter().map(|plan| plan.slug.clone());
    slugs.map(|slug| Offer::PressureTest { slug }).collect()
}

/// What every state also offers: the pressure tests, unlisted sections when a vision exists
/// and unlisted plans when a metaplan exists.
fn extras(facts: &Facts) -> Vec<Offer> {
    let mut offers = pressure_tests(facts);
    if facts.vision {
        let numbers = facts.unlisted_sections.iter().map(|&(number, _)| number);
        offers.extend(numbers.map(|number| Offer::AddSectionToVision { number }));
    }
    if facts.has_metaplan() {
        let slugs = facts.unlisted_plans.iter().map(|plan| plan.slug.clone());
        offers.extend(slugs.map(|slug| Offer::AddPlanToMetaplan { slug }));
    }
    offers
}
