import { useAtomValue } from "jotai/react";
import { UsersIcon } from "lucide-react";

import { Button } from "@/components/ui/button";
import type { HolderView, PresenceEntry } from "@/plan/api/schema";
import { NamePrompt } from "@/plan/header/name-prompt";
import { Popover, PopoverContent, PopoverTrigger } from "@/plan/header/popover";
import { authoringAtom, presenceAtom, youAtom } from "@/plan/state/atoms";

function freeTime(holder: HolderView | null, entry: PresenceEntry): string | null {
  if (!holder || holder.public_id !== entry.public_id || !holder.frees_at) return null;
  const at = new Date(holder.frees_at);
  return Number.isNaN(at.getTime())
    ? holder.frees_at
    : at.toLocaleTimeString([], { hour: "2-digit", minute: "2-digit" });
}

function PresenceRow({
  entry,
  you,
  holder,
}: {
  entry: PresenceEntry;
  you: boolean;
  holder: HolderView | null;
}) {
  const frees = entry.away ? freeTime(holder, entry) : null;
  return (
    <li
      tabIndex={0}
      className="plan-presence-row flex flex-col rounded-md px-1.5 py-1 outline-none"
    >
      <span className="flex flex-wrap items-baseline gap-x-1.5 text-[13px] font-medium">
        <span>{entry.name}</span>
        {you && <span className="text-muted-foreground">(you)</span>}
        {entry.holder && <span className="text-(--tone-executing)">authoring</span>}
        {entry.away && (
          <span className="text-(--tone-warning)">away{frees ? `, frees at ${frees}` : ""}</span>
        )}
      </span>
      {entry.reading && (
        <span className="plan-presence-reading truncate text-xs text-muted-foreground">
          {entry.reading}
        </span>
      )}
    </li>
  );
}

export function Presence() {
  const presence = useAtomValue(presenceAtom);
  const you = useAtomValue(youAtom);
  const holder = useAtomValue(authoringAtom)?.holder ?? null;
  const entries = presence?.entries ?? [];
  return (
    <Popover>
      <PopoverTrigger asChild>
        <Button variant="ghost" size="sm" aria-label={`People here: ${entries.length}`}>
          <UsersIcon aria-hidden="true" />
          <span>{entries.length}</span>
        </Button>
      </PopoverTrigger>
      <PopoverContent>
        <NamePrompt />
        <ul aria-label="People here" className="flex flex-col gap-0.5">
          {entries.map((entry) => (
            <PresenceRow
              key={entry.public_id}
              entry={entry}
              you={entry.public_id === you?.public_id}
              holder={holder}
            />
          ))}
        </ul>
      </PopoverContent>
    </Popover>
  );
}
