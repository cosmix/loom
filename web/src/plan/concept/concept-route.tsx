import { useAtomValue, useSetAtom, useStore } from "jotai/react";
import { useEffect, useRef, type ReactNode } from "react";
import { Link, useLocation, useNavigate } from "react-router";

import { Kbd } from "@/components/ui/kbd";
import { isTyping } from "@/lib/is-typing";
import { OPEN_DIALOG } from "@/lib/open-dialog";
import {
  featureOfKey,
  isOverviewKey,
  overviewKey,
  pageHref,
  pageKeyOfPath,
  parsePageKey,
  type ConceptScope,
} from "@/plan/api/page-key";
import type { ConceptPage } from "@/plan/api/schema-concept";
import { AddToChat, addAttachmentAtom, useSelectionAttachment } from "@/plan/attachments";
import type { AttachmentDraft } from "@/plan/attachments/types";
import { elsewhereSessionAtom } from "@/plan/chat/session-home";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { PageHead } from "@/plan/feature/plan-page";
import { activeFeatureAtom, canAuthorAtom, featuresAtom } from "@/plan/state/atoms";
import { PendingApprovalNotice } from "@/plan/summaries/approval-notice";
import { LoadState } from "@/plan/ui/load-state";
import type { Store } from "@/state/store";

import { summaryIdOfKey } from "./column-context";
import { foldedWith } from "./column-model";
import { DocumentKey } from "./document-key";
import { ConceptNotFound, EmptyStates } from "./empty-states";
import { citableSentences } from "./open-state";
import { hasOpenable } from "./open-all";
import { OpenAllButton, PageHeading } from "./page-heading";
import {
  conceptPageAtom,
  isMissing,
  loadConceptPage,
  type ConceptPageState,
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
const GO_TARGETS: Record<string, ConceptScope> = { p: "product", e: "metaplan" };

/** The last path part as written in the URL: still encoded, so it holds no `/`. */
function rawNameOf(pathname: string): string {
  return pathname.split("/").filter(Boolean).pop() ?? "";
}

/**
 * The name of the concept page at `pathname`; null outside concept mode. It titles an overview and
 * feeds the eyebrow above it (a summary page's header shows its own title); the presence list
 * does not read it.
 */
export function conceptPageLabel(pathname: string): string | null {
  const key = pageKeyOfPath(pathname);
  const parts = key === null ? null : parsePageKey(key);
  if (parts === null) return null;
  if (parts.kind === "overview") {
    return "Product overview";
  }
  if (parts.kind === "summary" && parts.scope === "metaplan") return "Metaplan summary";
  return `${parts.doc} summary`;
}

/**
 * The eyebrow above an overview: "<feature slug> · product overview", or the plain page label
 * while the feature is not in the project's checklists.
 */
function useOverviewEyebrow(pageKey: string, pathname: string): string | null {
  const feature = featureOfKey(pageKey);
  const known = useAtomValue(featuresAtom).some((item) => item.feature === feature);
  const label = conceptPageLabel(pathname);
  if (!feature || !known || label === null) return label;
  return `${feature} · ${label.toLowerCase()}`;
}

/** `g p` and `g e` go to the product overview and metaplan; ignored while typing or while a dialog is open. */
function useGoKeys(feature: string | null): void {
  const navigate = useNavigate();
  const pressedAt = useRef<number | null>(null);
  useEffect(() => {
    const onKeyDown = (event: KeyboardEvent) => {
      if (event.altKey || event.ctrlKey || event.metaKey || isTyping(event.target)) return;
      if (document.querySelector(OPEN_DIALOG)) return;
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
      if (document.querySelector(OPEN_DIALOG)) return;
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
  const heading = "Unlisted sections";
  return (
    <section aria-label={heading} className="pt-8">
      <h2 className="plan-eyebrow pb-2">{heading}</h2>
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
    const sentence = source ? citableSentences(source).find((item) => item.id === id) : undefined;
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
  // An open session that belongs to another screen leaves the panel with no composer, so a draft
  // added now would carry a chip nobody sees. A concluded one is not open: the next chat starts
  // here, so attaching still works.
  const attachHere = (draft: AttachmentDraft) => {
    if (store.get(elsewhereSessionAtom) === null) add(draft);
  };
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
        onAttach={canAuthor ? attachHere : undefined}
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
  // `a` is a no-op while an open session belongs to another screen (`attachHere`).
  // Both atoms are read unconditionally: a hook behind `&&` changes the hook count with authoring.
  const canAuthor = useAtomValue(canAuthorAtom);
  const elsewhere = useAtomValue(elsewhereSessionAtom);
  const canAttach = canAuthor && elsewhere === null;
  const hints = canAttach ? [...HINTS, ATTACH_HINT] : HINTS;
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

/**
 * An overview's head: the eyebrow over the page title (ui/DESIGN.md, Page rhythm), with Open all
 * / Fold all at the end of the title's line when the page has a sentence that opens. A rebuild
 * waiting for approval shows its "Rebuild all" and sentence on a line under the title.
 */
function OverviewHead({ page, pageKey }: { page: ConceptPage; pageKey: string }) {
  const { pathname } = useLocation();
  const eyebrow = useOverviewEyebrow(pageKey, pathname);
  const title = conceptPageLabel(pathname);
  if (title === null) return null;
  return (
    <div className="concept-overview-head">
      <PageHead eyebrow={eyebrow ?? title} title={title} />
      {hasOpenable(page) && <OpenAllButton page={page} />}
      {/* Empty (and hidden by CSS) unless a rebuild is waiting for approval. */}
      <div className="concept-overview-head__notice">
        <PendingApprovalNotice />
      </div>
    </div>
  );
}

/**
 * `above` (the gaps) sits between the page's head and an overview's document key, so the head
 * comes first. A summary page draws no key: its heading names its document.
 */
function LoadedPage({
  page,
  pageKey,
  above,
}: {
  page: ConceptPage;
  pageKey: string;
  above: ReactNode;
}) {
  const summary = !isOverviewKey(pageKey);
  return (
    <>
      {summary ? <PageHeading page={page} /> : <OverviewHead page={page} pageKey={pageKey} />}
      {above}
      {!summary && page.key_chips.length > 0 && <DocumentKey page={page} pageKey={pageKey} />}
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

/** The page is loading, missing, or failed to load. */
function Unloaded({ pageKey, state }: { pageKey: string; state: ConceptPageState }) {
  const store = useStore();
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

/** What the route shows for a page key and its load state. */
function RouteBody({ pageKey, above }: { pageKey: string; above: ReactNode }) {
  useConceptSubscription();
  const feature = featureOfKey(pageKey);
  useGoKeys(feature);
  useEscapeKeys(pageKey);
  const setFeature = useSetAtom(activeFeatureAtom);
  useEffect(() => {
    if (feature !== null) setFeature(feature);
  }, [feature, setFeature]);
  const state = useConceptPage(pageKey);
  if (state.page) return <LoadedPage page={state.page} pageKey={pageKey} above={above} />;
  return (
    <div className="concept-unloaded">
      {above}
      <Unloaded pageKey={pageKey} state={state} />
    </div>
  );
}

/**
 * The overview and summary pages of Product, Engineering and the metaplan. `above` renders in the page's column
 * under the page's head and ahead of its document key, so the feature screen can put its gaps
 * there without a second column or scroll container.
 */
export function ConceptRoute({ above }: { above?: ReactNode }) {
  const { pathname } = useLocation();
  const key = pageKeyOfPath(pathname);
  return (
    <div className="w-full px-[clamp(16px,4vw,56px)] pt-8 pb-12">
      <div className="concept-page-column mx-auto max-w-[68ch]">
        {key === null ? (
          <>
            {above}
            <ConceptNotFound pageKey={rawNameOf(pathname)} />
          </>
        ) : (
          <RouteBody pageKey={key} above={above} />
        )}
      </div>
    </div>
  );
}
