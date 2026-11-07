import { useAtomValue } from "jotai/react";
import { lazy, Suspense, useEffect, useRef, type JSX, type ReactNode, type RefObject } from "react";
import { Link, useLocation, useNavigate, useSearchParams } from "react-router";

import { Button } from "@/components/ui/button";
import type { DocView, FileTree } from "@/plan/api/schema-files";
import { SelectionAttach } from "@/plan/attachments";
import { openDiff } from "@/plan/diff/open-diff";
import { toAddress } from "@/plan/files/address";
import { openDocAtom, previousOpenTextAtom } from "@/plan/files/atoms";
import { FileDocHeading } from "@/plan/files/doc-heading-view";
import { useOpenDoc, type Load, type SlugRef, type StaleRefresh } from "@/plan/files/open-doc";
import { PlanView } from "@/plan/files/plan/plan-view";
import { planSlug, renamedPlan, treeDocs } from "@/plan/files/tree";
import { useDocMarkdownProps } from "@/plan/files/use-doc-markdown";
import { Markdown } from "@/plan/markdown";
import { formatBytes } from "@/plan/ui/format-bytes";
import { LoadState } from "@/plan/ui/load-state";

// Edit mode's React side is a separate chunk, fetched when editing starts: it kept plan.js over
// vite's 500 kB chunk warning. Revisit if plan.js drops well below the limit or edit mode must
// open without a second fetch.
const EditMode = lazy(() =>
  import("@/plan/editor/edit-mode").then((module) => ({ default: module.EditMode })),
);

type Props = {
  path: string;
  anchor: string | null;
  tree: FileTree | null;
  /** The bar with the control that opens the tree drawer; absent when the tree is beside. */
  opener?: ReactNode;
};

function useMarks(): string[] {
  const state: unknown = useLocation().state;
  const marks = (state as { marks?: unknown } | null)?.marks;
  return Array.isArray(marks) ? marks.filter((m): m is string => typeof m === "string") : [];
}

function DocMarkdown({
  doc,
  tree,
  marks,
}: {
  doc: DocView;
  tree: FileTree | null;
  marks: string[];
}) {
  const links = useDocMarkdownProps(doc, tree);
  return (
    <Markdown
      source={doc.text}
      path={doc.path}
      sections={doc.sections}
      marks={marks}
      suppressTitle={doc.title}
      {...links}
    />
  );
}

function DocBody({ doc, tree, marks }: { doc: DocView; tree: FileTree | null; marks: string[] }) {
  if (doc.truncated) {
    return (
      <>
        <p role="note" className="px-6 pb-2 text-[13px] text-muted-foreground">
          This file is {formatBytes(doc.size)}; file mode shows the first 1 MiB and does not edit
          files over 1 MiB.
        </p>
        <pre className="overflow-x-auto px-6 pb-8 font-mono text-xs whitespace-pre-wrap">
          {doc.text}
        </pre>
      </>
    );
  }
  if (doc.plan) return <PlanView doc={doc} />;
  return <DocMarkdown doc={doc} tree={tree} marks={marks} />;
}

function Missing({
  tree,
  slug,
  path,
}: {
  tree: FileTree | null;
  slug: string | null;
  path: string;
}) {
  const navigate = useNavigate();
  const known = slug ?? planSlug(path.split("/").at(-1) ?? null);
  const moved = tree && known ? renamedPlan(tree, known, path) : null;
  return (
    <LoadState
      state="not_found"
      title="This document no longer exists"
      action={{ label: "Back to the tree", onClick: () => navigate("/files") }}
    >
      {moved?.path && (
        <Link to={toAddress(moved.path)} className="underline">
          {moved.title} is now at {moved.path}
        </Link>
      )}
    </LoadState>
  );
}

/** Scrolls to the anchor (or the top) on a new target, and restores the heading kept across a refresh. */
function useScrollRestore(
  doc: DocView | null,
  anchor: string | null,
  missingAnchor: boolean,
  container: RefObject<HTMLElement | null>,
  keep: RefObject<string | null>,
): void {
  useEffect(() => {
    if (!doc) return;
    if (anchor && !missingAnchor) document.getElementById(anchor)?.scrollIntoView?.();
    else container.current?.closest("[data-reading-scroll]")?.scrollTo?.({ top: 0 });
    // Keyed on the target so a refresh of the same document does not scroll.
  }, [doc?.path, anchor, missingAnchor]);

  useEffect(() => {
    const id = keep.current;
    keep.current = null;
    if (id) document.getElementById(id)?.scrollIntoView?.();
  }, [doc?.hash, keep]);
}

/** A renamed plan: the tree no longer lists this path but lists the same slug elsewhere. */
function useRenamedRedirect(doc: DocView | null, tree: FileTree | null, path: string): void {
  const navigate = useNavigate();
  useEffect(() => {
    if (!doc?.plan || !tree || treeDocs(tree).some((e) => e.path === path)) return;
    const moved = renamedPlan(tree, doc.plan.slug.toLowerCase(), path);
    if (moved?.path) navigate(toAddress(moved.path), { replace: true });
  }, [tree, doc, path, navigate]);
}

function StaleNotice({ refresh }: { refresh: StaleRefresh }) {
  return (
    <div
      role="alert"
      className="flex flex-wrap items-center gap-x-3 px-6 pb-4 text-[13px] text-muted-foreground"
    >
      <span>
        This document changed, but the new text could not be loaded ({refresh.message}). The text
        below is the earlier version.
      </span>
      <Button variant="ghost" size="sm" onClick={refresh.retry}>
        Retry
      </Button>
    </div>
  );
}

function Notices({
  doc,
  anchor,
  missingAnchor,
  previous,
  refresh,
}: {
  doc: DocView;
  anchor: string | null;
  missingAnchor: boolean;
  previous: { path: string } | null;
  refresh: StaleRefresh | null;
}) {
  return (
    <>
      {refresh && <StaleNotice refresh={refresh} />}
      {previous?.path === doc.path && (
        <p className="px-6 pb-4 text-[13px] text-muted-foreground">
          <Link to={openDiff("open", doc.path)} className="underline">
            changed just now
          </Link>
        </p>
      )}
      {missingAnchor && (
        <p className="px-6 pb-4 text-[13px] text-muted-foreground">
          Section #{anchor} is not in this document any more
        </p>
      )}
    </>
  );
}

function LoadStates({
  load,
  loaded,
  retry,
  tree,
  path,
  slug,
}: {
  load: Load;
  loaded: boolean;
  retry: () => void;
  tree: FileTree | null;
  path: string;
  slug: SlugRef;
}) {
  return (
    <>
      {load.status === "loading" && !loaded && (
        <LoadState state="loading" title="Loading the document" />
      )}
      {load.status === "error" && (
        <LoadState
          state="error"
          title="Could not load the document"
          message={load.message}
          action={{ label: "Retry", onClick: retry }}
        />
      )}
      {load.status === "missing" && (
        <Missing
          tree={tree}
          slug={slug.current?.path === path ? slug.current.slug : null}
          path={path}
        />
      )}
    </>
  );
}

function LazyEditMode({ doc }: { doc: DocView }): JSX.Element {
  return (
    <Suspense
      fallback={
        <p role="status" className="px-3 py-2 text-[13px] text-muted-foreground">
          Loading editor
        </p>
      }
    >
      <EditMode doc={doc} />
    </Suspense>
  );
}

function DocPane({
  doc,
  anchor,
  missingAnchor,
  previous,
  refresh,
  editing,
  tree,
  marks,
  container,
}: {
  doc: DocView;
  anchor: string | null;
  missingAnchor: boolean;
  previous: { path: string } | null;
  refresh: StaleRefresh | null;
  editing: boolean;
  tree: FileTree | null;
  marks: string[];
  container: RefObject<HTMLDivElement | null>;
}) {
  return (
    <>
      <FileDocHeading doc={doc} />
      <Notices
        doc={doc}
        anchor={anchor}
        missingAnchor={missingAnchor}
        previous={previous}
        refresh={refresh}
      />
      {editing ? (
        <LazyEditMode doc={doc} />
      ) : (
        <>
          <div ref={container} data-doc-container className="min-w-0 px-6 pb-4">
            <DocBody doc={doc} tree={tree} marks={marks} />
          </div>
          <SelectionAttach
            container={container}
            path={doc.path}
            sections={doc.sections}
            hash={doc.hash}
          />
        </>
      )}
    </>
  );
}

/** The open document: header, lock notice and the rendered text. */
export function ReadingColumn({ path, anchor, tree, opener }: Props): JSX.Element {
  const container = useRef<HTMLDivElement>(null);
  const marks = useMarks();
  const [params] = useSearchParams();
  const { load, retry, refresh, keep, slug } = useOpenDoc(path, container);
  const open = useAtomValue(openDocAtom);
  const previous = useAtomValue(previousOpenTextAtom);
  const doc = open && open.doc.path === path && load.status !== "missing" ? open.doc : null;
  const editing = params.get("edit") === "1" && !!doc?.editable && !doc.truncated;
  const missingAnchor =
    !!doc &&
    !!anchor &&
    !anchor.startsWith("stage:") &&
    !doc.sections.some((s) => s.anchor === anchor);

  useScrollRestore(doc, anchor, missingAnchor, container, keep);
  useRenamedRedirect(doc, tree, path);

  return (
    <div className="flex min-h-full min-w-0 flex-1 flex-col">
      {opener}
      <LoadStates load={load} loaded={!!doc} retry={retry} tree={tree} path={path} slug={slug} />
      {doc && (
        <DocPane
          doc={doc}
          anchor={anchor}
          missingAnchor={missingAnchor}
          previous={previous}
          refresh={refresh}
          editing={editing}
          tree={tree}
          marks={marks}
          container={container}
        />
      )}
    </div>
  );
}
