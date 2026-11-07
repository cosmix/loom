import { ChevronRight, CornerDownRight, FilePlus2, X } from "lucide-react";
import { useId, type JSX, type ReactNode } from "react";

import { ShortcutGroups } from "@/components/help-shortcuts";
import { ignoreAction } from "@/plan/help/sample";
import { COVERAGE_SHORTCUTS, SHORTCUTS_LOOK } from "@/plan/help/shortcuts";
import { SettleMark, StateMark } from "@/plan/trace/coverage-marks";
import type { RowState } from "@/plan/trace/coverage-model";
import { DisabledReason } from "@/plan/ui/disabled-reason";

import "@/plan/trace/maps.css";
import "@/plan/trace/maps-findings.css";
import "@/plan/trace/trace.css";

// The help bodies of the two map pages. Each opens with an inert specimen drawn in the page's own
// classes (trace/maps.css, trace/maps-findings.css), so the help shows what the page shows, then
// names the parts. The pages read the feature's trace from the server, so the specimens are
// written here rather than mounted from the pages: a help scene never requests live data.

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

/** The ledger as the page draws it (`trace/findings-cards.tsx`): an open finding, then a decided one. */
export function FindingSpecimen(): JSX.Element {
  return (
    <div data-loom-plan inert>
      <ul className="maps-findings-list">
        <li className="maps-finding" data-status="open">
          <div className="maps-finding-head">
            <span className="plan-fine maps-finding-id">F-3</span>
            <h3 className="maps-finding-title">Stops have no time window</h3>
            <p className="plan-fine maps-finding-status">Open</p>
          </div>
          <p className="maps-about">
            <span>
              <span className="maps-chip">
                <span>trips · stops</span>
              </span>
            </span>
            <span>
              <span className="maps-chip">
                <span>Stops</span>
              </span>
            </span>
          </p>
          <p className="maps-finding-text">
            A stop stores a place and notes; the time window the spec asks for is missing from the
            record.
          </p>
          <div className="maps-actions">
            <DisabledReason reason={null} look="secondary" onClick={ignoreAction}>
              <FilePlus2 aria-hidden="true" />
              Plan this
            </DisabledReason>
            <DisabledReason reason={null} look="secondary" onClick={ignoreAction}>
              <CornerDownRight aria-hidden="true" />
              Fold into entry
            </DisabledReason>
            <DisabledReason reason={null} look="secondary" onClick={ignoreAction}>
              <X aria-hidden="true" />
              Dismiss
            </DisabledReason>
          </div>
        </li>
        <li className="maps-finding" data-status="dismissed">
          <div className="maps-finding-head">
            <span className="plan-fine maps-finding-id">F-2</span>
            <h3 className="maps-finding-title">Rename the flag</h3>
            <p className="plan-fine maps-finding-status">Dismissed: not needed</p>
          </div>
        </li>
      </ul>
    </div>
  );
}

const ACTIONS: readonly Definition[] = [
  [
    "Plan this",
    "Starts Plan follow-ups with the finding as its first message, to plan it on its own.",
  ],
  [
    "Fold into entry",
    "Picks a metaplan entry whose plan has not started, the best match first, and starts Plan follow-ups to fold the finding into it.",
  ],
  ["Dismiss", "Asks for a reason, then records the finding as dismissed with it."],
  ["Review the feature", "Starts a review session that records what you saw in the ledger."],
];

const STATUSES: readonly Definition[] = [
  [
    "Open",
    "Waiting for a decision; counted in the page's tally and on the Run page's Findings link.",
  ],
  ["Planned", "A plan named in the status takes it up."],
  ["Folded", "A metaplan entry named in the status takes it up before its plan is written or run."],
  ["Dismissed", "Set aside, with the reason beside the word."],
];

const GROUPS: readonly Definition[] = [
  ["Run of ...", "The findings a run review added when its plan finished, the newest run first."],
  ["Reviews", "The findings you recorded with Review the feature, after the runs."],
  [
    "Decided findings",
    "A planned, folded or dismissed finding shrinks to one line with its outcome.",
  ],
];

export function FindingsHelp(): JSX.Element {
  return (
    <div className="space-y-7 font-sans">
      <FindingSpecimen />
      <Definitions title="Groups" items={GROUPS} />
      <Definitions title="Deciding a finding" items={ACTIONS} />
      <Definitions title="Status" items={STATUSES} />
    </div>
  );
}

export const COVERAGE_TIPS: readonly string[] = [
  "A tile counts the whole feature and shows only its own rows when pressed.",
  "A heading under Not covered is a gap: write or extend a plan that names it.",
  "Open a row to read why the heading exists and how it is delivered, then follow its links.",
];

export const FINDINGS_TIPS: readonly string[] = [
  "A run review adds its open findings to the ledger when the plan completes, grouped under that run.",
  "Findings you record with Review the feature sit under Reviews, after the runs.",
  "The address of a finding is its card: a link from a coverage row lands on it.",
];
