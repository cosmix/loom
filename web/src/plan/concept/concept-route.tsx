import { useAtomValue, useSetAtom, useStore } from "jotai/react";
import { useEffect, useRef } from "react";
import { Link, useLocation, useNavigate } from "react-router";

import { isTyping } from "@/lib/is-typing";
import { isConceptPageKey, type ConceptPage } from "@/plan/api/schema-concept";
import { AddToChat, addAttachmentAtom, useSelectionAttachment } from "@/plan/attachments";
import { canAuthorAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";
import type { Store } from "@/state/store";

import { summaryIdOfKey } from "./column-context";
import { DocumentKey } from "./document-key";
import { ConceptNotFound, EmptyStates } from "./empty-states";
import { pageSentences } from "./open-state";
import { PageHeader } from "./page-header";
import {
  conceptPageAtom,
  isMissing,
  loadConceptPage,
  pageKeyOfSummary,
  useConceptPage,
  useConceptSubscription,
  type ConceptPageState,
} from "./page-state";
import { ReadingColumn } from "./reading-column";
import {
  attachmentFromSelection,
  attachmentsFromSelection,
  overLimit,
  type SentenceResolver,
} from "./selection";

/** The second key of `g p` / `g e` must follow the first within this long. */
const GO_KEY_WINDOW_MS = 1000;
const GO_TARGETS: Record<string, string> = { p: "/product", e: "/engineering" };

function segmentsOf(pathname: string): string[] {
  return pathname
    .split("/")
    .filter(Boolean)
    .map((part) => {
      try {
        return decodeURIComponent(part);
      } catch {
        return part;
      }
    });
}

/**
 * The page key of a concept URL: `product`, `engineering`, `product/<stem>`, `engineering/<slug>`.
 * Decoding can yield anything (`..%2F..%2Fserver` becomes `../../server`); `isConceptPageKey`
 * decides whether the result is a page.
 */
function pageKeyOf(pathname: string): string {
  return segmentsOf(pathname).join("/");
}

/** The last path part as written in the URL: still encoded, so it holds no `/`. */
function rawNameOf(pathname: string): string {
  return pathname.split("/").filter(Boolean).pop() ?? "";
}

/**
 * The name of the concept page at `pathname`; null outside concept mode. Its only caller is the
 * eyebrow above an overview (a summary page's header shows its own title); the presence list
 * does not read it.
 */
export function conceptPageLabel(pathname: string): string | null {
  const [scope, name, ...rest] = segmentsOf(pathname);
  if ((scope !== "product" && scope !== "engineering") || rest.length > 0) return null;
  if (name === undefined) return scope === "product" ? "Product overview" : "Engineering overview";
  return `${name} summary`;
}

/** `g p` and `g e` go to the overviews; ignored while typing. */
function useGoKeys(): void {
  const navigate = useNavigate();
  const pressedAt = useRef<number | null>(null);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.altKey || event.ctrlKey || event.metaKey || isTyping(event.target)) return;
      const armed =
        pressedAt.current !== null && event.timeStamp - pressedAt.current < GO_KEY_WINDOW_MS;
      const target = GO_TARGETS[event.key];
      pressedAt.current = null;
      if (event.key === "g") pressedAt.current = event.timeStamp;
      else if (armed && target) navigate(target);
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navigate]);
}

function Unlisted({ page, pageKey }: { page: ConceptPage; pageKey: string }) {
  if (page.unlisted.length === 0 || pageKey.includes("/")) return null;
  const heading = pageKey === "engineering" ? "Unlisted plans" : "Unlisted sections";
  return (
    <section aria-label={heading} className="pt-8">
      <h2 className="eyebrow pb-2">{heading}</h2>
      <ul className="flex flex-col gap-1 text-[13px]">
        {page.unlisted.map((doc) => (
          <li key={doc.id}>
            <Link to={`/${doc.page}`} className="underline">
              {doc.title}
            </Link>
          </li>
        ))}
      </ul>
    </section>
  );
}

/** Resolves a selected sentence by its summary: the page's own, else the cited summary page's. */
function sentenceResolver(store: Store, page: ConceptPage, pageKey: string): SentenceResolver {
  return (summary, id) => {
    const own = summary === summaryIdOfKey(pageKey);
    const source = own ? page : store.get(conceptPageAtom(pageKeyOfSummary(page, summary))).page;
    const sentence = source ? pageSentences(source).find((item) => item.id === id) : undefined;
    return source && sentence ? { sentence, hash: source.file_hash } : undefined;
  };
}

/**
 * The reading column plus the floating "Add to chat" for a text selection inside it. A
 * selection spanning levels carries each part's provenance. When any part is over the
 * per-attachment limit, `AddToChat` is given the first such part: it shows its over-limit
 * note and no Add button, so nothing is added and the reader is told why. Otherwise it adds
 * the first part and `addRest` the others.
 */
function Column({ page, pageKey }: { page: ConceptPage; pageKey: string }) {
  const canAuthor = useAtomValue(canAuthorAtom);
  const add = useSetAtom(addAttachmentAtom);
  const columnRef = useRef<HTMLDivElement>(null);
  const store = useStore();
  const selection = useSelectionAttachment(columnRef, (sel) =>
    attachmentFromSelection(sel, sentenceResolver(store, page, pageKey)),
  );
  // `addRest` re-reads the live selection, so it holds the others to the limit itself.
  const addRest = () => {
    const sel = window.getSelection();
    const parts = sel ? attachmentsFromSelection(sel, sentenceResolver(store, page, pageKey)) : [];
    parts
      .slice(1)
      .filter((draft) => !overLimit(draft))
      .forEach((draft) => add(draft));
    selection.clear();
  };
  return (
    <div ref={columnRef}>
      <ReadingColumn
        key={pageKey}
        pageKey={pageKey}
        page={page}
        onAttach={canAuthor ? (draft) => add(draft) : undefined}
      />
      {selection.draft && selection.rect && (
        <AddToChat draft={selection.draft} rect={selection.rect} onAdded={addRest} />
      )}
    </div>
  );
}

function LoadedPage({ page, pageKey }: { page: ConceptPage; pageKey: string }) {
  const { pathname } = useLocation();
  const summary = pageKey.includes("/");
  return (
    <>
      {summary ? (
        <PageHeader page={page} />
      ) : (
        <p className="eyebrow pb-3">{conceptPageLabel(pathname)}</p>
      )}
      {page.key_chips.length > 0 && <DocumentKey page={page} pageKey={pageKey} />}
      {page.empty ? (
        <EmptyStates page={page} empty={page.empty} />
      ) : (
        <Column page={page} pageKey={pageKey} />
      )}
      <Unlisted page={page} pageKey={pageKey} />
    </>
  );
}

/** What the route shows for a page key and its load state. */
function RouteBody({
  pageKey,
  rawName,
  state,
}: {
  pageKey: string;
  rawName: string;
  state: ConceptPageState;
}) {
  const store = useStore();
  // An address that is no page key is never fetched; `rawName` keeps the not-found lookup
  // from deriving a source path out of it.
  if (!isConceptPageKey(pageKey)) return <ConceptNotFound pageKey={rawName} />;
  if (state.page) return <LoadedPage page={state.page} pageKey={pageKey} />;
  if (state.error === null) return <LoadState state="loading" />;
  if (isMissing(state)) return <ConceptNotFound pageKey={pageKey} />;
  return (
    <LoadState
      state="error"
      title="Could not load this page"
      message={state.error}
      action={{ label: "Retry", onClick: () => void loadConceptPage(store, pageKey) }}
    />
  );
}

/** The Product and Engineering tabs and their document summary pages. */
export function ConceptRoute() {
  useConceptSubscription();
  useGoKeys();
  const { pathname } = useLocation();
  const key = pageKeyOf(pathname);
  const state = useConceptPage(key);
  return (
    <div className="mx-auto w-full max-w-[70ch] px-4 py-6 lg:px-6">
      <RouteBody pageKey={key} rawName={rawNameOf(pathname)} state={state} />
    </div>
  );
}
