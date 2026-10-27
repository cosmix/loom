import { useAtomValue, useStore } from "jotai/react";
import { MenuIcon } from "lucide-react";
import { useEffect, useMemo, useRef, useState, type JSX, type RefObject } from "react";
import { Link, useLocation, useNavigate } from "react-router";

import { Button } from "@/components/ui/button";
import type { DocView, FileTree } from "@/plan/api/schema-files";
import { SelectionAttach } from "@/plan/attachments";
import { toAddress } from "@/plan/files/address";
import { fetchDoc } from "@/plan/files/api";
import { openDocAtom, openDocIsStaleAtom, previousOpenTextAtom } from "@/plan/files/atoms";
import { DocHeader } from "@/plan/files/doc-header";
import { LockNotice } from "@/plan/files/lock-notice";
import { PlanView } from "@/plan/files/plan/plan-view";
import { planSlug, renamedPlan, treeDocs } from "@/plan/files/tree";
import { Markdown } from "@/plan/markdown";
import { LoadState } from "@/plan/ui/load-state";

type Load =
  | { status: "loading" }
  | { status: "ready" }
  | { status: "missing" }
  | { status: "error"; message: string };

const HEADINGS = "h1[id],h2[id],h3[id],h4[id],h5[id],h6[id]";

/** The id of the heading at or just above the top of the column, so a refresh can restore it. */
function headingAtTop(container: HTMLElement | null): string | null {
  if (!container) return null;
  const headings = [...container.querySelectorAll<HTMLElement>(HEADINGS)];
  const top = container.closest("[data-reading-scroll]")?.getBoundingClientRect().top ?? 0;
  const above = headings.filter((heading) => heading.getBoundingClientRect().top <= top + 1);
  return (above.at(-1) ?? headings[0])?.id ?? null;
}

function formatSize(bytes: number): string {
  if (bytes >= 1048576) return `${(bytes / 1048576).toFixed(1)} MiB`;
  if (bytes >= 1024) return `${(bytes / 1024).toFixed(1)} KiB`;
  return `${bytes} bytes`;
}

/**
 * Fetches the document at `path` into `openDocAtom`. While it is open, a changed hash
 * (`openDocIsStaleAtom`) refetches in place and records the text it replaced, so the
 * reader keeps their place and can see what changed.
 */
function useOpenDoc(path: string, container: RefObject<HTMLElement | null>) {
  const store = useStore();
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const stale = useAtomValue(openDocIsStaleAtom);
  const seenHash = useAtomValue(openDocAtom)?.seenHash;
  const keep = useRef<string | null>(null);
  const slug = useRef<{ path: string; slug: string } | null>(null);

  useEffect(() => {
    let active = true;
    store.set(openDocAtom, null);
    store.set(previousOpenTextAtom, null);
    setLoad({ status: "loading" });
    void fetchDoc(path).then((result) => {
      if (!active) return;
      if (!result.ok) {
        setLoad(
          result.status === 404
            ? { status: "missing" }
            : { status: "error", message: result.message },
        );
        return;
      }
      const doc = result.value;
      slug.current = doc.plan ? { path, slug: doc.plan.slug.toLowerCase() } : null;
      store.set(openDocAtom, { doc, seenHash: doc.hash });
      setLoad({ status: "ready" });
    });
    return () => {
      active = false;
    };
  }, [path, attempt, store]);

  useEffect(() => {
    const open = store.get(openDocAtom);
    if (!stale || !open || open.doc.path !== path) return;
    let active = true;
    store.set(previousOpenTextAtom, { path, text: open.doc.text });
    keep.current = headingAtTop(container.current);
    void fetchDoc(path).then((result) => {
      if (!active) return;
      if (result.ok) store.set(openDocAtom, { doc: result.value, seenHash: result.value.hash });
      else if (result.status === 404) setLoad({ status: "missing" });
    });
    return () => {
      active = false;
    };
  }, [stale, seenHash, path, store, container]);

  return { load, retry: () => setAttempt((n) => n + 1), keep, slug };
}

type Props = {
  path: string;
  anchor: string | null;
  tree: FileTree | null;
  onOpenTree?: () => void;
};

function useMarks(): string[] {
  const state: unknown = useLocation().state;
  const marks = (state as { marks?: unknown } | null)?.marks;
  return Array.isArray(marks) ? marks.filter((m): m is string => typeof m === "string") : [];
}

function DocBody({ doc, tree, marks }: { doc: DocView; tree: FileTree | null; marks: string[] }) {
  const navigate = useNavigate();
  const docPaths = useMemo(
    () => new Set(tree ? treeDocs(tree).flatMap((e) => (e.path ? [e.path] : [])) : [doc.path]),
    [tree, doc.path],
  );
  const anchors = useMemo(() => new Set(doc.sections.map((s) => s.anchor)), [doc.sections]);
  if (doc.truncated) {
    return (
      <>
        <p role="note" className="px-6 pb-2 text-[13px] text-muted-foreground">
          This file is {formatSize(doc.size)}; file mode shows the first 1 MiB and does not edit
          files over 1 MiB.
        </p>
        <pre className="overflow-x-auto px-6 pb-8 font-mono text-xs whitespace-pre-wrap">
          {doc.text}
        </pre>
      </>
    );
  }
  if (doc.plan) return <PlanView doc={doc} />;
  return (
    <Markdown
      source={doc.text}
      path={doc.path}
      sections={doc.sections}
      marks={marks}
      onNavigate={(address) => navigate(address)}
      docPaths={docPaths}
      anchorsOf={(target) => (target === doc.path ? anchors : null)}
    />
  );
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
    <>
      <LoadState
        state="not_found"
        title="This document no longer exists"
        action={{ label: "Back to the tree", onClick: () => navigate("/files") }}
      />
      {moved?.path && (
        <p className="px-6 text-[13px]">
          <Link to={toAddress(moved.path)} className="underline">
            {moved.title} is now at {moved.path}
          </Link>
        </p>
      )}
    </>
  );
}

/** The open document: header, lock notice and the rendered text. */
export function ReadingColumn({ path, anchor, tree, onOpenTree }: Props): JSX.Element {
  const container = useRef<HTMLDivElement>(null);
  const navigate = useNavigate();
  const marks = useMarks();
  const { load, retry, keep, slug } = useOpenDoc(path, container);
  const open = useAtomValue(openDocAtom);
  const previous = useAtomValue(previousOpenTextAtom);
  const doc = open && open.doc.path === path && load.status !== "missing" ? open.doc : null;

  const missingAnchor =
    doc && anchor && !anchor.startsWith("stage:") && !doc.sections.some((s) => s.anchor === anchor);

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

  // A renamed plan: the tree no longer lists this path but lists the same slug elsewhere.
  useEffect(() => {
    if (!doc?.plan || !tree || treeDocs(tree).some((e) => e.path === path)) return;
    const moved = renamedPlan(tree, doc.plan.slug.toLowerCase(), path);
    if (moved?.path) navigate(toAddress(moved.path), { replace: true });
  }, [tree, doc, path, navigate]);

  return (
    <div className="flex min-w-0 flex-1 flex-col">
      {onOpenTree && (
        <div className="border-b border-hairline px-4 py-2">
          <Button variant="ghost" size="sm" onClick={onOpenTree}>
            <MenuIcon aria-hidden="true" />
            Documents
          </Button>
        </div>
      )}
      {load.status === "loading" && !doc && (
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
      {doc && (
        <>
          <DocHeader doc={doc} />
          {doc.lock && !doc.plan && <LockNotice lock={doc.lock} />}
          {previous?.path === doc.path && (
            <p className="px-6 pt-2 text-[13px] text-muted-foreground">
              <Link to={`${toAddress(doc.path)}?diff=open`} className="underline">
                changed just now
              </Link>
            </p>
          )}
          {missingAnchor && (
            <p className="px-6 pt-2 text-[13px] text-muted-foreground">
              Section #{anchor} is not in this document any more
            </p>
          )}
          <div ref={container} data-doc-container className="min-w-0 px-6 py-4">
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
    </div>
  );
}
