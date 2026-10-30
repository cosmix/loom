import { useAtomValue, useSetAtom, useStore } from "jotai/react";
import { useEffect, useRef } from "react";
import { Link, useLocation, useNavigate } from "react-router";

import { Kbd } from "@/components/ui/kbd";
import { isTyping } from "@/lib/is-typing";
import {
  featureOfKey,
  isOverviewKey,
  overviewKey,
  pageHref,
  pageKeyOfPath,
  parsePageKey,
  scopeOfKey,
  type ConceptScope,
} from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { AddToChat, addAttachmentAtom, useSelectionAttachment } from "@/plan/attachments";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { activeFeatureAtom, canAuthorAtom, featuresAtom } from "@/plan/state/atoms";
import { LoadState } from "@/plan/ui/load-state";
import type { Store } from "@/state/store";

import { summaryIdOfKey } from "./column-context";
import { foldedWith } from "./column-model";
import { DocumentKey } from "./document-key";
import { ConceptNotFound, EmptyStates } from "./empty-states";
import { pageSentences } from "./open-state";
import { PageHeading } from "./page-heading";
import {
  conceptPageAtom,
  isMissing,
  loadConceptPage,
  pageKeyOfSummary,
  useConceptPage,
  useConceptSubscription,
} from "./page-state";
import { ReadingColumn } from "./reading-column";
import {
  attachmentFromSelection,
  attachmentsFromSelection,
  overLimit,
  type SentenceResolver,
} from "./selection";
import { openRunsAtom } from "./view-state";

/** The second key of `g p` / `g e` must follow the first within this long. */
const GO_KEY_WINDOW_MS = 1000;
const GO_TARGETS: Record<string, ConceptScope> = { p: "product", e: "engineering" };

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
  const key = pageKeyOfPath(pathname);
  const parts = key === null ? null : parsePageKey(key);
  if (parts === null) return null;
  if (parts.kind === "overview") {
    return parts.scope === "product" ? "Product overview" : "Engineering overview";
  }
  return `${parts.doc} summary`;
}

/**
 * The eyebrow above an overview: "<feature title> · product overview", or the plain page label
 * while the feature's title is not loaded.
 */
function useOverviewEyebrow(pageKey: string, pathname: string): string | null {
  const feature = featureOfKey(pageKey);
  const title = useAtomValue(featuresAtom).find((item) => item.feature === feature)?.title;
  const label = conceptPageLabel(pathname);
  if (!title || label === null) return label;
  return `${title} · ${label.toLowerCase()}`;
}

/** `g p` and `g e` go to the overviews; ignored while typing. */
function useGoKeys(feature: string | null): void {
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
      else if (armed && target && feature) navigate(pageHref(overviewKey(feature, target)));
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [navigate, feature]);
}

/** Outside the column, Escape folds the last run without moving focus out of the header. */
function useEscapeKeys(pageKey: string): void {
  const store = useStore();
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (
        event.key !== "Escape" ||
        event.defaultPrevented ||
        event.altKey ||
        event.ctrlKey ||
        event.metaKey ||
        isTyping(event.target)
      ) {
        return;
      }
      if (document.querySelector('[role="dialog"][data-state="open"], [role="alertdialog"]'))
        return;
      // The column owns Escape around its focused run, including events from its controls.
      const target = event.target instanceof Element ? event.target : null;
      if (
        target?.closest(".concept-column") ||
        document.activeElement?.closest(".concept-column")
      ) {
        return;
      }
      const open = store.get(openRunsAtom(pageKey));
      const last = open[open.length - 1];
      if (!last) return;
      event.preventDefault();
      store.set(openRunsAtom(pageKey), event.shiftKey ? [] : foldedWith(open, last));
    };
    window.addEventListener("keydown", onKeyDown);
    return () => window.removeEventListener("keydown", onKeyDown);
  }, [store, pageKey]);
}

function Unlisted({ page, pageKey }: { page: ConceptPage; pageKey: string }) {
  if (page.unlisted.length === 0 || !isOverviewKey(pageKey)) return null;
  const heading = scopeOfKey(pageKey) === "engineering" ? "Unlisted plans" : "Unlisted sections";
  return (
    <section aria-label={heading} className="pt-8">
      <h2 className="eyebrow pb-2">{heading}</h2>
      <ul className="flex flex-col gap-1 text-[13px]">
        {page.unlisted.map((doc) => (
          <li key={doc.id}>
            <Link to={pageHref(doc.page)} className="underline">
              <Untrusted text={doc.title} />
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
function Column({
  page,
  pageKey,
  dropTitle,
}: {
  page: ConceptPage;
  pageKey: string;
  dropTitle: boolean;
}) {
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
        dropTitle={dropTitle}
        onAttach={canAuthor ? (draft) => add(draft) : undefined}
      />
      {selection.draft && selection.rect && (
        <AddToChat draft={selection.draft} rect={selection.rect} onAdded={addRest} />
      )}
    </div>
  );
}

const HINTS: { keys: string; text: string }[] = [
  { keys: "Enter", text: "opens the focused sentence" },
  { keys: "Esc", text: "folds one level" },
  { keys: "⇧ Esc", text: "folds all" },
];
const ATTACH_HINT = { keys: "a", text: "adds the sentence to chat" };

/** The keys of the reading column, under it; `a` only where the column can attach. */
function KeyHints() {
  const canAuthor = useAtomValue(canAuthorAtom);
  const hints = canAuthor ? [...HINTS, ATTACH_HINT] : HINTS;
  return (
    <div
      aria-label="Reading keys"
      role="group"
      className="mt-1.5 flex flex-wrap gap-x-4 gap-y-2 font-sans text-xs text-muted-foreground"
    >
      {hints.map(({ keys, text }) => (
        <span key={keys} className="inline-flex items-center gap-1.5">
          <Kbd className="h-auto min-w-0 rounded-[4px] border border-b-2 border-hairline bg-card px-[5px] py-px font-mono text-[11px]">
            {keys}
          </Kbd>
          {text}
        </span>
      ))}
    </div>
  );
}

function LoadedPage({ page, pageKey }: { page: ConceptPage; pageKey: string }) {
  const { pathname } = useLocation();
  const eyebrow = useOverviewEyebrow(pageKey, pathname);
  const summary = !isOverviewKey(pageKey);
  return (
    <>
      {summary ? (
        <PageHeading page={page} />
      ) : (
        <p className="eyebrow pb-3">{eyebrow !== null && <Untrusted text={eyebrow} />}</p>
      )}
      {page.key_chips.length > 0 && <DocumentKey page={page} pageKey={pageKey} />}
      {page.empty ? (
        <EmptyStates page={page} empty={page.empty} />
      ) : (
        <>
          <Column page={page} pageKey={pageKey} dropTitle={summary} />
          <KeyHints />
        </>
      )}
      <Unlisted page={page} pageKey={pageKey} />
    </>
  );
}

/** What the route shows for a page key and its load state. */
function RouteBody({ pageKey }: { pageKey: string }) {
  useConceptSubscription();
  const feature = featureOfKey(pageKey);
  useGoKeys(feature);
  useEscapeKeys(pageKey);
  const setFeature = useSetAtom(activeFeatureAtom);
  useEffect(() => {
    if (feature !== null) setFeature(feature);
  }, [feature, setFeature]);
  const state = useConceptPage(pageKey);
  const store = useStore();
  if (state.page) return <LoadedPage page={state.page} pageKey={pageKey} />;
  if (state.error === null) return <LoadState state="loading" />;
  if (isMissing(state)) return <ConceptNotFound pageKey={pageKey} />;
  return (
    <LoadState
      state="error"
      title="Could not load this page"
      message={visibleControls(state.error)}
      action={{ label: "Retry", onClick: () => void loadConceptPage(store, pageKey) }}
    />
  );
}

/** The Product and Engineering tabs and their document summary pages. */
export function ConceptRoute() {
  const { pathname } = useLocation();
  const key = pageKeyOfPath(pathname);
  return (
    <div className="w-full px-[clamp(16px,4vw,56px)] pt-8 pb-12">
      <div className="mx-auto max-w-[68ch]">
        {key === null ? (
          <ConceptNotFound pageKey={rawNameOf(pathname)} />
        ) : (
          <RouteBody pageKey={key} />
        )}
      </div>
    </div>
  );
}
