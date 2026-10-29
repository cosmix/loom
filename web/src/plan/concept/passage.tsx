import { cn } from "cn";
import { FileText } from "lucide-react";
import { useMemo, type JSX } from "react";
import { Link } from "react-router";

import type { SourceAnchor } from "@/plan/api/schema-concept";
import type { DocView } from "@/plan/api/schema-files";
import { toAddress } from "@/plan/files/address";
import { Markdown } from "@/plan/markdown";

import { passageKey, useColumn } from "./column-context";
import { FLASH_CLASS, useFlashOnChange } from "./flash";
import { anchorKey } from "./open-state";
import { useKeptDoc } from "./passage-doc";
import { passageOf } from "./passages";

type Passage = NonNullable<ReturnType<typeof passageOf>>;

/** The address file mode opens, carrying the passage's lines so it can highlight them. */
export function passageHref(anchor: SourceAnchor, lines: { start: number; end: number } | null) {
  const query = lines ? `?lines=${lines.start}-${lines.end}` : "";
  const hash = anchor.anchor ? `#${encodeURIComponent(anchor.anchor)}` : "";
  return `${toAddress(anchor.path)}${query}${hash}`;
}

function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** The passage body through file mode's renderer, as an excerpt of its document. */
function PassageBody({
  anchor,
  passage,
  doc,
}: {
  anchor: SourceAnchor;
  passage: Passage;
  doc: DocView;
}): JSX.Element {
  const column = useColumn();
  const anchors = useMemo(() => new Set(doc.sections.map((item) => item.anchor)), [doc]);
  return (
    <Markdown
      source={passage.markdown}
      path={anchor.path}
      sections={doc.sections}
      onNavigate={column.openAddress}
      docPaths={column.docPaths}
      anchorsOf={(path) => (path === anchor.path ? anchors : null)}
      lineOffset={passage.startLine - 1}
      headingIds={false}
    />
  );
}

function missingText(failed: boolean, doc: DocView | null): string {
  if (failed) return "This file could not be loaded.";
  return doc ? "This section no longer exists." : "Loading";
}

/**
 * One source passage: a heading section of a file under `doc/`, flowing inline in the opened
 * run after a chip naming its file and heading, with no card or border. Its body goes through
 * file mode's Markdown renderer as an excerpt (line offset, no heading ids), so a cut never
 * claims another section's id.
 */
export function PassageBlock({
  parent,
  sentenceId,
  anchor,
}: {
  /** The parent of the run showing the passage: `sentenceId` may be open under two citers. */
  parent: string | null;
  sentenceId: string;
  anchor: SourceAnchor;
}): JSX.Element {
  const { doc, failed } = useKeptDoc(anchor.path);
  const passage = useMemo(() => (doc ? passageOf(doc, anchor.anchor) : null), [doc, anchor.anchor]);
  const flashing = useFlashOnChange(passage?.hash ?? null);
  const key = anchorKey(anchor);
  const lines = passage ? { start: passage.startLine, end: passage.endLine } : null;
  return (
    <div
      className={cn("concept-passage", flashing && FLASH_CLASS)}
      data-focus={passageKey(parent, sentenceId, key)}
      data-passage={key}
      data-hash={doc?.hash}
      data-start-line={lines?.start}
      data-end-line={lines?.end}
      tabIndex={-1}
    >
      <Link tabIndex={-1} className="concept-passage-chip" to={passageHref(anchor, lines)}>
        <FileText aria-hidden="true" size={12} />
        <span className="concept-passage-path">{fileName(anchor.path)}</span>
        {" › "}
        {passage?.title ?? anchor.anchor}
      </Link>
      {passage && doc ? (
        <PassageBody anchor={anchor} passage={passage} doc={doc} />
      ) : (
        <p className="concept-muted">{missingText(failed, doc)}</p>
      )}
    </div>
  );
}
