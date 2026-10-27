import { useAtomValue } from "jotai/react";
import {
  ArchiveIcon,
  CheckIcon,
  ChevronDownIcon,
  ChevronRightIcon,
  FileTextIcon,
  LockIcon,
  PlayIcon,
  type LucideIcon,
} from "lucide-react";
import { useEffect, useState, type JSX } from "react";
import { Link } from "react-router";

import { cn } from "cn";
import type { PlanStateView, TreeEntry } from "@/plan/api/schema-files";
import { toAddress } from "@/plan/files/address";
import { openDocAtom, validationAtom } from "@/plan/files/atoms";
import { Outline } from "@/plan/files/outline";
import { TreeEntryExtras } from "@/plan/files/tree-entry-extras";

const STATES: Record<PlanStateView, { icon: LucideIcon; tone: string }> = {
  written: { icon: FileTextIcon, tone: "text-muted-foreground" },
  running: { icon: PlayIcon, tone: "text-(--tone-executing)" },
  done: { icon: CheckIcon, tone: "text-(--tone-completed)" },
  retired: { icon: ArchiveIcon, tone: "text-(--tone-dimmed)" },
};

export function gapAddress(title: string): string {
  return `/files?gap=${encodeURIComponent(title)}`;
}

export function containsPath(entry: TreeEntry, path: string | null): boolean {
  if (path === null) return false;
  return entry.path === path || entry.children.some((child) => containsPath(child, path));
}

function PlanMeta({ entry }: { entry: TreeEntry }): JSX.Element | null {
  const validation = useAtomValue(validationAtom);
  if (entry.kind !== "plan" || entry.state === null) return null;
  const { icon: Icon, tone } = STATES[entry.state];
  const live = entry.path ? validation[entry.path] : undefined;
  const errors = live ? live.errors.length : entry.errors;
  return (
    <span className="flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
      <span className={cn("flex items-center gap-1", tone)}>
        <Icon className="size-3" aria-hidden="true" />
        {entry.state}
      </span>
      {entry.pressure_date && <span>Pressure-tested {entry.pressure_date}</span>}
      {errors > 0 && (
        <span className="text-(--tone-blocked)">
          {errors === 1 ? "1 error" : `${errors} errors`}
        </span>
      )}
    </span>
  );
}

function Title({ entry, current }: { entry: TreeEntry; current: boolean }): JSX.Element {
  const className = cn("min-w-0 truncate text-[13px] font-medium", current && "text-foreground");
  if (entry.gap) {
    return (
      <Link to={gapAddress(entry.title)} className={cn(className, "text-muted-foreground")}>
        {entry.title}
      </Link>
    );
  }
  if (entry.path === null) return <span className={className}>{entry.title}</span>;
  return (
    <Link
      to={toAddress(entry.path)}
      title={entry.file_name ?? undefined}
      aria-current={current ? "page" : undefined}
      className={className}
    >
      {entry.title}
    </Link>
  );
}

type Props = { entry: TreeEntry; forceOpen: boolean };

/** One tree row with its metadata, brief bundles or topics nested beneath. */
export function TreeEntryRow({ entry, forceOpen }: Props): JSX.Element {
  const open = useAtomValue(openDocAtom)?.doc ?? null;
  const holdsOpen = containsPath(entry, open?.path ?? null);
  const expandable = entry.children.length > 0;
  const [expanded, setExpanded] = useState(expandable && !entry.kind.endsWith("_folder"));
  useEffect(() => {
    if (holdsOpen) setExpanded(true);
  }, [holdsOpen]);
  const shown = expandable && (expanded || forceOpen);
  const Chevron = shown ? ChevronDownIcon : ChevronRightIcon;
  const current = open !== null && entry.path === open.path;

  return (
    <li className={cn(entry.gap && "opacity-70")}>
      <div className="flex items-center gap-1 px-2 py-1">
        {expandable ? (
          <button
            type="button"
            aria-expanded={shown}
            aria-label={`${shown ? "Collapse" : "Expand"} ${entry.title}`}
            className="shrink-0 text-muted-foreground"
            onClick={() => setExpanded(!shown)}
          >
            <Chevron className="size-3.5" aria-hidden="true" />
          </button>
        ) : (
          <span className="w-3.5 shrink-0" aria-hidden="true" />
        )}
        <Title entry={entry} current={current} />
        {entry.gap && <span className="text-xs text-muted-foreground">missing</span>}
        {entry.locked && (
          <LockIcon className="size-3 shrink-0 text-muted-foreground" aria-label="Locked" />
        )}
        <TreeEntryExtras entry={entry} />
      </div>
      <div className="flex flex-col gap-0.5 pr-2 pl-7">
        <PlanMeta entry={entry} />
        {entry.blurb && <span className="text-xs text-muted-foreground">{entry.blurb}</span>}
      </div>
      {current && open && <Outline path={open.path} sections={open.sections} />}
      {shown && (
        <ul className="ml-4 border-l border-hairline">
          {entry.children.map((child) => (
            <TreeEntryRow key={child.path ?? child.title} entry={child} forceOpen={forceOpen} />
          ))}
        </ul>
      )}
    </li>
  );
}
