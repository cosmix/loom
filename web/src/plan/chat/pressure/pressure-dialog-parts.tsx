import { cn } from "cn";
import { Clock, FilePen, FilePlus, TriangleAlert } from "lucide-react";
import { useId, type JSX, type ReactNode } from "react";

import { DialogDescription, DialogTitle } from "@/components/ui/dialog";
import { OTHER_GLYPHS } from "@/plan/ui/glyphs";

import { Untrusted } from "../visible-controls";

const PressureMark = OTHER_GLYPHS.pressureTested;

export function PressureHero(): JSX.Element {
  return (
    <div className="grid grid-cols-[auto_minmax(0,1fr)_auto] items-center gap-3.5 border-b border-hairline bg-[radial-gradient(120%_160%_at_0%_0%,color-mix(in_oklch,var(--primary)_16%,var(--card))_0%,var(--card)_62%)] px-5 pt-[18px] pb-4">
      <span className="flex size-11 shrink-0 items-center justify-center rounded-[12px] bg-[color-mix(in_oklch,var(--primary)_28%,transparent)] text-primary">
        {/* The dialog's mark, not work in progress: the pressure-tested flask, still. */}
        <PressureMark className="size-6" aria-hidden="true" />
      </span>
      <div className="min-w-0 space-y-0.5">
        <DialogTitle className="text-[17px] font-[650]">Pressure-test a plan</DialogTitle>
        <DialogDescription className="text-[13px]">
          Two reviewers read the plan at once, then an editor addresses both reviews.
        </DialogDescription>
      </div>
      {/* Room for the dialog's close button, which sits over the hero's right edge. */}
      <span aria-hidden="true" className="w-7" />
    </div>
  );
}

const ROUND_CHOICES = [1, 2, 3] as const;

/** The label row of "What happens": the section name, then Rounds with its 1, 2, 3 pill group. */
export function PressureRounds({
  rounds,
  setRounds,
}: {
  rounds: number;
  setRounds: (rounds: number) => void;
}): JSX.Element {
  const labelId = useId();
  return (
    <div className="flex flex-wrap items-center justify-between gap-2">
      <h3 className="text-sm font-semibold">What happens</h3>
      <div className="flex items-center gap-2">
        <span id={labelId} className="text-[13px] font-medium text-muted-foreground">
          Rounds
        </span>
        <div
          role="group"
          aria-labelledby={labelId}
          className="flex rounded-full bg-background p-[3px] ring-1 ring-hairline"
        >
          {ROUND_CHOICES.map((value) => (
            <button
              key={value}
              type="button"
              aria-pressed={rounds === value}
              onClick={() => setRounds(value)}
              className={cn(
                "h-[26px] w-[30px] rounded-full text-[13px] font-medium focus-visible:outline-2 focus-visible:outline-ring",
                rounds === value
                  ? "bg-primary text-primary-foreground"
                  : "text-muted-foreground hover:bg-muted hover:text-foreground",
              )}
            >
              {value}
            </button>
          ))}
        </div>
      </div>
    </div>
  );
}

function Fact({
  icon,
  className,
  children,
}: {
  icon: ReactNode;
  className?: string;
  children: ReactNode;
}): JSX.Element {
  return (
    <li className={cn("flex items-start gap-2 text-[13px] text-muted-foreground", className)}>
      <span aria-hidden="true" className="mt-0.5 inline-flex shrink-0 [&_svg]:size-3.5">
        {icon}
      </span>
      <span className="min-w-0 [overflow-wrap:anywhere]">{children}</span>
    </li>
  );
}

/**
 * What the run changes. The time fact names no minutes: the server sends no duration to quote.
 * `chatName` is the open authoring chat that the run ends first, or null when none is open.
 */
export function PressureConsequences({
  rounds,
  chatName,
}: {
  rounds: number;
  chatName: string | null;
}): JSX.Element {
  const chosenRounds = `${rounds} ${rounds === 1 ? "round" : "rounds"}`;
  return (
    <ul aria-label="Consequences" className="grid gap-x-6 gap-y-2.5 sm:grid-cols-2">
      <Fact icon={<Clock />}>
        <strong className="font-semibold text-foreground">Takes time. </strong>
        <span>
          This takes a while. Time grows with the {chosenRounds}, each round running two reviews
          then an address step.
        </span>
      </Fact>
      <Fact icon={<FilePen />}>
        <strong className="font-semibold text-foreground">Edits</strong> the plan and its brief
        bundle
      </Fact>
      <Fact icon={<FilePlus />}>
        <span>
          <strong className="font-semibold text-foreground">Writes</strong>{" "}
          <code className="font-mono text-[13px]">planning-omissions.md</code>
        </span>
      </Fact>
      {chatName !== null && (
        <Fact
          icon={<TriangleAlert className="text-(--tone-warning)" />}
          className="tone-warning text-foreground"
        >
          <strong className="font-semibold">Ends</strong> &lsquo;
          <Untrusted text={chatName} />
          &rsquo; first, with its closing write
        </Fact>
      )}
    </ul>
  );
}
