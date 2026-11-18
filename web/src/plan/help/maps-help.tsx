import { ChevronRight } from "lucide-react";
import { useId, type JSX, type ReactNode } from "react";

import { ShortcutGroups } from "@/components/help-shortcuts";
import { FindingSpecimen } from "@/plan/help/finding-specimen";
import { COVERAGE_SHORTCUTS, SHORTCUTS_LOOK } from "@/plan/help/shortcuts";
import { SettleMark, StateMark } from "@/plan/trace/coverage-marks";
import type { RowState } from "@/plan/trace/coverage-model";

import "@/plan/trace/maps.css";
import "@/plan/trace/trace.css";

// The help bodies of the two map pages. Each opens with an inert specimen drawn in the page's own
// classes (trace/maps.css) or components (`finding-specimen.tsx`), so the help shows what the page
// shows, then names the parts. The pages read the feature's trace from the server, so the
// specimens are fed static data rather than mounted from the pages: a help scene never requests
// live data.

type Definition = readonly [name: string, meaning: string];

function Definitions({ title, items }: { title: string; items: readonly Definition[] }) {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <h3 id={headingId} className="font-sans text-[15px] font-semibold text-foreground">
        {title}
      </h3>
      <dl className="grid grid-cols-1 gap-x-6 gap-y-3 sm:grid-cols-2">
        {items.map(([name, meaning]) => (
          <div key={name}>
            <dt className="text-[13px] font-semibold leading-5 text-foreground">{name}</dt>
            <dd className="text-[13px] leading-5 text-muted-foreground">{meaning}</dd>
          </div>
        ))}
      </dl>
    </section>
  );
}

/** One heading row of the specimen, in the page's own markup (`trace/coverage-rows.tsx`). */
function SampleRow(props: {
  title: string;
  state: RowState;
  covered?: string;
  findings?: string;
  open?: boolean;
  children?: ReactNode;
}) {
  return (
    <li
      className="coverage-row"
      data-state={props.state}
      data-open={props.open ? "true" : undefined}
    >
      <div className="coverage-row__line">
        <button
          type="button"
          className="coverage-row__toggle"
          data-row-toggle
          aria-expanded={props.open === true}
        >
          <ChevronRight className="coverage-row__chevron" aria-hidden="true" />
          {props.title}
        </button>
        <div className="coverage-cell" data-cell="covered">
          {props.covered === undefined ? (
            <em className="coverage-nothing">Nothing yet</em>
          ) : (
            <span className="coverage-chain">{props.covered}</span>
          )}
        </div>
        <div className="coverage-cell" data-cell="findings">
          {props.findings ?? "–"}
        </div>
        <div className="coverage-cell" data-cell="state">
          <StateMark state={props.state} />
        </div>
      </div>
      {props.children}
    </li>
  );
}

function SampleTile(props: { label: string; count: number; pressed?: boolean }) {
  return (
    <button type="button" className="coverage-tile" aria-pressed={props.pressed === true}>
      <span className="coverage-tile__count">{props.count}</span>
      <span className="coverage-tile__label">{props.label}</span>
    </button>
  );
}

function SampleDetail(): JSX.Element {
  return (
    <div className="coverage-detail">
      <div className="coverage-detail__cols">
        <section className="coverage-col" data-col="why">
          <h3>Why</h3>
          <blockquote className="coverage-quote">
            <strong>Trip planning</strong> Travellers build a trip from stops and a budget.
          </blockquote>
        </section>
        <section className="coverage-col" data-col="how">
          <h3>How</h3>
          <dl className="coverage-how">
            <dt>Entry</dt>
            <dd>1. Trips</dd>
            <dt>Plan</dt>
            <dd>
              trips <span className="plan-fine coverage-plan-state">Running</span>
            </dd>
            <dt>Stage</dt>
            <dd>stops</dd>
          </dl>
        </section>
        <section className="coverage-col" data-col="findings">
          <h3>Findings</h3>
          <p className="plan-fine">F-3 Stops have no time window</p>
        </section>
      </div>
    </div>
  );
}

/** The page with the Stops row open: the tiles, a section, and what a row opens to. */
export function CoverageSpecimen(): JSX.Element {
  return (
    <div data-loom-plan inert className="coverage">
      <div className="coverage-tiles" role="group" aria-label="Show headings">
        <SampleTile label="All headings" count={12} pressed />
        <SampleTile label="Not covered" count={3} />
        <SampleTile label="Changed since covered" count={1} />
        <SampleTile label="With open findings" count={2} />
      </div>
      <section className="coverage-section" data-written="true">
        <header className="coverage-section__head">
          <span className="coverage-section__number">04</span>
          <h2 className="coverage-section__title">Trip planning</h2>
          <SettleMark state="settled" />
          <span className="coverage-section__count">2 of 3 covered</span>
        </header>
        <div className="coverage-cols" aria-hidden="true">
          <span>Heading</span>
          <span>Covered by</span>
          <span>Findings</span>
          <span>State</span>
        </div>
        <ul className="coverage-rows">
          <SampleRow title="Creating a trip" state="built" covered="trips › creation" />
          <SampleRow title="Stops" state="changed" covered="trips › stops" findings="1 open" open>
            <SampleDetail />
          </SampleRow>
          <SampleRow title="Budget" state="uncovered" />
        </ul>
      </section>
    </div>
  );
}

const TILES: readonly Definition[] = [
  ["All headings", "Every heading of every written section; the other tiles pick rows from it."],
  ["Not covered", "Headings no entry, plan or stage names yet."],
  ["Changed since covered", "Headings whose covering plan was settled against an earlier spec."],
  ["With open findings", "Headings a finding that waits for a decision is about."],
];

const ROW_PARTS: readonly Definition[] = [
  ["Heading", "The heading's name; open the row to read why it exists and how it is delivered."],
  ["Covered by", "The plans, and the stages of a plan, that implement the heading."],
  ["Findings", "How many open findings are about the heading."],
  ["State", "Where the work stands, in words; the glyph repeats it."],
];

const STATES: readonly Definition[] = [
  ["Built", "Every plan covering the heading is done."],
  ["Planned", "A plan or a metaplan entry covers it, and the work is not finished."],
  [
    "Changed since covered",
    'A covering plan reads "Spec changed since you settled it"; the open row says so and links what changed.',
  ],
  ["Not covered", "No entry, plan or stage names the heading."],
];

const OPEN_ROW: readonly Definition[] = [
  ["Why", "The vision row the section came from, with links to it and to the section."],
  ["How", "One block per covering path: the metaplan entry, the plan and its state, the stage."],
  ["Findings", 'The open findings about the heading, or "None open."'],
];

export function CoverageHelp(): JSX.Element {
  return (
    <div className="space-y-7 font-sans">
      <CoverageSpecimen />
      <Definitions title="The tiles" items={TILES} />
      <Definitions title="A heading row" items={ROW_PARTS} />
      <Definitions title="States" items={STATES} />
      <Definitions title="An open row" items={OPEN_ROW} />
      <div className={SHORTCUTS_LOOK}>
        <ShortcutGroups groups={[COVERAGE_SHORTCUTS]} />
      </div>
    </div>
  );
}

const ACTIONS: readonly Definition[] = [
  [
    "Address the findings",
    "Starts one session for every open, verified finding: a team with one writer per document edits the specs and plans they are about, and the lead marks each finding addressed, with what changed, or not, with why. The team panel above the findings shows its progress.",
  ],
  [
    "Review the feature",
    "Starts a review session that checks each unverified finding against the documents, adds its quote or dismisses it, and records what you saw with a quote of its own.",
  ],
  ["Dismiss", "Asks for a reason, then records the finding as dismissed with it."],
];

const STATUSES: readonly Definition[] = [
  [
    "Open",
    "Verified and waiting to be addressed, with its severity; counted in the page's tally, on the spine and on the Run panel's Findings link.",
  ],
  ["Being addressed", "An address run is working on it."],
  [
    "Unverified",
    "It has no quote of the text it is about yet, as a run review's findings arrive. Review the feature verifies it.",
  ],
  ["Stale", "Its quote is no longer in the passage it is about: the text changed since."],
  ["Duplicate", "An earlier finding reports the same thing; it is not counted or addressed."],
  ["Addressed", "An address run changed the documents; the headings it changed follow."],
  ["Could not address", "An address run left it, with the reason beside the words."],
  ["Dismissed", "Set aside, with the reason beside the word."],
];

const GROUPS: readonly Definition[] = [
  ["Run of ...", "The findings a run review added when its plan finished, the newest run first."],
  ["Reviews", "The findings you recorded with Review the feature, after the runs."],
  [
    "Lines",
    "An addressed, unaddressed, dismissed or duplicate finding shrinks to one line with its state.",
  ],
];

export function FindingsHelp(): JSX.Element {
  return (
    <div className="space-y-7 font-sans">
      <FindingSpecimen />
      <Definitions title="Groups" items={GROUPS} />
      <Definitions title="Sessions and actions" items={ACTIONS} />
      <Definitions title="State" items={STATUSES} />
    </div>
  );
}

export const COVERAGE_TIPS: readonly string[] = [
  "A tile counts the whole feature and shows only its own rows when pressed.",
  "A heading under Not covered is a gap: write or extend a plan that names it.",
  "Open a row to read why the heading exists and how it is delivered, then follow its links.",
];

export const FINDINGS_TIPS: readonly string[] = [
  "A run review adds its open findings when the plan completes, unverified; one that repeats an earlier finding is not added again.",
  "Run Review the feature before Address the findings: only verified findings are addressed.",
  "The address of a finding is its card: a link from a coverage row lands on it.",
];
