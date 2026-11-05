import { cn } from "cn";
import { FileText } from "lucide-react";
import { useMemo, type JSX, type ReactNode } from "react";
import { Link } from "react-router";

import type { SourceAnchor } from "@/plan/api/schema-concept";
import type { DocView } from "@/plan/api/schema-files";
import { Untrusted } from "@/plan/chat/visible-controls";
import { featureOfDocPath, featureSourceHref } from "@/plan/feature/source-href";
import { Markdown } from "@/plan/markdown";

import { passageKey, useColumn } from "./column-context";
import { FLASH_CLASS, useFlashOnChange } from "./flash";
import { anchorKey } from "./open-state";
import { useKeptDoc } from "./passage-doc";
import { fileName, passageOf } from "./passages";
import { useSamePassage } from "./same-passage";

type Passage = NonNullable<ReturnType<typeof passageOf>>;

/**
 * The address of the passage's source, carrying its lines so the page can highlight them: inside
 * the feature the document belongs to, in file mode for a document outside any feature.
 */
export function passageHref(anchor: SourceAnchor, lines: { start: number; end: number } | null) {
  return featureSourceHref(featureOfDocPath(anchor.path), anchor.path, {
    lines,
    anchor: anchor.anchor,
  });
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

/** A repeated passage: one line pointing back to the first showing in this view. */
function SamePassageLine({ goFirst }: { goFirst: () => void }): JSX.Element {
  return (
    <p className="concept-muted">
      <button type="button" className="underline" onClick={goFirst}>
        Same passage as above
      </button>
    </p>
  );
}

/**
 * One source passage: a heading section of a file under `doc/`, flowing inline in the opened
 * run after a chip naming its file and heading, with no card or border. Its body goes through
 * file mode's Markdown renderer as an excerpt (line offset, no heading ids), so a cut never
 * claims another section's id. `leading` opens the chip line and `trailing` follows the body:
 * the run's fold caps, so they sit in the text rather than on rows of their own.
 */
export function PassageBlock({
  parent,
  sentenceId,
  anchor,
  leading,
  trailing,
}: {
  /** The parent of the run showing the passage: `sentenceId` may be open under two citers. */
  parent: string | null;
  sentenceId: string;
  anchor: SourceAnchor;
  leading?: ReactNode;
  trailing?: ReactNode;
}): JSX.Element {
  const { doc, failed } = useKeptDoc(anchor.path);
  const passage = useMemo(() => (doc ? passageOf(doc, anchor.anchor) : null), [doc, anchor.anchor]);
  const flashing = useFlashOnChange(passage?.hash ?? null);
  const key = anchorKey(anchor);
  const lines = passage ? { start: passage.startLine, end: passage.endLine } : null;
  const focusKey = passageKey(parent, sentenceId, key);
  const goFirst = useSamePassage(key, focusKey);
  return (
    <div
      className={cn("concept-passage", flashing && FLASH_CLASS)}
      data-focus={focusKey}
      data-passage={key}
      data-hash={doc?.hash}
      data-start-line={lines?.start}
      data-end-line={lines?.end}
      tabIndex={-1}
    >
      {leading}
      <Link tabIndex={-1} className="concept-passage-chip" to={passageHref(anchor, lines)}>
        <FileText aria-hidden="true" size={12} />
        <span className="concept-passage-path">
          <Untrusted text={fileName(anchor.path)} />
        </span>
        {" › "}
        <Untrusted text={passage?.title ?? anchor.anchor} />
      </Link>
      {goFirst ? (
        <SamePassageLine goFirst={goFirst} />
      ) : passage && doc ? (
        <PassageBody anchor={anchor} passage={passage} doc={doc} />
      ) : (
        <p className="concept-muted">{missingText(failed, doc)}</p>
      )}
      {trailing}
    </div>
  );
}
