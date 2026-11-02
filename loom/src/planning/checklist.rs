//! The planning checklist computed from the project view (section 01): the step marks, the
//! current step, one item per expected or unlisted document, the offers and the starting-point
//! state.

mod items;
mod offers;
#[cfg(test)]
mod offers_tests;
#[cfg(test)]
mod order_tests;
#[cfg(test)]
mod scope_tests;
#[cfg(test)]
mod tests;

pub use offers::{move_offers, Offer};

use std::collections::{BTreeMap, BTreeSet};
use std::path::{Path, PathBuf};

use serde::{Serialize, Serializer};

use crate::planning::discover::DocTree;
use crate::planning::kinds::vision_path;
use crate::planning::metaplan::MetaplanEntry;
use crate::planning::pressure::PressureRecord;
use crate::planning::slug::{PlanEntry, PlanState};
use crate::planning::vision::{VisionProblem, VisionRow};

/// What `build_checklist` reads beyond the tree; `Project::load` parses it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ParsedDocs {
    /// The vision's section rows (`Err` also when there is no vision).
    pub vision_rows: Result<Vec<VisionRow>, VisionProblem>,
    /// Every metaplan's entries, sorted by metaplan path.
    pub metaplans: Vec<(PathBuf, Vec<MetaplanEntry>)>,
    /// Pressure-test records by plan slug.
    pub pressure: BTreeMap<String, Vec<PressureRecord>>,
    /// The existing locked files under this feature's directories.
    pub locked_files: Vec<PathBuf>,
    /// Sorted numbers of the product section files that hold a non-whitespace character, or
    /// that could not be read and are not empty.
    pub sections_written: Vec<u32>,
}

/// A step of section 01's workflow, in order.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Step {
    /// 1: the product vision.
    Vision = 1,
    /// 2: the product sections the vision lists.
    ProductSpec,
    /// 3: at least one metaplan.
    Metaplan,
    /// 4: a plan for every metaplan entry.
    Plans,
    /// 5: pressure tests of written plans.
    PressureTest,
    /// 6: running the plans.
    Run,
}

impl Step {
    /// The step's display name.
    pub fn name(self) -> &'static str {
        match self {
            Self::Vision => "Vision",
            Self::ProductSpec => "Product spec",
            Self::Metaplan => "Metaplan",
            Self::Plans => "Plans",
            Self::PressureTest => "Pressure test",
            Self::Run => "Run",
        }
    }
}

/// How a step stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum StepMark {
    /// The step's documents exist.
    Done,
    /// Something the step expects is missing.
    Gap,
    /// The step is under way (a plan is running).
    Active,
    /// The step may be done but is never required.
    Optional,
}

/// Where the project stands: the earliest gap, or ready to run.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CurrentStep {
    /// The step to work on next.
    Step(Step),
    /// Steps 1 to 5 hold no gap: the plans can run.
    Ready,
    /// Every metaplan entry's plan is done or retired.
    ReadyAllDone,
}

/// Serialises to one string: the step's snake_case name, `ready` or `ready_all_done`.
impl Serialize for CurrentStep {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        match self {
            CurrentStep::Step(step) => step.serialize(serializer),
            CurrentStep::Ready => serializer.serialize_str("ready"),
            CurrentStep::ReadyAllDone => serializer.serialize_str("ready_all_done"),
        }
    }
}

/// How a checklist item stands.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    /// The expected document exists.
    Present,
    /// The expected document is missing.
    Gap,
    /// A document no vision table or metaplan lists.
    Unlisted,
    /// A written plan with no pressure-test record.
    Untested,
}

/// One line of the checklist.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ChecklistItem {
    /// The step the item belongs to.
    pub step: Step,
    /// How it stands.
    pub status: ItemStatus,
    /// The document, existing or expected; `None` for "no metaplan".
    pub path: Option<PathBuf>,
    /// One line naming the item.
    pub label: String,
    /// What the app offers for it.
    pub offer: Option<Offer>,
}

/// The starting-point facts a planning session is briefed with.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ProjectState {
    /// Whether the feature's vision exists.
    pub vision_present: bool,
    /// Rows of the vision's table (0 when it cannot be read).
    pub sections_listed: usize,
    /// Sorted numbers of the written section files: those holding a non-whitespace character,
    /// or that could not be read and are not empty.
    pub sections_written: Vec<u32>,
    /// Metaplan paths, sorted.
    pub metaplans: Vec<PathBuf>,
    /// Plans no metaplan names.
    pub plans_unlisted: usize,
    /// Plan count per state (states with no plan are absent).
    pub plans_by_state: BTreeMap<PlanState, usize>,
    /// The existing locked files under this feature's directories.
    pub locked_files: Vec<PathBuf>,
}

/// One feature's checklist of section 01.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Checklist {
    /// The feature this checklist describes.
    pub feature: String,
    /// The current step.
    pub current: CurrentStep,
    /// All six steps in order with their marks (JSON `[{"step", "mark"}]`).
    #[serde(serialize_with = "steps_json")]
    pub steps: Vec<(Step, StepMark)>,
    /// One item per expected, unlisted or untested document, in step order.
    pub items: Vec<ChecklistItem>,
    /// The main offer.
    pub primary: Option<Offer>,
    /// The other offers, each once.
    pub secondary: Vec<Offer>,
    /// Starting-point facts.
    pub state: ProjectState,
}

fn steps_json<S: Serializer>(steps: &[(Step, StepMark)], serializer: S) -> Result<S::Ok, S::Error> {
    #[derive(Serialize)]
    struct Entry<'a> {
        step: Step,
        mark: &'a StepMark,
    }
    serializer.collect_seq(steps.iter().map(|(step, mark)| Entry { step: *step, mark }))
}

/// Compute the checklist from a discovered tree and its parsed documents (pure).
pub fn build_checklist(tree: &DocTree, parsed: &ParsedDocs, feature: &str) -> Checklist {
    let facts = Facts::new(tree, parsed, feature);
    let steps = facts.marks();
    let current = current_step(&steps);
    let (primary, secondary) = offers::choose(&current, &facts);
    Checklist {
        feature: feature.to_string(),
        current,
        steps,
        items: facts.items(),
        primary,
        secondary,
        state: facts.state(),
    }
}

/// The facts every rule reads, computed once, each list in reading order.
struct Facts<'a> {
    pub(super) feature: &'a str,
    tree: &'a DocTree,
    parsed: &'a ParsedDocs,
    vision: bool,
    /// The vision's rows; `None` without a vision or with an unreadable table.
    rows: Option<&'a [VisionRow]>,
    /// Listed sections with no written file, in table order.
    missing_sections: Vec<u32>,
    /// Section files the table does not list, by path.
    unlisted_sections: Vec<(u32, &'a Path)>,
    /// Every metaplan entry naming a plan: `(metaplan, slug)`.
    entries: Vec<(&'a Path, &'a str)>,
    /// The first entry of every slug that resolves to no plan.
    missing_plans: Vec<(&'a Path, &'a str)>,
    /// Plans no entry names, by file name.
    unlisted_plans: Vec<&'a PlanEntry>,
    /// Written plans with no pressure record.
    untested: Vec<&'a PlanEntry>,
}

impl<'a> Facts<'a> {
    fn new(tree: &'a DocTree, parsed: &'a ParsedDocs, feature: &'a str) -> Self {
        let vision = tree.get(&vision_path(feature)).is_some();
        let rows = parsed.vision_rows.as_deref().ok().filter(|_| vision);
        let written: BTreeSet<u32> = parsed.sections_written.iter().copied().collect();
        let numbers = rows.unwrap_or_default().iter().map(|row| row.number);
        let missing_sections = numbers.filter(|n| !written.contains(n)).collect();
        let unlisted_sections = items::unlisted_sections(tree, feature, rows);
        let entries = items::metaplan_entries(parsed);
        let mut named = BTreeSet::new();
        let firsts: Vec<_> = entries
            .iter()
            .copied()
            .filter(|(_, s)| named.insert(*s))
            .collect();
        let unwritten = firsts.iter().copied();
        let plan_for = |slug: &str| {
            tree.plan(slug)
                .filter(|plan| plan.feature.as_deref() == Some(feature))
        };
        // Slugs are repository-wide: a misplaced plan must be moved, never rewritten.
        let missing_plans = unwritten.filter(|(_, s)| tree.plan(s).is_none()).collect();
        let mut unlisted_plans: Vec<&PlanEntry> = tree
            .plans
            .values()
            .filter(|p| p.feature.as_deref() == Some(feature))
            .filter(|p| !named.contains(p.slug.as_str()))
            .collect();
        unlisted_plans.sort_by(|a, b| a.path.file_name().cmp(&b.path.file_name()));
        let listed_plans = firsts.iter().filter_map(|(_, slug)| plan_for(slug));
        let untested = listed_plans.chain(unlisted_plans.iter().copied());
        let untested = untested.filter(|plan| is_untested(plan, parsed)).collect();
        Facts {
            feature,
            tree,
            parsed,
            vision,
            rows,
            missing_sections,
            unlisted_sections,
            entries,
            missing_plans,
            unlisted_plans,
            untested,
        }
    }

    fn has_metaplan(&self) -> bool {
        !self.parsed.metaplans.is_empty()
    }

    fn plans(&self) -> impl Iterator<Item = &'a PlanEntry> + '_ {
        self.tree
            .plans
            .values()
            .filter(|plan| plan.feature.as_deref() == Some(self.feature))
    }

    fn plan(&self, slug: &str) -> Option<&PlanEntry> {
        // Entry presence and completion follow the unique slug, even in a foreign home.
        self.tree.plan(slug)
    }

    fn marks(&self) -> Vec<(Step, StepMark)> {
        let done_or = |done: bool, otherwise| if done { StepMark::Done } else { otherwise };
        let spec = self.rows.is_some() && self.missing_sections.is_empty();
        let plans = self.has_metaplan() && self.missing_plans.is_empty();
        vec![
            (Step::Vision, done_or(self.vision, StepMark::Gap)),
            (Step::ProductSpec, done_or(spec, StepMark::Gap)),
            (Step::Metaplan, done_or(self.has_metaplan(), StepMark::Gap)),
            (Step::Plans, done_or(plans, StepMark::Gap)),
            (
                Step::PressureTest,
                done_or(self.untested.is_empty(), StepMark::Optional),
            ),
            (Step::Run, self.run_mark()),
        ]
    }

    fn run_mark(&self) -> StepMark {
        if self.plans().any(|plan| plan.state == PlanState::Running) {
            return StepMark::Active;
        }
        let finished = |slug: &str| {
            let plan = self.plan(slug);
            plan.is_some_and(|plan| plan.state >= PlanState::Done)
        };
        if !self.entries.is_empty() && self.entries.iter().all(|(_, slug)| finished(slug)) {
            StepMark::Done
        } else {
            StepMark::Optional
        }
    }

    fn state(&self) -> ProjectState {
        let mut plans_by_state = BTreeMap::new();
        for plan in self.plans() {
            *plans_by_state.entry(plan.state).or_insert(0) += 1;
        }
        ProjectState {
            vision_present: self.vision,
            sections_listed: self.rows.map_or(0, <[VisionRow]>::len),
            sections_written: self.parsed.sections_written.clone(),
            metaplans: self
                .parsed
                .metaplans
                .iter()
                .map(|(path, _)| path.clone())
                .collect(),
            plans_unlisted: self.unlisted_plans.len(),
            plans_by_state,
            locked_files: self.parsed.locked_files.clone(),
        }
    }
}

fn is_untested(plan: &PlanEntry, parsed: &ParsedDocs) -> bool {
    plan.state == PlanState::Written && parsed.pressure.get(&plan.slug).is_none_or(Vec::is_empty)
}

/// The earliest gap of steps 1 to 4; then step 5 while it is optional; then ready.
fn current_step(steps: &[(Step, StepMark)]) -> CurrentStep {
    let gap = steps
        .iter()
        .find(|(step, mark)| *step <= Step::Plans && *mark == StepMark::Gap);
    if let Some((step, _)) = gap {
        return CurrentStep::Step(*step);
    }
    let mark = |want: Step| {
        steps
            .iter()
            .find(|(step, _)| *step == want)
            .map(|(_, mark)| mark)
    };
    if mark(Step::PressureTest) == Some(&StepMark::Optional) {
        CurrentStep::Step(Step::PressureTest)
    } else if mark(Step::Run) == Some(&StepMark::Done) {
        CurrentStep::ReadyAllDone
    } else {
        CurrentStep::Ready
    }
}
