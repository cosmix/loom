import { cn } from "cn";
import { useAtomValue } from "jotai/react";
import { loadable } from "jotai/utils";
import { useEffect, useMemo, useState, type JSX } from "react";
import { Link } from "react-router";

import type { SourceAnchor } from "@/plan/api/schema-concept";
import type { DocView } from "@/plan/api/schema-files";
import { toAddress } from "@/plan/files/address";
import { Markdown } from "@/plan/markdown";

import { passageKey, useColumn } from "./column-context";
import { FLASH_CLASS, useFlashOnChange } from "./flash";
import { anchorKey } from "./open-state";
import { docViewAtom, passageOf } from "./passages";

const loadables = new Map<string, ReturnType<typeof makeLoadable>>();

function makeLoadable(path: string) {
  return loadable(docViewAtom(path));
}

/** One loadable per document, so every passage of a file shares a single fetch and atom. */
function docLoadable(path: string) {
  let atom = loadables.get(path);
  if (!atom) {
    atom = makeLoadable(path);
    loadables.set(path, atom);
  }
  return atom;
}

/** The address file mode opens, carrying the passage's lines so it can highlight them. */
export function passageHref(anchor: SourceAnchor, lines: { start: number; end: number } | null) {
  const query = lines ? `?lines=${lines.start}-${lines.end}` : "";
  const hash = anchor.anchor ? `#${encodeURIComponent(anchor.anchor)}` : "";
  return `${toAddress(anchor.path)}${query}${hash}`;
}

function fileName(path: string): string {
  return path.slice(path.lastIndexOf("/") + 1);
}

/** The last loaded document, kept while a refetch is in flight so the block never blanks. */
function useKeptDoc(path: string): { doc: DocView | null; failed: boolean } {
  const state = useAtomValue(docLoadable(path));
  const [kept, setKept] = useState<DocView | null>(null);
  const data = state.state === "hasData" ? state.data : null;
  useEffect(() => {
    if (data) setKept(data);
  }, [data]);
  return { doc: data ?? kept, failed: state.state === "hasError" };
}

/**
 * One source passage: a heading section of a file under `doc/`, set as an inset block on the
 * card surface with a hairline border. Its body goes through file mode's Markdown renderer
 * as an excerpt (line offset, no heading ids), so a cut never claims another section's id.
 */
export function PassageBlock({
  sentenceId,
  anchor,
}: {
  sentenceId: string;
  anchor: SourceAnchor;
}): JSX.Element {
  const column = useColumn();
  const { doc, failed } = useKeptDoc(anchor.path);
  const passage = useMemo(() => (doc ? passageOf(doc, anchor.anchor) : null), [doc, anchor.anchor]);
  const flashing = useFlashOnChange(passage?.hash ?? null);
  const key = anchorKey(anchor);
  const lines = passage ? { start: passage.startLine, end: passage.endLine } : null;
  const anchors = useMemo(() => new Set(doc?.sections.map((item) => item.anchor)), [doc]);
  return (
    <div
      className={cn("concept-passage", flashing && FLASH_CLASS)}
      data-focus={passageKey(sentenceId, key)}
      data-passage={key}
      data-hash={doc?.hash}
      data-start-line={lines?.start}
      data-end-line={lines?.end}
      tabIndex={-1}
    >
      <Link tabIndex={-1} className="concept-passage-head" to={passageHref(anchor, lines)}>
        <span className="concept-passage-path">{fileName(anchor.path)}</span>
        {" › "}
        {passage?.title ?? anchor.anchor}
      </Link>
      {passage && doc ? (
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
      ) : (
        <p className="concept-muted">
          {failed
            ? "This file could not be loaded."
            : doc
              ? "This section no longer exists."
              : "Loading"}
        </p>
      )}
    </div>
  );
}
