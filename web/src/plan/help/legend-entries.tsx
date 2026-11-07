import type { CSSProperties, ReactNode } from "react";

import type { Mark, OutOfDate } from "@/plan/api/schema-concept";
import type { StepView } from "@/plan/api/schema";
import { FoldCap } from "@/plan/concept/fold-handle";
import { Marks } from "@/plan/concept/marks";
import { PlanGlyph } from "@/plan/concept/plan-glyph";
import {
  ACTIVITY_LABELS,
  ActivityGlyph,
  StateMark,
  type YourTurnKind,
} from "@/plan/ui/activity-glyphs";
import { DocMark } from "@/plan/ui/doc-mark";
import {
  DOC_KIND_GLYPHS,
  OTHER_GLYPHS,
  PLAN_STATE_WORD_GLYPHS,
  STEP_MARK_GLYPHS,
  SUMMARY_STATE_GLYPHS,
  type DocKind,
  type PlanState,
  type SummaryState,
} from "@/plan/ui/glyphs";

/** `wide` entries need more than the 32px square for a word or multiple marks. */
export type LegendEntry = { name: string; meaning: string; glyph: ReactNode; wide?: boolean };
type Definition = { name: string; meaning: string };

const NOOP = (): undefined => undefined;
const STALE: OutOfDate = { reason: "source_changed", detail: null };

function sampleMark(outOfDate: OutOfDate | null): Mark {
  return {
    doc: { id: "legend", title: "Source", path: "doc/source.md", page: "legend" },
    colour: 1,
    out_of_date: outOfDate,
  };
}

/**
 * Production marks and caps size themselves in `em`, so the tile sets the font size they scale
 * from. They are buttons in the reading column; here the tile is inert.
 */
function MarkTile({ outOfDate }: { outOfDate: OutOfDate | null }) {
  return (
    <span className="pointer-events-none text-[28px] leading-none">
      <Marks marks={[sampleMark(outOfDate)]} onOpen={NOOP} />
    </span>
  );
}

function CapTile() {
  const style = { "--run-colour": "var(--doc-1)" } as CSSProperties;
  return (
    <span className="pointer-events-none text-base leading-none" style={style}>
      <FoldCap runId="legend-fold-cap" side="start" onFold={NOOP} />
    </span>
  );
}

/** The link button is inert here; its popover lists the plans that implement a section. */
function PlanGlyphTile() {
  return (
    <span className="pointer-events-none text-base leading-none">
      <PlanGlyph plans={[{ slug: "atlas", title: "Atlas", state: "written" }]} sections={[]} />
    </span>
  );
}

/** `.concept-no-source` sizes in `em` below 13px; the inline style keeps the word readable. */
function NoSourceTile() {
  return (
    <span className="concept-no-source" style={{ margin: 0, fontSize: 13 }}>
      no source
    </span>
  );
}

function otherGlyph(kind: keyof typeof OTHER_GLYPHS): ReactNode {
  const Glyph = OTHER_GLYPHS[kind];
  return <Glyph data-other-glyph={kind} className="size-4" />;
}

export const READING: readonly LegendEntry[] = [
  {
    name: "Source mark",
    meaning: "Names a document the sentence draws on and opens its summary page.",
    glyph: <MarkTile outOfDate={null} />,
  },
  {
    name: "Dashed mark",
    meaning:
      "Marks a source whose summary is out of date, including while it rebuilds or waits for approval.",
    glyph: <MarkTile outOfDate={STALE} />,
  },
  {
    name: "No source",
    meaning: "Replaces the marks on a sentence that cites nothing, which is a defect in its page.",
    glyph: <NoSourceTile />,
    wide: true,
  },
  {
    name: "Fold cap",
    meaning: "Closes the expanded run of text between its two caps; Esc also folds it.",
    glyph: <CapTile />,
  },
  {
    name: "Plan link",
    meaning:
      "Ends a document's first sentence and lists the plans that implement a section, or the sections a plan implements.",
    glyph: <PlanGlyphTile />,
  },
  {
    name: "Read-only",
    meaning: "Appears beside the note explaining why a document is locked against editing.",
    glyph: otherGlyph("readOnly"),
  },
  {
    name: "Pressure-test status",
    meaning:
      "Marks a plan's pressure-test status: not tested, being tested, or the date of the last test; the flask in the home table marks a tested standalone plan.",
    glyph: otherGlyph("pressureTested"),
  },
  {
    name: "Open questions",
    meaning: "Identifies questions that still need an answer.",
    glyph: otherGlyph("openQuestions"),
  },
  {
    name: "Feature switcher",
    meaning: "Opens feature navigation to choose another feature or return home.",
    glyph: otherGlyph("switcher"),
  },
  {
    name: "Settings",
    meaning: "Opens the planner's reading and appearance preferences.",
    glyph: otherGlyph("settings"),
  },
  {
    name: "Help",
    meaning: "Opens the planner's guides, legend and keyboard shortcuts.",
    glyph: otherGlyph("help"),
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

const STEP_MARKS: readonly (Definition & { mark: StepView["mark"] })[] = [
  {
    mark: "done",
    name: "Done step",
    meaning: "A circled check on a spine row means its documents are all written.",
  },
  {
    mark: "gap",
    name: "Step with gaps",
    meaning: "A dashed circle on a spine row means it has documents still to write.",
  },
  {
    mark: "active",
    name: "Active step",
    meaning: "A circled dot on a spine row means it is the step in progress.",
  },
  {
    mark: "optional",
    name: "Optional step",
    meaning: "A dash on a spine row means the step can be skipped.",
  },
];

export const STATES: readonly LegendEntry[] = [
  ...(Object.keys(DOC_KIND_GLYPHS) as DocKind[]).map((kind) => ({
    ...DOCUMENTS[kind],
    glyph: <DocMark kind={kind} />,
  })),
  ...(Object.keys(PLAN_STATE_WORD_GLYPHS) as PlanState[]).map((state) => {
    const Glyph = PLAN_STATE_WORD_GLYPHS[state];
    return { ...PLAN_STATES[state], glyph: <Glyph className="size-4" /> };
  }),
  ...(Object.keys(SUMMARY_STATE_GLYPHS) as SummaryState[]).map((state) => {
    const Glyph = SUMMARY_STATE_GLYPHS[state];
    return { ...SUMMARY_STATES[state], glyph: <Glyph className="size-4" /> };
  }),
  ...STEP_MARKS.map(({ mark, name, meaning }) => {
    const Glyph = STEP_MARK_GLYPHS[mark];
    return {
      name,
      meaning,
      glyph: <Glyph data-step-mark={mark} className="size-4" />,
    };
  }),
  {
    name: "Finished pressure step",
    meaning: "In the pressure timeline, a check replaces a step's own glyph once the step is done.",
    glyph: (
      <OTHER_GLYPHS.stepDone
        data-other-glyph="stepDone"
        className="size-4 text-(--tone-completed)"
      />
    ),
  },
];

/** The signs that mean "your turn"; work under way draws the two rotating arrows instead. */
const YOUR_TURN: Record<YourTurnKind, string> = {
  permission: "A session needs your permission before it can continue.",
  waiting: "A session is waiting for your reply.",
};

export const ACTIVITIES: readonly LegendEntry[] = [
  {
    name: "Working",
    meaning:
      "Two rotating arrows stand before a word that says what is happening: Running, Drafting, Rebuilding. They stop when the work does. A green dot pulses before the item or action itself, such as a plan, a document or a pressure test.",
    glyph: <StateMark kind="running" className="size-4" />,
  },
  ...(Object.keys(YOUR_TURN) as YourTurnKind[]).map((kind) => ({
    name: ACTIVITY_LABELS[kind],
    meaning: YOUR_TURN[kind],
    glyph: <ActivityGlyph kind={kind} className="size-4" />,
  })),
];
