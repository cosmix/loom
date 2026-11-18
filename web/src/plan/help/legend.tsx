import { useId, type JSX } from "react";

import { TooltipProvider } from "@/components/ui/tooltip";
import "@/plan/concept/concept.css";
import "@/plan/feature/spine-instrument.css";

import { ACTIVITIES, READING, STATES, WELLS, type LegendEntry } from "./legend-entries";

/** Each entry is a 40px glyph slot with its name and meaning; `wide` slots hold a word or two marks. */
function LegendRow({ entry }: { entry: LegendEntry }): JSX.Element {
  const tile = entry.wide ? "w-[88px]" : "w-10";
  return (
    <div className={entry.wide ? "relative min-h-10 pl-[104px]" : "relative min-h-10 pl-14"}>
      <dt className="text-[13px] font-semibold leading-5 text-foreground">
        <span
          aria-hidden="true"
          className={`absolute top-0 left-0 inline-flex h-10 ${tile} items-center justify-center text-foreground`}
        >
          {entry.glyph}
        </span>
        {entry.name}
      </dt>
      <dd className="text-[13px] leading-5 text-muted-foreground">{entry.meaning}</dd>
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
      <h3 id={headingId} className="font-sans text-[15px] font-semibold text-foreground">
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
    // The mark and fold cap are the reading column's own components, which carry tooltips.
    <TooltipProvider>
      <div className="space-y-7 font-sans">
        <LegendGroup title="Reading" entries={READING} />
        <LegendGroup title="States" entries={STATES} />
        <LegendGroup title="Spine wells" entries={WELLS} />
        <LegendGroup title="In progress" entries={ACTIVITIES} />
      </div>
    </TooltipProvider>
  );
}
