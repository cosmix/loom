import { useAtomValue } from "jotai/react";
import { ChevronDownIcon, ChevronRightIcon } from "lucide-react";
import type { Nodes } from "mdast";
import { fromMarkdown } from "mdast-util-from-markdown";
import { useEffect, useState, type JSX } from "react";
import { Link } from "react-router";

import { cn } from "cn";
import type { TreeEntry } from "@/plan/api/schema-files";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { toAddress } from "@/plan/files/address";
import { openDocAtom, validationAtom } from "@/plan/files/atoms";
import { Outline } from "@/plan/files/outline";
import { TreeEntryExtras } from "@/plan/files/tree-entry-extras";
import { DocMark } from "@/plan/ui/doc-mark";
import { OTHER_GLYPHS, PLAN_STATE_GLYPHS } from "@/plan/ui/glyphs";

const ReadOnly = OTHER_GLYPHS.readOnly;

function EntryMark({ kind }: Pick<TreeEntry, "kind">): JSX.Element | null {
  switch (kind) {
    case "vision":
    case "section":
    case "note":
    case "metaplan":
    case "plan":
    case "brief":
    case "knowledge":
    case "review":
      return <DocMark kind={kind} />;
    default:
      return null;
  }
}

export function gapAddress(
  title: string,
  feature: string | null = null,
  target: string | null = null,
): string {
  let address = `/files?gap=${encodeURIComponent(title)}`;
  if (feature !== null) address += `&feature=${encodeURIComponent(feature)}`;
  if (target !== null) address += `&target=${encodeURIComponent(target)}`;
  return address;
}

export function containsPath(entry: TreeEntry, path: string | null): boolean {
  if (path === null) return false;
  return entry.path === path || entry.children.some((child) => containsPath(child, path));
}

const STATE_TONES = {
  running: "text-(--tone-executing)",
  done: "text-(--tone-completed)",
  retired: "text-(--tone-dimmed)",
} as const;

/** A plan's state: a fixed-width right-aligned label that stays on the name's line. */
function StateLabel({ entry }: { entry: TreeEntry }): JSX.Element | null {
  if (entry.kind !== "plan" || entry.state === null) return null;
  const Icon = PLAN_STATE_GLYPHS[entry.state];
  return (
    <span
      className={cn(
        "flex w-[5.25rem] shrink-0 items-center justify-end gap-1 text-xs text-muted-foreground",
        entry.state !== "written" && STATE_TONES[entry.state],
      )}
    >
      <Icon className="size-3" aria-hidden="true" />
      {entry.state}
    </span>
  );
}

/** The plan's pressure date and error count, under the name. */
function PlanMeta({ entry }: { entry: TreeEntry }): JSX.Element | null {
  const validation = useAtomValue(validationAtom);
  if (entry.kind !== "plan" || entry.state === null) return null;
  const live = entry.path ? validation[entry.path] : undefined;
  const errors = live ? live.errors.length : entry.errors;
  if (!entry.pressure_date && errors === 0) return null;
  return (
    <span className="flex flex-wrap items-center gap-x-2 text-xs text-muted-foreground">
      {entry.pressure_date && <span>Pressure-tested {entry.pressure_date}</span>}
      {errors > 0 && (
        <span className="text-(--tone-blocked)">
          {errors === 1 ? "1 error" : `${errors} errors`}
        </span>
      )}
    </span>
  );
}

export function plainTitle(text: string): string {
  const textOf = (node: Nodes): string => {
    if (node.type === "definition") return "";
    if (node.type === "break") return " ";
    if ("children" in node) return node.children.map(textOf).join("");
    if ("value" in node) return node.value;
    if ("alt" in node) return node.alt ?? "";
    return "";
  };
  return fromMarkdown(text).children.map(textOf).join(" ");
}

function Title({ entry, current }: { entry: TreeEntry; current: boolean }): JSX.Element {
  const className = cn(
    "max-w-full min-w-[8ch] flex-1 basis-0 truncate text-[13px] font-medium",
    current && "text-foreground",
  );
  const title = (
    <span style={{ unicodeBidi: "isolate", direction: "ltr" }}>
      <SentenceText text={entry.title} />
    </span>
  );
  if (entry.gap) {
    return (
      <Link
        to={gapAddress(entry.title, entry.feature, entry.target)}
        className={cn(className, "text-muted-foreground")}
      >
        {title}
      </Link>
    );
  }
  if (entry.path === null) return <span className={className}>{title}</span>;
  return (
    <Link
      to={toAddress(entry.path)}
      title={entry.file_name ? visibleControls(entry.file_name) : undefined}
      aria-current={current ? "page" : undefined}
      className={className}
    >
      {title}
    </Link>
  );
}

type Props = { entry: TreeEntry; forceOpen: boolean };

function EntryLabel({ entry, current }: { entry: TreeEntry; current: boolean }): JSX.Element {
  return (
    <div className="flex min-w-0 flex-1 flex-nowrap items-center gap-1">
      <Title entry={entry} current={current} />
      {entry.gap && <span className="text-xs text-muted-foreground">missing</span>}
      {entry.locked && (
        <ReadOnly className="size-3 shrink-0 text-muted-foreground" aria-label="Locked" />
      )}
      <StateLabel entry={entry} />
      <TreeEntryExtras entry={entry} />
    </div>
  );
}

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
      <div className="flex items-start gap-1 px-2 py-1">
        {expandable ? (
          <button
            type="button"
            aria-expanded={shown}
            aria-label={`${shown ? "Collapse" : "Expand"} ${visibleControls(plainTitle(entry.title))}`}
            className="shrink-0 text-muted-foreground"
            onClick={() => setExpanded(!shown)}
          >
            <Chevron className="size-3.5" aria-hidden="true" />
          </button>
        ) : (
          <span className="w-3.5 shrink-0" aria-hidden="true" />
        )}
        <EntryMark kind={entry.kind} />
        <EntryLabel entry={entry} current={current} />
      </div>
      <div className="flex flex-col gap-0.5 pr-2 pl-7">
        <PlanMeta entry={entry} />
        {entry.blurb && (
          <span className="text-xs text-muted-foreground">
            <Untrusted text={entry.blurb} />
          </span>
        )}
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
