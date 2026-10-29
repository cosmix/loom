import { useId, type JSX, type ReactNode } from "react";

import {
  ACTIVITY_KINDS,
  ACTIVITY_LABELS,
  ActivityGlyph,
  type ActivityKind,
} from "../ui/activity-glyphs";
import { DocMark } from "../ui/doc-mark";
import {
  DOC_KIND_GLYPHS,
  OTHER_GLYPHS,
  PLAN_STATE_GLYPHS,
  SUMMARY_STATE_GLYPHS,
  type DocKind,
  type PlanState,
  type SummaryState,
} from "../ui/glyphs";

type LegendEntry = { name: string; meaning: string; glyph: ReactNode };
type Definition = { name: string; meaning: string };

const READING: readonly LegendEntry[] = [
  {
    name: "Source mark",
    meaning: "Names a document the sentence draws on and opens its summary page.",
    glyph: <span className="size-2.5 rounded-[2px] bg-[var(--doc-1)]" />,
  },
  {
    name: "Dashed mark",
    meaning: "Marks a source whose summary is out of date, including while it rebuilds.",
    glyph: (
      <span className="size-2.5 rounded-[2px] border-[1.5px] border-dashed border-[var(--doc-1)]" />
    ),
  },
  {
    name: "Fold cap",
    meaning: "Closes the expanded run of text between its two caps; Esc also folds it.",
    glyph: (
      <span className="inline-flex size-5 items-center justify-center rounded-full bg-[var(--doc-1)] text-background">
        <span className="size-2 rotate-45 border-b-2 border-l-2" />
      </span>
    ),
  },
  {
    name: "Read-only",
    meaning: "Indicates that the document or session cannot be edited here.",
    glyph: <OTHER_GLYPHS.readOnly className="size-4" />,
  },
  {
    name: "Pressure-tested",
    meaning: "Indicates that the plan has been through a pressure test.",
    glyph: <OTHER_GLYPHS.pressureTested className="size-4" />,
  },
  {
    name: "Open questions",
    meaning: "Indicates questions that still need an answer.",
    glyph: <OTHER_GLYPHS.openQuestions className="size-4" />,
  },
];

const DOCUMENTS: Record<DocKind, Definition> = {
  vision: { name: "Vision", meaning: "Describes the feature's purpose and intended outcome." },
  section: { name: "Section", meaning: "Specifies part of the product's required behaviour." },
  note: { name: "Note", meaning: "Records supporting ideas, context or decisions." },
  metaplan: { name: "Metaplan", meaning: "Organises a feature's implementation into plans." },
  plan: { name: "Plan", meaning: "Defines the stages and checks for implementation work." },
  brief: { name: "Brief", meaning: "Gives an executor the context and scope for its work." },
  knowledge: { name: "Knowledge", meaning: "Stores project knowledge for future work." },
  review: { name: "Run review", meaning: "Records findings from reviewing an execution run." },
};

const PLAN_STATES: Record<PlanState, Definition> = {
  written: { name: "Written", meaning: "The plan is written and has not started running." },
  running: { name: "Running", meaning: "The plan's execution is in progress." },
  done: { name: "Done", meaning: "The plan's execution has finished." },
  retired: { name: "Retired", meaning: "The plan has been set aside from active work." },
};

const SUMMARY_STATES: Record<SummaryState, Definition> = {
  current: { name: "Current", meaning: "The summary is up to date with its sources." },
  rebuilding: { name: "Rebuilding", meaning: "A new summary is being generated." },
  stale: {
    name: "Stale",
    meaning:
      "The summary is out of date and may be queued, awaiting approval or unable to rebuild.",
  },
};

const ACTIVITY_MEANINGS: Record<ActivityKind, string> = {
  permission: "A session needs your permission before it can continue.",
  waiting: "A session is waiting for your reply.",
  running: "Loom is executing the stages of a plan.",
  pressure: "Reviewers are pressure-testing a plan.",
  codex: "Codex is reviewing the plan.",
  drafting: "A session is writing a document.",
  summaries: "Loom is rebuilding summaries from their sources.",
  starting: "A session is starting up.",
};

const STATES: readonly LegendEntry[] = [
  ...(Object.keys(DOC_KIND_GLYPHS) as DocKind[]).map((kind) => ({
    ...DOCUMENTS[kind],
    glyph: <DocMark kind={kind} />,
  })),
  ...(Object.keys(PLAN_STATE_GLYPHS) as PlanState[]).map((state) => {
    const Glyph = PLAN_STATE_GLYPHS[state];
    return { ...PLAN_STATES[state], glyph: <Glyph className="size-4" /> };
  }),
  ...(Object.keys(SUMMARY_STATE_GLYPHS) as SummaryState[]).map((state) => {
    const Glyph = SUMMARY_STATE_GLYPHS[state];
    return { ...SUMMARY_STATES[state], glyph: <Glyph className="size-4" /> };
  }),
];

const ACTIVITIES: readonly LegendEntry[] = ACTIVITY_KINDS.map((kind) => ({
  name: ACTIVITY_LABELS[kind],
  meaning: ACTIVITY_MEANINGS[kind],
  glyph: <ActivityGlyph kind={kind} className="size-4" />,
}));

function LegendRow({ entry }: { entry: LegendEntry }): JSX.Element {
  return (
    <div className="relative min-h-8 pl-11">
      <dt className="text-[13px] font-medium leading-5">
        <span
          aria-hidden="true"
          className="absolute top-0 left-0 inline-flex size-8 items-center justify-center rounded-lg bg-muted text-foreground"
        >
          {entry.glyph}
        </span>
        {entry.name}
      </dt>
      <dd className="text-xs leading-5 text-muted-foreground">{entry.meaning}</dd>
    </div>
  );
}

function LegendGroup({
  title,
  entries,
}: {
  title: string;
  entries: readonly LegendEntry[];
}): JSX.Element {
  const headingId = useId();
  return (
    <section aria-labelledby={headingId} className="space-y-3">
      <h3 id={headingId} className="font-sans text-sm font-semibold">
        {title}
      </h3>
      <dl className="grid grid-cols-1 gap-x-6 gap-y-4 sm:grid-cols-2">
        {entries.map((entry) => (
          <LegendRow key={entry.name} entry={entry} />
        ))}
      </dl>
    </section>
  );
}

export function PlannerLegend(): JSX.Element {
  return (
    <div className="space-y-7 font-sans">
      <LegendGroup title="Reading" entries={READING} />
      <LegendGroup title="States" entries={STATES} />
      <LegendGroup title="In progress" entries={ACTIVITIES} />
    </div>
  );
}
