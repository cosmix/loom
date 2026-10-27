import { useEffect, useMemo, useRef, useState, type JSX } from "react";

import { useMediaQuery } from "@/lib/use-media-query";
import type { SectionView } from "@/plan/api/schema-files";
import { AddToChat } from "@/plan/attachments";
import type { EditorHandle } from "@/plan/editor/codemirror";
import { loadCodeMirror } from "@/plan/editor/load";
import { useEditorSelectionAttachment } from "@/plan/editor/selection-attachment";

import { diffProvenance } from "./diff-provenance";

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

/**
 * Builds the CodeMirror diff view into `host` and rebuilds it when either text or the
 * layout changes. A load that finishes after the effect is gone is dropped.
 */
function useDiffEditor(before: string | null, after: string | null, wide: boolean) {
  const host = useRef<HTMLDivElement>(null);
  const handle = useRef<EditorHandle | null>(null);
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

/**
 * A read-only diff. The view is rebuilt when either text or the layout changes. A
 * selection on either side can be added to the chat.
 */
export function DiffView({
  before,
  after,
  path,
  sections = NO_SECTIONS,
}: DiffViewProps): JSX.Element {
  const wrapper = useRef<HTMLDivElement>(null);
  const wide = useMediaQuery(SIDE_BY_SIDE);
  const { host, handle, phase } = useDiffEditor(before, after, wide);
  const provenance = useMemo(() => diffProvenance(path, sections), [path, sections]);
  const attachment = useEditorSelectionAttachment(wrapper, handle, provenance);

  return (
    <div
      ref={wrapper}
      className="flex min-h-0 flex-1 flex-col"
      data-diff-layout={wide ? "split" : "unified"}
      role="group"
      aria-label={`Diff of ${path}`}
    >
      {phase === "loading" ? (
        <p className="px-3 py-2 text-[13px] text-muted-foreground">Loading diff</p>
      ) : null}
      {phase === "failed" ? (
        <p role="alert" className="px-3 py-2 text-[13px] text-destructive">
          The diff failed to load. Reload the page to try again.
        </p>
      ) : null}
      <div ref={host} className="min-h-0 flex-1 overflow-auto" />
      {attachment.draft && attachment.rect && (
        <AddToChat draft={attachment.draft} rect={attachment.rect} onAdded={attachment.clear} />
      )}
    </div>
  );
}
