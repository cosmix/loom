import { useAtomValue } from "jotai";
import { useEffect, useState, type ReactNode } from "react";
import { Link, useParams } from "react-router";

import {
  featureOfKey,
  isOverviewKey,
  overviewKey,
  pageHref,
  parsePageKey,
} from "@/plan/api/page-key";
import type { ConceptPage, DocLink, EmptyState as EmptyData } from "@/plan/api/schema-concept";
import { Button } from "@/components/ui/button";
import { Untrusted, visibleControls } from "@/plan/chat/visible-controls";
import { toAddress } from "@/plan/files/address";
import { fetchDoc } from "@/plan/files/api";
import { useFeature } from "@/plan/feature/use-feature";
import { checklistAtom } from "@/plan/state/atoms";
import { plural } from "@/plan/ui/plural";
import { LoadState } from "@/plan/ui/load-state";

import "./concept-page.css";
import { EmptyOffer } from "./empty-offer";

type Copy = { title: string; description?: string };

function copyFor(page: ConceptPage, empty: EmptyData): Copy {
  const building = page.building;
  switch (empty.kind) {
    case "no_vision":
      return { title: "No product vision yet" };
    case "sections_no_vision":
      return {
        title: "No product overview yet",
        description: "The sections below have no vision to tie them together.",
      };
    case "no_metaplan":
      return { title: "No metaplan yet" };
    case "plans_no_metaplan":
      return {
        title: "No engineering overview yet",
        description: "Plans exist, but no metaplan names them.",
      };
    case "never_built":
      return {
        title: "Summaries not built",
        // The server's estimate covers every missing summary: it describes an overview only.
        description: `${plural(empty.count ?? 0, "document")} without a summary.${
          page.estimate && isOverviewKey(page.key) ? ` ${visibleControls(page.estimate)}` : ""
        }`,
      };
    case "building":
      return {
        title:
          building === null
            ? "Building summaries"
            : `Building ${building.done} of ${building.total}`,
      };
  }
}

/** Sections and plans list to their summary pages; never-built documents open in file mode. */
function DocList({ kind, docs }: { kind: EmptyData["kind"]; docs: DocLink[] }) {
  if (docs.length === 0) return null;
  const href = (doc: DocLink) =>
    kind === "never_built" ? toAddress(doc.path) : pageHref(doc.page);
  return (
    <ul aria-label="Documents" className="plan-list concept-empty-docs">
      {docs.map((doc) => (
        <li key={doc.id}>
          <Link to={href(doc)} className="no-underline hover:underline">
            <Untrusted text={doc.title} />
          </Link>
          <span className="plan-fine font-mono">
            <Untrusted text={doc.path} />
          </span>
        </li>
      ))}
    </ul>
  );
}

/** `.plan-empty` text: the title in the foreground, its explanation on the line below. */
function EmptyNote({ title, description }: Copy) {
  return (
    <p className="plan-empty">
      <strong className="concept-empty-title">{title}</strong>
      {description}
    </p>
  );
}

/**
 * The checklist whose offers this screen may draw: the route feature's, so the offer comes from
 * the feature on screen (`checklistAtom` follows the active feature, which an effect sets and
 * which differs from the route's while several features exist). A standalone page has no route
 * feature, so the active one is the only candidate there.
 */
function useScreenChecklist() {
  const { feature } = useParams();
  const routed = useFeature().checklist;
  const active = useAtomValue(checklistAtom);
  return feature === undefined ? active : routed;
}

/** The page's empty or partial state, with section 05's text for its kind. */
export function EmptyStates({ page, empty }: { page: ConceptPage; empty: EmptyData }) {
  const checklist = useScreenChecklist();
  const copy = copyFor(page, empty);
  // The server lists no documents for a never-built overview; its key chips are them. A document
  // summary page lists none: its key is the whole feature, and the heading reaches its document.
  const keyDocs =
    empty.kind === "never_built" && isOverviewKey(page.key)
      ? page.key_chips.map((chip) => chip.doc)
      : [];
  const docs = empty.docs.length > 0 ? empty.docs : keyDocs;
  // The server sends no offer with a plans-without-metaplan list; section 05 still offers one.
  const offer = empty.offer ?? (empty.kind === "plans_no_metaplan" ? "start-metaplan" : null);
  let action: ReactNode = null;
  if (empty.kind !== "building" && offer) {
    action = <EmptyOffer offer={offer} checklist={checklist ?? null} />;
  }
  return (
    <div className="concept-empty" role={empty.kind === "building" ? "status" : undefined}>
      <EmptyNote {...copy} />
      {action}
      <DocList kind={empty.kind} docs={docs} />
    </div>
  );
}

/** The source document behind a summary page key, or null for an overview. */
function sourceOf(pageKey: string): string | null {
  const key = parsePageKey(pageKey);
  if (key === null || key.kind === "overview") return null;
  if (key.kind === "standalone") return `doc/plans/PLAN-${key.doc}.md`;
  return key.scope === "product"
    ? `doc/product/${key.feature}/${key.doc}.md`
    : `doc/plans/${key.feature}/PLAN-${key.doc}.md`;
}

/** Null until the document lookup answers. */
function useDocExists(path: string | null): boolean | null {
  const [found, setFound] = useState<{ path: string; exists: boolean } | null>(null);
  useEffect(() => {
    if (path === null) return;
    let live = true;
    void fetchDoc(path).then((result) => {
      if (live) setFound({ path, exists: result.ok });
    });
    return () => {
      live = false;
    };
  }, [path]);
  return found?.path === path ? found.exists : null;
}

/** A summary page the server does not have: the document may exist without a summary. */
export function ConceptNotFound({ pageKey }: { pageKey: string }) {
  const source = sourceOf(pageKey);
  const exists = useDocExists(source);
  const feature = featureOfKey(pageKey);
  const name = pageKey.split("/").pop() ?? pageKey;
  if (source !== null && exists === null) return <LoadState state="loading" />;
  if (source !== null && exists) {
    return (
      <LoadState state="empty" title={`No summary for ${visibleControls(name)}`}>
        <Button asChild data-look="secondary" className="plan-button">
          <Link to={toAddress(source)}>Open in file mode</Link>
        </Button>
      </LoadState>
    );
  }
  return (
    <LoadState state="not_found" title="No such document" message={`${name} does not exist.`}>
      {feature !== null ? (
        <nav aria-label="Overviews" className="concept-empty-links">
          <Link
            to={pageHref(overviewKey(feature, "product"))}
            className="no-underline hover:underline"
          >
            Product overview
          </Link>
          <Link
            to={pageHref(overviewKey(feature, "engineering"))}
            className="no-underline hover:underline"
          >
            Engineering overview
          </Link>
        </nav>
      ) : (
        <Link to="/" className="concept-empty-links no-underline hover:underline">
          Planner
        </Link>
      )}
    </LoadState>
  );
}
