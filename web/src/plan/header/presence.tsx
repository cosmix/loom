import { useAtomValue } from "jotai/react";
import { UsersIcon } from "lucide-react";
import type { CSSProperties } from "react";

import type { HolderView, PresenceEntry } from "@/plan/api/schema";
import { Untrusted } from "@/plan/chat/visible-controls";
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

const MAX_AVATARS = 3;

/** Up to two initials from letters and digits only, so a hostile name cannot inject marks. */
function initials(name: string): string {
  const marks = name
    .split(/\s+/)
    .map((word) => Array.from(word).find((char) => /[\p{L}\p{N}]/u.test(char)) ?? "")
    .filter((mark) => mark !== "");
  return (marks.length > 1 ? marks[0]! + marks[1]! : (marks[0] ?? "?")).toUpperCase();
}

/** A stable hue per viewer, so the same person keeps the same colour across reloads. */
function hueOf(publicId: string): number {
  let hash = 0;
  for (const char of publicId) hash = (hash * 31 + (char.codePointAt(0) ?? 0)) >>> 0;
  return hash % 360;
}

function Avatar({ entry }: { entry: PresenceEntry }) {
  return (
    <span
      className="plan-avatar"
      data-holder={entry.holder ? "" : undefined}
      style={{ "--avatar-hue": hueOf(entry.public_id) } as CSSProperties}
      aria-hidden="true"
    >
      {initials(entry.name)}
    </span>
  );
}

function AvatarStack({ entries }: { entries: PresenceEntry[] }) {
  if (entries.length === 0) {
    return (
      <span className="plan-avatar plan-avatar-more" aria-hidden="true">
        <UsersIcon className="size-3.5" />
      </span>
    );
  }
  // The authoring holder leads, so its ring is never hidden behind the "+N" circle.
  const ordered = [...entries].sort((a, b) => Number(b.holder) - Number(a.holder));
  const extra = ordered.length - MAX_AVATARS;
  return (
    <>
      {ordered.slice(0, MAX_AVATARS).map((entry) => (
        <Avatar key={entry.public_id} entry={entry} />
      ))}
      {extra > 0 && (
        <span className="plan-avatar plan-avatar-more" aria-hidden="true">
          +{extra}
        </span>
      )}
    </>
  );
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
    <li className="flex flex-col rounded-md px-1.5 py-1">
      <span className="flex flex-wrap items-baseline gap-x-1.5 text-[13px] font-medium">
        <Untrusted text={entry.name} />
        {you && <span className="text-muted-foreground">(you)</span>}
        {entry.holder && <span className="text-(--tone-executing)">authoring</span>}
        {entry.away && (
          <span className="text-(--tone-warning)">
            away
            {frees && (
              <>
                , frees at <Untrusted text={frees} />
              </>
            )}
          </span>
        )}
      </span>
      {entry.reading && (
        <span className="truncate text-xs text-muted-foreground">
          <Untrusted text={entry.reading} />
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
        <button
          type="button"
          className="plan-avatars"
          aria-label={`People here: ${entries.length}`}
        >
          <AvatarStack entries={entries} />
        </button>
      </PopoverTrigger>
      <PopoverContent aria-label="People here">
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
