import { cn } from "cn";

import { StateBadge, toneClass } from "@/components/state-badge";
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog";
import { Kbd } from "@/components/ui/kbd";
import { Separator } from "@/components/ui/separator";
import type { Tone } from "@/lib/format";
import { LEGEND, type StageStateMeta } from "@/lib/states";

/// The activity column's vocabulary (`legend.rs` `activity_lines`, plus the
/// `crashed` cell from `cells.rs`).
const ACTIVITY: { word: string; tone: Tone; meaning: string }[] = [
  { word: "working", tone: "completed", meaning: "the session used a tool recently" },
  { word: "idle", tone: "dimmed", meaning: "alive, nothing happening" },
  { word: "stale", tone: "warning", meaning: "no heartbeat for 5 min" },
  { word: "orphaned", tone: "blocked", meaning: "executing with no session record" },
  { word: "crashed", tone: "blocked", meaning: "the session process died" },
];

/// Where `legend.rs` draws its "needs you" rule: after the seventh state.
const NEEDS_YOU_FROM = 7;

/// The thirteen stage states and the activity words, from the shared table.
export function LegendDialog({
  open,
  onOpenChange,
}: {
  open: boolean;
  onOpenChange: (open: boolean) => void;
}) {
  return (
    <Dialog open={open} onOpenChange={onOpenChange}>
      <DialogContent className="sm:max-w-2xl">
        <DialogHeader>
          <DialogTitle>Stage states</DialogTitle>
          <DialogDescription>
            Press <Kbd>?</Kbd> to toggle this legend, <Kbd>Esc</Kbd> to close it.
          </DialogDescription>
        </DialogHeader>
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1.5 text-sm">
          {LEGEND.slice(0, NEEDS_YOU_FROM).map((entry) => (
            <LegendRow key={entry.status} entry={entry} />
          ))}
          <div className={cn("col-span-2 mt-2 eyebrow", toneClass("blocked"))}>needs you</div>
          {LEGEND.slice(NEEDS_YOU_FROM).map((entry) => (
            <LegendRow key={entry.status} entry={entry} />
          ))}
        </dl>
        <Separator />
        <dl className="grid grid-cols-[auto_1fr] gap-x-4 gap-y-1 text-sm">
          <dt className="eyebrow col-span-2">activity</dt>
          {ACTIVITY.map(({ word, tone, meaning }) => (
            <ActivityRow key={word} word={word} tone={tone} meaning={meaning} />
          ))}
          <dt className="eyebrow col-span-2 mt-2">context</dt>
          <dd className="col-span-2 text-muted-foreground">
            resident tokens of the stage's session against its ceiling
          </dd>
        </dl>
      </DialogContent>
    </Dialog>
  );
}

function LegendRow({ entry }: { entry: StageStateMeta }) {
  return (
    <div className="contents">
      <dt>
        <StateBadge status={entry.status} />
      </dt>
      <dd className="text-muted-foreground">{entry.legend}</dd>
    </div>
  );
}

function ActivityRow({ word, tone, meaning }: { word: string; tone: Tone; meaning: string }) {
  return (
    <>
      <dt className={cn("font-medium", toneClass(tone))}>{word}</dt>
      <dd className="text-muted-foreground">{meaning}</dd>
    </>
  );
}
