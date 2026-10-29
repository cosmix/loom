import type { JSX, ReactNode } from "react";

import { ActivityGlyph, type ActivityKind } from "@/plan/ui/activity-glyphs";

export interface RoundDiagramProps {
  rounds: number;
  review: ReactNode;
  secondReview: ReactNode;
  address: ReactNode;
}

// A cleared Rounds input arrives as NaN; show the legend without a count then.
function roundsLegend(rounds: number): string {
  if (!Number.isFinite(rounds)) return "Each round";
  return `Each round, ${rounds} ${rounds === 1 ? "time" : "times"}`;
}

export function RoundDiagram(props: RoundDiagramProps): JSX.Element {
  return (
    <fieldset
      role="group"
      aria-label="One pressure-test round"
      className="min-w-0 rounded-lg border border-dashed border-hairline p-3"
    >
      <legend className="px-2 text-xs font-medium text-muted-foreground">
        {roundsLegend(props.rounds)}
      </legend>
      <div className="grid grid-cols-[1rem_minmax(0,1fr)] sm:grid-cols-2 sm:gap-x-3">
        <div className="col-start-2 row-start-1 min-w-0 sm:col-start-1">
          <RoundNode title="Review" command="/pressure" kind="pressure">
            {props.review}
          </RoundNode>
        </div>
        <ReviewConnector />
        <div
          aria-hidden="true"
          className="col-start-1 row-start-3 border-l border-hairline sm:hidden"
        />
        <div className="col-start-2 row-start-3 min-w-0 sm:row-start-1">
          <RoundNode title="Second review" command="$pressure" kind="codex" background>
            {props.secondReview}
          </RoundNode>
        </div>
        <AddressConnectors />
        <div className="col-start-2 row-start-5 min-w-0 sm:col-span-2 sm:col-start-1 sm:row-start-3">
          <RoundNode title="Address" command="/address" kind="drafting">
            {props.address}
          </RoundNode>
        </div>
      </div>
    </fieldset>
  );
}

function RoundNode({
  title,
  command,
  kind,
  background = false,
  children,
}: {
  title: string;
  command: string;
  kind: ActivityKind;
  background?: boolean;
  children: ReactNode;
}): JSX.Element {
  return (
    <section
      aria-label={title}
      className="h-full min-w-0 space-y-3 rounded-md border border-hairline bg-card p-3"
    >
      <div className="flex flex-wrap items-center gap-2 text-sm">
        <ActivityGlyph kind={kind} paused className="size-4 shrink-0" />
        <span className="font-medium">{title}</span>
        <code className="rounded bg-muted px-1.5 py-0.5 text-xs text-muted-foreground">
          {command}
        </code>
        {background && (
          <span className="rounded-full border border-hairline px-2 py-0.5 text-xs text-muted-foreground">
            background
          </span>
        )}
      </div>
      <div className="min-w-0">{children}</div>
    </section>
  );
}

// The first review bypasses the second on narrow screens: both feed Address.
function ReviewConnector(): JSX.Element {
  return (
    <svg
      aria-hidden="true"
      focusable="false"
      viewBox="0 0 100 24"
      preserveAspectRatio="none"
      className="col-span-2 col-start-1 row-start-2 h-6 w-full overflow-visible text-hairline sm:hidden"
      fill="none"
      stroke="currentColor"
    >
      <path d="M50 0V12H0V24" vectorEffect="non-scaling-stroke" />
    </svg>
  );
}

function AddressConnectors(): JSX.Element {
  return (
    <div
      aria-hidden="true"
      className="relative col-span-2 col-start-1 row-start-4 h-8 sm:row-start-2"
    >
      <svg
        focusable="false"
        viewBox="0 0 100 32"
        preserveAspectRatio="none"
        className="h-full w-full overflow-visible text-hairline"
        fill="none"
        stroke="currentColor"
      >
        <path d="M0 0V16H50V32M50 0V16" vectorEffect="non-scaling-stroke" className="sm:hidden" />
        <path
          d="M25 0V16H50V32M75 0V16H50"
          vectorEffect="non-scaling-stroke"
          className="hidden sm:block"
        />
      </svg>
      <span className="absolute bottom-0 left-1/2 size-1.5 -translate-x-1/2 -translate-y-0.5 rotate-45 border-r border-b border-hairline" />
    </div>
  );
}
