import { Repeat } from "lucide-react";
import type { CSSProperties, JSX, ReactNode } from "react";

import { ActivityGlyph, type ActivityKind } from "@/plan/ui/activity-glyphs";

export interface RoundDiagramProps {
  rounds: number;
  review: ReactNode;
  secondReview: ReactNode;
  address: ReactNode;
}

function roundsLegend(rounds: number): string {
  return rounds === 1 ? "Each round, once" : `Each round, ${rounds} times`;
}

/**
 * Review and Second review stack in column 1, their wires merge in the 46px middle column, and
 * Address spans both rows in column 3. Below 600px it is one column and the wires go.
 */
export function RoundDiagram(props: RoundDiagramProps): JSX.Element {
  return (
    <fieldset
      role="group"
      aria-label="One pressure-test round"
      className="relative m-0 min-w-0 rounded-[14px] border-[1.5px] border-dashed border-[color-mix(in_oklch,var(--ring)_55%,var(--border))] px-4 pt-[26px] pb-4"
    >
      <RoundsLegend rounds={props.rounds} />
      <div className="grid grid-cols-[minmax(0,1fr)_46px_minmax(0,1fr)] grid-rows-[1fr_1fr] gap-y-2.5 max-[600px]:grid-cols-1 max-[600px]:grid-rows-none">
        <div className="col-start-1 row-start-1 min-w-0">
          <RoundNode
            title="Review"
            kind="pressure"
            tile="--doc-8"
            line={
              <>
                <AgentLine agent="Claude" command="/pressure" /> · a team of 3
              </>
            }
          >
            {props.review}
          </RoundNode>
        </div>
        <div className="col-start-1 row-start-2 min-w-0 max-[600px]:row-start-auto">
          <RoundNode
            title="Second review"
            kind="codex"
            tile="--doc-3"
            line={<AgentLine agent="Codex" command="$pressure" />}
            background
          >
            {props.secondReview}
          </RoundNode>
        </div>
        <Wires />
        <div className="col-start-3 row-span-2 row-start-1 min-w-0 self-center max-[600px]:col-start-1 max-[600px]:row-span-1 max-[600px]:row-start-auto">
          <RoundNode
            title="Address"
            kind="drafting"
            tile="--doc-5"
            line={<AgentLine agent="Claude" command="/address" />}
          >
            {props.address}
          </RoundNode>
        </div>
      </div>
    </fieldset>
  );
}

function RoundsLegend({ rounds }: { rounds: number }): JSX.Element {
  return (
    <span className="absolute -top-[11px] left-3.5 inline-flex h-[22px] items-center gap-1.5 rounded-full border border-hairline bg-card px-2.5 text-xs font-medium text-muted-foreground">
      <Repeat aria-hidden="true" className="size-3" />
      {roundsLegend(rounds)}
    </span>
  );
}

function AgentLine({ agent, command }: { agent: string; command: string }): JSX.Element {
  return (
    <>
      {agent} <code className="font-mono text-[11.5px] text-foreground">{command}</code>
    </>
  );
}

function RoundNode({
  title,
  kind,
  tile,
  line,
  background = false,
  children,
}: {
  title: string;
  kind: ActivityKind;
  /** The `--doc-n` token that tints the icon tile. */
  tile: string;
  line: ReactNode;
  background?: boolean;
  children: ReactNode;
}): JSX.Element {
  return (
    <section
      aria-label={title}
      className="grid h-full min-w-0 content-start gap-2 rounded-xl border border-border bg-card px-3 py-2.5"
    >
      <div className="flex items-center gap-2">
        <span
          aria-hidden="true"
          className="inline-flex size-[26px] shrink-0 items-center justify-center rounded-lg bg-[color-mix(in_oklch,var(--tile)_16%,transparent)] text-[13px] text-(--tile) dark:bg-[color-mix(in_oklch,var(--tile)_20%,transparent)]"
          style={{ "--tile": `var(${tile})` } as CSSProperties}
        >
          <ActivityGlyph kind={kind} paused className="size-[15px]" />
        </span>
        <span className="text-[13px] font-semibold">{title}</span>
        {background && (
          <span className="ml-auto rounded-full border border-hairline px-2 py-0.5 text-[11px] text-muted-foreground">
            background
          </span>
        )}
      </div>
      <p className="text-xs text-muted-foreground">{line}</p>
      <div className="min-w-0">{children}</div>
    </section>
  );
}

// Two beziers from the reviews' rows merge into one arrowhead that points at Address.
function Wires(): JSX.Element {
  return (
    <svg
      aria-hidden="true"
      focusable="false"
      viewBox="0 0 46 100"
      preserveAspectRatio="none"
      className="col-start-2 row-span-2 row-start-1 h-full w-full overflow-visible text-muted-foreground max-[600px]:hidden"
      fill="none"
      stroke="currentColor"
      strokeWidth={1.5}
      opacity={0.7}
    >
      <path d="M0 25C22 25 20 50 40 50" vectorEffect="non-scaling-stroke" />
      <path d="M0 75C22 75 20 50 40 50" vectorEffect="non-scaling-stroke" />
      <path d="M35 45L42 50L35 55" vectorEffect="non-scaling-stroke" strokeLinecap="round" />
    </svg>
  );
}
