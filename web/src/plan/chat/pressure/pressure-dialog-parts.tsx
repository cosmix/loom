import { useId } from "react";

import { Button } from "@/components/ui/button";
import { DialogDescription, DialogHeader, DialogTitle } from "@/components/ui/dialog";
import { ActivityGlyph } from "@/plan/ui/activity-glyphs";
import { DocMark } from "@/plan/ui/doc-mark";

import { Untrusted, visibleControls } from "../visible-controls";

export function PressureHero() {
  return (
    <DialogHeader className="flex-row items-start gap-3 pr-6">
      <span className="flex size-11 shrink-0 items-center justify-center rounded-xl bg-primary/10 text-primary">
        <ActivityGlyph kind="pressure" className="size-6" />
      </span>
      <div className="min-w-0 space-y-1">
        <DialogTitle>Pressure-test a plan</DialogTitle>
        <DialogDescription>
          Two reviewers read the plan at once, then an editor addresses both.
        </DialogDescription>
      </div>
    </DialogHeader>
  );
}

export function PressurePlanCard({
  plan,
  plans,
  setPlan,
}: {
  plan: string;
  plans: readonly string[];
  setPlan: (plan: string) => void;
}) {
  const fileName = plan.split("/").at(-1) ?? plan;
  const title = fileName
    .replace(/^(?:IN_PROGRESS-PLAN-|DONE-PLAN-|PLAN-)/, "")
    .replace(/\.md$/, "")
    .replace(/[-_]+/g, " ");
  return (
    <div className="flex items-center gap-3 rounded-lg border bg-muted/30 p-3">
      <DocMark kind="plan" size="lg" />
      <div className="min-w-0 flex-1 space-y-0.5 [overflow-wrap:anywhere]">
        <p className="font-medium">
          <Untrusted text={title} />
        </p>
        <p className="font-mono text-xs text-muted-foreground">
          <Untrusted text={fileName} />
        </p>
      </div>
      <PlanChoice plan={plan} plans={plans} setPlan={setPlan} />
    </div>
  );
}

function PlanChoice({
  plan,
  plans,
  setPlan,
}: {
  plan: string;
  plans: readonly string[];
  setPlan: (plan: string) => void;
}) {
  const id = useId();
  return (
    <div className="relative shrink-0 rounded-md focus-within:outline-2 focus-within:outline-ring">
      {/* The accessible name contains the visible word "Change" (WCAG 2.5.3). */}
      <label htmlFor={id} className="sr-only">
        Change plan
      </label>
      <span
        aria-hidden="true"
        className="inline-flex h-8 items-center px-2 text-sm underline underline-offset-4"
      >
        Change
      </span>
      <select
        id={id}
        value={plan}
        required
        className="absolute inset-0 size-full cursor-pointer opacity-0"
        onChange={(event) => setPlan(event.currentTarget.value)}
      >
        {plans.map((option) => (
          <option key={option} value={option}>
            {visibleControls(option)}
          </option>
        ))}
      </select>
    </div>
  );
}

export function PressureRounds({
  rounds,
  setRounds,
}: {
  rounds: number;
  setRounds: (rounds: number) => void;
}) {
  const id = useId();
  return (
    <div className="flex flex-wrap items-center gap-3">
      <label htmlFor={id} className="text-sm font-medium">
        Rounds
      </label>
      <div role="group" aria-label="Quick-pick rounds" className="flex rounded-md border p-0.5">
        {[1, 2, 3].map((value) => (
          <Button
            key={value}
            type="button"
            size="sm"
            variant={rounds === value ? "secondary" : "ghost"}
            aria-pressed={rounds === value}
            onClick={() => setRounds(value)}
          >
            {value}
          </Button>
        ))}
      </div>
      <input
        id={id}
        className="h-8 w-16 rounded-md border border-input bg-background px-2 text-sm focus-visible:outline-2 focus-visible:outline-ring"
        name="rounds"
        type="number"
        min={1}
        max={10}
        step={1}
        required
        value={Number.isNaN(rounds) ? "" : rounds}
        onChange={(event) => setRounds(event.currentTarget.valueAsNumber)}
      />
    </div>
  );
}

export function PressureConsequences({ rounds, endsChat }: { rounds: number; endsChat: boolean }) {
  const chosenRounds = Number.isFinite(rounds)
    ? `${rounds} ${rounds === 1 ? "round" : "rounds"}`
    : "chosen rounds";
  return (
    <ul
      aria-label="Consequences"
      className="grid gap-x-6 gap-y-4 border-t pt-4 text-sm sm:grid-cols-2"
    >
      <li className="space-y-1">
        <p className="font-medium">Takes time</p>
        <p className="text-muted-foreground">
          This takes a while. Time grows with the {chosenRounds}, each round running two reviews
          then an address step.
        </p>
      </li>
      <li className="space-y-1">
        <p className="font-medium">Edits planning documents</p>
        <p className="text-muted-foreground">Edits the plan and its briefs.</p>
      </li>
      <li className="space-y-1">
        <p className="font-medium">Writes the omissions report</p>
        <p className="text-muted-foreground">
          Writes <span className="font-mono text-xs">planning-omissions.md</span> at the repository
          root.
        </p>
      </li>
      {endsChat && (
        <li className="tone-warning space-y-1">
          <p className="font-medium">Ends the open chat</p>
          <p>The open chat ends first, with its closing write.</p>
        </li>
      )}
    </ul>
  );
}
