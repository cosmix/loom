import { Fragment, useEffect, useMemo, useRef, useState, type JSX } from "react";

import { useMediaQuery } from "@/lib/use-media-query";
import type { SectionView } from "@/plan/api/schema-files";
import { AddToChat } from "@/plan/attachments";
import type { EditorHandle } from "@/plan/editor/codemirror";
import { loadCodeMirror } from "@/plan/editor/load";
import { useEditorSelectionAttachment } from "@/plan/editor/selection-attachment";
import { LoadState } from "@/plan/ui/load-state";
import { SegmentedToggle } from "@/plan/ui/segmented-toggle";

import { diffBlocks, type DiffBlock } from "./block-diff";
import { diffProvenance } from "./diff-provenance";
import { RenderedDiff } from "./rendered-diff";

import "./diff.css";

export interface DiffViewProps {
  /** `null`: the file was created, so the whole text is added. */
  before: string | null;
  /** `null`: the file was deleted, so the whole last text is removed. */
  after: string | null;
  path: string;
  /** The document's sections, for the Add to chat anchor; none means no anchor. */
  sections?: SectionView[];
}

const NO_SECTIONS: SectionView[] = [];

/** Side by side from this width up; unified below. Keep equal to `UNIFIED_BELOW_PX`. */
const SIDE_BY_SIDE = "(min-width: 900px)";

type Phase = "loading" | "ready" | "failed";
type View = "rendered" | "source";

const VIEWS: { value: View; label: string }[] = [
  { value: "rendered", label: "Rendered" },
  { value: "source", label: "Source" },
];

/**
 * Builds the CodeMirror diff view into `host` and rebuilds it when either text or the
 * layout changes. A load that finishes after the effect is gone is dropped. The first view
 * built scrolls to its first change; a rebuild leaves the reader where the editor puts them.
 */
function useDiffEditor(before: string | null, after: string | null, wide: boolean) {
  const host = useRef<HTMLDivElement>(null);
  const handle = useRef<EditorHandle | null>(null);
  const scrolled = useRef(false);
  const [phase, setPhase] = useState<Phase>("loading");

  useEffect(() => {
    let cancelled = false;
    let created: EditorHandle | null = null;
    setPhase("loading");
    loadCodeMirror().then(
      (codemirror) => {
        const parent = host.current;
        if (cancelled || !parent) return;
        created = codemirror.createDiffView(parent, {
          before: before ?? "",
          after: after ?? "",
          unified: !wide,
        });
        handle.current = created;
        setPhase("ready");
        if (!scrolled.current) created.scrollToFirstChange();
        scrolled.current = true;
      },
      () => {
        if (!cancelled) setPhase("failed");
      },
    );
    return () => {
      cancelled = true;
      created?.destroy();
      if (handle.current === created) handle.current = null;
    };
  }, [before, after, wide]);

  return { host, handle, phase };
}

/** The CodeMirror diff of the two source texts. A selection on either side can be added to the chat. */
function SourceDiff(props: Required<DiffViewProps> & { wide: boolean }): JSX.Element {
  const { before, after, path, sections, wide } = props;
  const wrapper = useRef<HTMLDivElement>(null);
  const { host, handle, phase } = useDiffEditor(before, after, wide);
  const provenance = useMemo(() => diffProvenance(path, sections), [path, sections]);
  const attachment = useEditorSelectionAttachment(wrapper, handle, provenance);

  return (
    <div ref={wrapper} className="diff-source">
      {phase === "loading" && <LoadState state="loading" size="compact" title="Loading diff" />}
      {phase === "failed" && (
        <LoadState
          state="error"
          size="compact"
          title="The diff failed to load"
          message="Reload the page to try again."
        />
      )}
      {/* The host stays mounted for the editor to build into; it takes the spare height only once
          the diff is drawn, so the state above centres in the whole view until then. */}
      <div ref={host} className="diff-source__host" data-ready={phase === "ready" || undefined} />
      {attachment.draft && attachment.rect && (
        <AddToChat draft={attachment.draft} rect={attachment.rect} onAdded={attachment.clear} />
      )}
    </div>
  );
}

const COUNTED = [
  { kind: "added", glyph: "+" },
  { kind: "removed", glyph: "−" },
  { kind: "changed", glyph: "±" },
] as const;

/** What changed, in words: a created or deleted file, then the count of blocks of each kind. */
function Summary(props: { blocks: DiffBlock[]; created: boolean; deleted: boolean }): JSX.Element {
  const parts: JSX.Element[] = [];
  if (props.created)
    parts.push(
      <span key="new" className="diff-summary__file">
        New file
      </span>,
    );
  if (props.deleted) {
    parts.push(
      <span key="deleted" className="diff-summary__file">
        Deleted file
      </span>,
    );
  }
  for (const { kind, glyph } of COUNTED) {
    const count = props.blocks.filter((block) => block.kind === kind).length;
    if (count === 0) continue;
    parts.push(
      <span key={kind} className="diff-summary__count" data-kind={kind}>
        <span className="diff-summary__glyph" aria-hidden="true">
          {glyph}
        </span>
        {count} {kind}
      </span>,
    );
  }
  if (parts.length === 0) parts.push(<span key="none">No changes</span>);
  return (
    <p className="diff-summary">
      {/* The space between parts is real text: the parts are inline, so it is drawn, copied and
          spoken as a break ("1 added 2 changed"). */}
      {parts.map((part, index) => (
        <Fragment key={part.key}>
          {index > 0 && " "}
          {part}
        </Fragment>
      ))}
    </p>
  );
}

/**
 * A read-only diff of one document. It opens on the rendered document with its changes marked;
 * Source shows the CodeMirror diff of the text, which is loaded only when chosen.
 */
export function DiffView({
  before,
  after,
  path,
  sections = NO_SECTIONS,
}: DiffViewProps): JSX.Element {
  const wide = useMediaQuery(SIDE_BY_SIDE);
  const [view, setView] = useState<View>("rendered");
  const blocks = useMemo(() => diffBlocks(before ?? "", after ?? ""), [before, after]);

  return (
    <div
      className="diff-view"
      data-diff-layout={wide ? "split" : "unified"}
      role="group"
      aria-label={`Diff of ${path}`}
    >
      <div className="diff-head">
        <Summary blocks={blocks} created={before === null} deleted={after === null} />
        <SegmentedToggle<View> label="Diff view" options={VIEWS} value={view} onChange={setView} />
      </div>
      {view === "rendered" ? (
        <RenderedDiff blocks={blocks} path={path} />
      ) : (
        <SourceDiff before={before} after={after} path={path} sections={sections} wide={wide} />
      )}
    </div>
  );
}
