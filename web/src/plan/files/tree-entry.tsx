import { useAtomValue } from "jotai/react";
import { ChevronRightIcon, CircleCheckBig, CircleDashed, File, Folder } from "lucide-react";
import type { Nodes } from "mdast";
import { fromMarkdown } from "mdast-util-from-markdown";
import { useEffect, useState, type JSX } from "react";
import { Link } from "react-router";

import type { TreeEntry } from "@/plan/api/schema-files";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { SentenceText } from "@/plan/concept/sentence-text";
import { toAddress } from "@/plan/files/address";
import { openDocAtom } from "@/plan/files/atoms";
import { Outline } from "@/plan/files/outline";
import { EntrySignals } from "@/plan/files/tree-entry-signals";
import { arrangeChildren, featureCount, entryNumber, shortTitle } from "@/plan/files/tree-model";
import { FeatureMark } from "@/plan/home/feature-mark";
import { DocMark } from "@/plan/ui/doc-mark";

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

/** Open or closed, starting at `initial`; it opens by itself once it holds the open document. */
export function useDisclosure(initial: boolean, holdsOpen: boolean) {
  const [expanded, setExpanded] = useState(initial || holdsOpen);
  useEffect(() => {
    if (holdsOpen) setExpanded(true);
  }, [holdsOpen]);
  return [expanded, setExpanded] as const;
}

/** The chevron column: a toggle for a row with children, an empty slot of the same width else. */
export function Twist({
  shown,
  label,
  onToggle,
}: {
  shown: boolean;
  label: string | null;
  onToggle: () => void;
}): JSX.Element {
  if (label === null) return <span className="plan-tree__twist" aria-hidden="true" />;
  return (
    <button
      type="button"
      aria-expanded={shown}
      aria-label={`${shown ? "Collapse" : "Expand"} ${label}`}
      className="plan-tree__twist"
      onClick={onToggle}
    >
      <ChevronRightIcon aria-hidden="true" />
    </button>
  );
}

/** What a row is: a document's coloured mark, a feature's mark, a folder, or a dashed gap. */
function EntryMark({ entry }: { entry: TreeEntry }): JSX.Element {
  if (entry.gap) return <CircleDashed className="plan-tree__mark" aria-hidden="true" />;
  switch (entry.kind) {
    case "feature":
      return <FeatureMark slug={entry.feature ?? entry.title} />;
    case "brief_folder":
    case "knowledge_folder":
      return <Folder className="plan-tree__mark" aria-hidden="true" />;
    case "other":
    case "metaplan_entry":
      return <File className="plan-tree__mark" aria-hidden="true" />;
    default:
      return <DocMark kind={entry.kind} className="size-4 rounded-[4px]" />;
  }
}

/** The full title and the file name, for the row's tooltip: the row shows a short title. */
function tooltipOf(entry: TreeEntry): string {
  const full = visibleControls(plainTitle(entry.title));
  return entry.file_name ? `${full}\n${visibleControls(entry.file_name)}` : full;
}

/** The row's link (or label, for a row with no document): its mark, number and short title. */
function EntryLabel({ entry, current }: { entry: TreeEntry; current: boolean }): JSX.Element {
  const number = entryNumber(entry);
  const body = (
    <>
      <EntryMark entry={entry} />
      {number !== null && (
        <>
          <span className="plan-tree__number">
            <Untrusted text={number} />
          </span>{" "}
        </>
      )}
      <span className="plan-tree__title" style={{ unicodeBidi: "isolate", direction: "ltr" }}>
        <SentenceText text={shortTitle(entry)} />
      </span>
    </>
  );
  const props = { className: "plan-tree__label", title: tooltipOf(entry) };
  if (entry.gap) {
    return (
      <Link to={gapAddress(entry.title, entry.feature, entry.target)} {...props}>
        {body}
      </Link>
    );
  }
  if (entry.path === null) return <span {...props}>{body}</span>;
  return (
    <Link to={toAddress(entry.path)} aria-current={current ? "page" : undefined} {...props}>
      {body}
    </Link>
  );
}

/** A feature's plan count, finished over listed, at the end of its row. */
function FeatureCount({ entry }: { entry: TreeEntry }): JSX.Element | null {
  const count = featureCount(entry);
  if (count === null) return null;
  return (
    <span className="plan-tree__count" title={count.spoken}>
      <span aria-hidden="true">{count.text}</span>
      <span className="sr-only">{count.spoken}</span>
    </span>
  );
}

type Props = { entry: TreeEntry; forceOpen: boolean };

/** The plans whose work is over, folded under one row that opens to list them. */
function FinishedPlans({ entries, forceOpen }: { entries: TreeEntry[]; forceOpen: boolean }) {
  const open = useAtomValue(openDocAtom)?.doc.path ?? null;
  const holdsOpen = entries.some((entry) => containsPath(entry, open));
  const [expanded, setExpanded] = useDisclosure(false, holdsOpen);
  const shown = expanded || forceOpen;
  // The whole row is the one toggle: it names no document, so there is nothing else to open.
  return (
    <li className="plan-tree__item">
      <button
        type="button"
        aria-expanded={shown}
        className="plan-tree__row plan-tree__fold"
        onClick={() => setExpanded(!shown)}
      >
        <span className="plan-tree__twist" aria-hidden="true">
          <ChevronRightIcon />
        </span>
        <span className="plan-tree__label">
          <CircleCheckBig className="plan-tree__mark" data-tone="done" aria-hidden="true" />
          <span className="plan-tree__title">Finished plans</span>
        </span>{" "}
        <span className="plan-tree__count">{entries.length}</span>
      </button>
      {shown && <Children rows={entries} finished={[]} forceOpen={forceOpen} />}
    </li>
  );
}

function Children({
  rows,
  finished,
  forceOpen,
}: {
  rows: TreeEntry[];
  finished: TreeEntry[];
  forceOpen: boolean;
}): JSX.Element {
  return (
    <ul className="plan-tree__children">
      {rows.map((child) => (
        <TreeEntryRow key={child.path ?? child.title} entry={child} forceOpen={forceOpen} />
      ))}
      {finished.length > 0 && <FinishedPlans entries={finished} forceOpen={forceOpen} />}
    </ul>
  );
}

/**
 * One tree row: chevron, mark, short title and signals on one line, then its blurb, the open
 * document's outline and its children. A row starts closed unless it holds the open document;
 * filtering (`forceOpen`) opens every row.
 */
export function TreeEntryRow({ entry, forceOpen }: Props): JSX.Element {
  const open = useAtomValue(openDocAtom)?.doc ?? null;
  const { rows, finished } = arrangeChildren(entry);
  const expandable = rows.length + finished.length > 0;
  const holdsOpen = containsPath(entry, open?.path ?? null);
  const [expanded, setExpanded] = useDisclosure(false, holdsOpen);
  const shown = expandable && (expanded || forceOpen);
  const current = open !== null && entry.path === open.path;
  return (
    <li className="plan-tree__item">
      <div
        className="plan-tree__row"
        data-kind={entry.kind}
        data-current={current ? "" : undefined}
        data-gap={entry.gap ? "" : undefined}
      >
        <Twist
          shown={shown}
          label={expandable ? visibleControls(plainTitle(entry.title)) : null}
          onToggle={() => setExpanded(!shown)}
        />
        <EntryLabel entry={entry} current={current} />
        <FeatureCount entry={entry} />
        <EntrySignals entry={entry} />
      </div>
      {entry.blurb && (
        <p className="plan-tree__blurb">
          <Untrusted text={entry.blurb} />
        </p>
      )}
      {current && open && (
        <div className="plan-tree__outline">
          <Outline path={open.path} sections={open.sections} />
        </div>
      )}
      {shown && <Children rows={rows} finished={finished} forceOpen={forceOpen} />}
    </li>
  );
}
