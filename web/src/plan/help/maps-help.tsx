import { useId, type JSX, type ReactNode } from "react";

import { ShortcutGroups } from "@/components/help-shortcuts";
import { ignoreAction } from "@/plan/help/sample";
import { COVERAGE_SHORTCUTS, SHORTCUTS_LOOK } from "@/plan/help/shortcuts";
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

/** One card of the specimen map, in the page's own markup (`coverage-page.tsx`, `CardButton`). */
function SampleCard(props: { title: string; state?: string; lineage?: string }) {
  return (
    <li className="maps-card-item">
      <button
        type="button"
        className="maps-card"
        data-state={props.state ?? "plain"}
        data-lineage={props.lineage}
        aria-current={props.lineage === "self" ? "true" : undefined}
      >
        <span>{props.title}</span>
      </button>
    </li>
  );
}

function SampleColumn(props: {
  id: string;
  label: string;
  group?: string;
  children: ReactNode;
}): JSX.Element {
  return (
    <section className="maps-column" data-column={props.id}>
      <h4 className="plan-eyebrow maps-column-title">{props.label}</h4>
      <div className="maps-group">
        {props.group && <p className="plan-fine maps-group-title">{props.group}</p>}
        <ul className="maps-cards">{props.children}</ul>
      </div>
    </section>
  );
}

/** The map with "Stops" selected: its section and vision row upstream, its entry, plan, stage and finding downstream. */
export function CoverageSpecimen(): JSX.Element {
  return (
    <div data-loom-plan inert className="maps-coverage">
      <div className="maps-body">
        <div className="maps-columns">
          <SampleColumn id="vision" label="Vision rows">
            <SampleCard title="Trip planning" state="revised" lineage="up" />
            <SampleCard title="Sign-ups" />
          </SampleColumn>
          <SampleColumn id="spec" label="Spec headings" group="Trip planning">
            <SampleCard title="Creating a trip" />
            <SampleCard title="Stops" lineage="self" />
            <SampleCard title="Budget" state="uncovered" />
          </SampleColumn>
          <SampleColumn id="metaplan" label="Metaplan entries">
            <SampleCard title="1. Trips" lineage="down" />
            <SampleCard title="2. Sign-ups" />
          </SampleColumn>
          <SampleColumn id="plans" label="Plans and stages">
            <SampleCard title="Trips" lineage="down" />
            <SampleCard title="trips · stops" lineage="down" />
          </SampleColumn>
          <SampleColumn id="findings" label="Findings">
            <SampleCard title="F-3 Stops have no time window" state="open" lineage="down" />
          </SampleColumn>
        </div>
      </div>
    </div>
  );
}

const COLUMNS: readonly Definition[] = [
  [
    "Vision rows",
    "The sections the vision lists, written or not; a section's settle state shows under its name.",
  ],
  ["Spec headings", "Every level-2 heading of a written section, grouped under its section."],
  [
    "Metaplan entries",
    "The plans the metaplan lists, each covering the headings its Covers line names.",
  ],
  ["Plans and stages", "Written plans and the stages that implement a heading on their own."],
  ["Findings", "What reviews found, attached to the headings, entries and plans they are about."],
];

const MARKS: readonly Definition[] = [
  ["Not covered", "A dashed card: no entry, plan or stage names this heading yet."],
  [
    "Revised, Stub changed, Upstream changed",
    "An amber dot: the document moved since it was settled.",
  ],
  ["Open", "An amber dot on a finding that still waits for a decision."],
  ["Lineage", "Select a card: it fills, its relatives take an accent dot, and the rest sink back."],
];

const FILTERS: readonly Definition[] = [
  ["All", "Every card."],
  ["Uncovered", "Headings nothing covers, with the sections and vision rows they sit in."],
  ["Changed since settled", "Documents that moved since they were settled, with their lineage."],
  ["Open findings", "Findings waiting for a decision, with everything they are about."],
];

export function CoverageHelp(): JSX.Element {
  return (
    <div className="space-y-7 font-sans">
      <CoverageSpecimen />
      <Definitions title="The columns" items={COLUMNS} />
      <Definitions title="Marks on a card" items={MARKS} />
      <Definitions title="Filters" items={FILTERS} />
      <div className={SHORTCUTS_LOOK}>
        <ShortcutGroups groups={[COVERAGE_SHORTCUTS]} />
      </div>
    </div>
  );
}

/** One open finding as the ledger draws it (`findings-page.tsx`, `FindingCard`), with its actions. */
export function FindingSpecimen(): JSX.Element {
  return (
    <div data-loom-plan inert>
      <ul className="maps-findings-list">
        <li className="maps-finding" data-status="open">
          <div className="maps-finding-head">
            <span className="plan-fine maps-finding-id">F-3</span>
            <p className="maps-finding-title">Stops have no time window</p>
          </div>
          <p className="plan-fine maps-finding-status">Open</p>
          <p className="maps-finding-text">
            A stop stores a place and notes; the time window the spec asks for is missing from the
            record.
          </p>
          <p className="maps-about">
            <span>
              <span className="trace-chip maps-chip">
                <span>trips · stops</span>
              </span>
            </span>
            <span>
              <span className="trace-chip maps-chip">
                <span>Stops</span>
              </span>
            </span>
          </p>
          <div className="maps-actions">
            <DisabledReason reason={null} look="quiet" onClick={ignoreAction}>
              Plan this
            </DisabledReason>
            <DisabledReason reason={null} look="quiet" onClick={ignoreAction}>
              Fold into entry
            </DisabledReason>
            <DisabledReason reason={null} look="quiet" onClick={ignoreAction}>
              Dismiss
            </DisabledReason>
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
    "Picks a metaplan entry whose plan has not started, and starts Plan follow-ups to fold the finding into it.",
  ],
  ["Dismiss", "Asks for a reason, then records the finding as dismissed with it."],
  ["Review the feature", "Starts a review session that records what you saw in the ledger."],
];

const STATUSES: readonly Definition[] = [
  ["Open", "Waiting for a decision; counted on the spine's Findings row."],
  ["Planned", "A plan named in the status takes it up."],
  ["Folded", "A metaplan entry named in the status takes it up before its plan is written or run."],
  ["Dismissed", "Set aside, with the reason beside the word."],
];

export function FindingsHelp(): JSX.Element {
  return (
    <div className="space-y-7 font-sans">
      <FindingSpecimen />
      <Definitions title="Deciding a finding" items={ACTIONS} />
      <Definitions title="Status" items={STATUSES} />
    </div>
  );
}

export const COVERAGE_TIPS: readonly string[] = [
  "The map draws no lines between columns: select a card and read its lineage from the accent dots.",
  "A filter keeps each matching card's lineage on the map, so a gap is always shown inside its section.",
  "Open from the focus panel, or press Enter on a card, to reach the document or passage it stands for.",
];

export const FINDINGS_TIPS: readonly string[] = [
  "A run review adds its open findings to the ledger when the plan completes, grouped under that run.",
  "Findings you record with Review the feature sit under Reviews, after the runs.",
  "The address of a finding is its card: a link from the coverage map lands on it.",
];
