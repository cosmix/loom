import { FileTextIcon, HammerIcon, InboxIcon } from "lucide-react";
import type { LucideIcon } from "lucide-react";
import { useEffect, useState, type ReactNode } from "react";
import { Link } from "react-router";

import { BusyRoundel } from "@/aurora-ui/feedback/BusyRoundel";
import { EmptyState } from "@/aurora-ui/feedback/EmptyState";
import { featureOfKey, overviewKey, pageHref, parsePageKey } from "@/plan/api/page-key";
import type { ConceptPage, DocLink, EmptyState as EmptyData } from "@/plan/api/schema-concept";
import { Button } from "@/components/ui/button";
import { toAddress } from "@/plan/files/address";
import { fetchDoc } from "@/plan/files/api";
import { LoadState } from "@/plan/ui/load-state";

import { EmptyOffer } from "./empty-offer";

type Copy = { icon: LucideIcon; title: string; description?: string };

function plural(count: number, noun: string): string {
  return `${count} ${noun}${count === 1 ? "" : "s"}`;
}

function copyFor(page: ConceptPage, empty: EmptyData): Copy {
  const building = page.building;
  switch (empty.kind) {
    case "no_vision":
      return { icon: InboxIcon, title: "No product vision yet" };
    case "sections_no_vision":
      return {
        icon: FileTextIcon,
        title: "No product overview yet",
        description: "The sections below have no vision to tie them together.",
      };
    case "no_metaplan":
      return { icon: InboxIcon, title: "No metaplan yet" };
    case "plans_no_metaplan":
      return {
        icon: FileTextIcon,
        title: "No engineering overview yet",
        description: "Plans exist, but no metaplan names them.",
      };
    case "never_built":
      return {
        icon: FileTextIcon,
        title: "Summaries not built",
        description: `${plural(empty.count ?? 0, "document")} without a summary.${
          page.estimate ? ` ${page.estimate}` : ""
        }`,
      };
    case "building":
      return {
        icon: HammerIcon,
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
    <ul
      aria-label="Documents"
      className="mx-auto flex max-w-[70ch] flex-col gap-1 pt-4 text-[13px]"
    >
      {docs.map((doc) => (
        <li key={doc.id}>
          <Link to={href(doc)} className="underline">
            {doc.title}
          </Link>{" "}
          <span className="font-mono text-xs text-muted-foreground">{doc.path}</span>
        </li>
      ))}
    </ul>
  );
}

/** The page's empty or partial state, with section 05's text for its kind. */
export function EmptyStates({ page, empty }: { page: ConceptPage; empty: EmptyData }) {
  const copy = copyFor(page, empty);
  // The server lists no documents for a never-built overview; its key chips are them.
  const keyDocs = empty.kind === "never_built" ? page.key_chips.map((chip) => chip.doc) : [];
  const docs = empty.docs.length > 0 ? empty.docs : keyDocs;
  // The server sends no offer with a plans-without-metaplan list; section 05 still offers one.
  const offer = empty.offer ?? (empty.kind === "plans_no_metaplan" ? "start-metaplan" : null);
  let action: ReactNode = null;
  if (empty.kind === "building") action = <BusyRoundel busy busyLabel={copy.title} />;
  else if (offer) action = <EmptyOffer offer={offer} />;
  return (
    <div className="px-4 py-6">
      <EmptyState
        icon={copy.icon}
        title={copy.title}
        description={copy.description}
        action={action}
      />
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
      <EmptyState
        icon={FileTextIcon}
        title={`No summary for ${name}`}
        action={
          <Button asChild>
            <Link to={toAddress(source)}>Open in file mode</Link>
          </Button>
        }
      />
    );
  }
  return (
    <div className="flex flex-col gap-3">
      <LoadState state="not_found" title="No such document" message={`${name} does not exist.`} />
      {feature !== null ? (
        <nav aria-label="Overviews" className="flex justify-center gap-4 text-[13px]">
          <Link to={pageHref(overviewKey(feature, "product"))} className="underline">
            Product overview
          </Link>
          <Link to={pageHref(overviewKey(feature, "engineering"))} className="underline">
            Engineering overview
          </Link>
        </nav>
      ) : (
        <Link to="/" className="text-center underline">
          Planner
        </Link>
      )}
    </div>
  );
}
