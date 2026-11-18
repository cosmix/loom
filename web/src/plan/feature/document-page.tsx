import { useAtomValue, useStore } from "jotai/react";
import { ArrowRight, FileText } from "lucide-react";
import { Link, useLocation, useParams } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { ConceptRoute } from "@/plan/concept/concept-route";
import { LedgerRow, LedgerStatus } from "@/plan/concept/header-ledger";
import { EmptyOffer } from "@/plan/concept/empty-offer";
import { Gaps } from "@/plan/feature/gaps";
import { FindingsRow, RunRow } from "@/plan/feature/run-panel";
import { hashAnchor, repoRel } from "@/plan/files/address";
import { useFileSocket } from "@/plan/files/api";
import { openDocAtom, treeAtom } from "@/plan/files/atoms";
import { ReadingColumn } from "@/plan/files/reading-column";
import { missingDocumentOffer } from "@/plan/feature/page-offer";
import { SettleAllAfterDrafting } from "@/plan/feature/settle-gate";
import { featureOfDocPath, featureSourceHref } from "@/plan/feature/source-href";
import { useFeature } from "@/plan/feature/use-feature";
import { BriefsCard } from "@/plan/trace/briefs-card";
import { ScopeLine } from "@/plan/trace/scope-line";
import { docIsLocked, SettleControl } from "@/plan/trace/settle-control";
import { LoadState } from "@/plan/ui/load-state";

/**
 * The document's scope standfirst, settle control and a plan's briefs, above its source; the hash
 * is the opened document's.
 */
function SourceTrace({ feature, path }: { feature: string; path: string }) {
  const open = useAtomValue(openDocAtom);
  const doc = open?.doc.path === repoRel(path) ? open.doc : null;
  const locked = docIsLocked(doc);
  return (
    <div className="trace-source-head">
      <ScopeLine feature={feature} path={path} />
      <SettleControl feature={feature} path={path} hash={doc?.hash ?? null} locked={locked} />
      <BriefsCard feature={feature} path={path} />
    </div>
  );
}

/** A document's source, read in the reading column; `anchor` scrolls to one of its sections. */
function SourcePage(props: { feature: string; path: string; anchor?: string | null }) {
  const { feature, path, anchor = null } = props;
  const store = useStore();
  useFileSocket(store);
  const tree = useAtomValue(treeAtom);
  return (
    <div className="plan-document">
      {/* Both restart on a new path; siblings must not share the one key. */}
      <SourceTrace key={`trace:${path}`} feature={feature} path={path} />
      <ReadingColumn key={`column:${path}`} path={repoRel(path)} anchor={anchor} tree={tree} />
    </div>
  );
}

/** One sentence and the offer that writes the document; the spine leaves that offer to this page. */
function MissingDocument({
  checklist,
  page,
  sentence,
}: {
  checklist: ChecklistView;
  page: string;
  sentence: string;
}) {
  const offer = missingDocumentOffer(checklist, page);
  return (
    <div className="plan-document plan-document--empty">
      <p className="plan-empty">{sentence}</p>
      {offer && <EmptyOffer offer={offer.id} checklist={checklist} />}
    </div>
  );
}

/** The vision summary page's Source row: the way to the whole document. */
function VisionSourceRow({ feature }: { feature: string }) {
  return (
    <LedgerRow
      label="Source"
      action={
        <Link
          to={featureSourceHref(feature, `doc/product/${feature}/index.md`)}
          data-look="secondary"
          className="plan-button inline-flex shrink-0 items-center justify-center"
        >
          Read the full vision
          <ArrowRight aria-hidden="true" />
        </Link>
      }
    >
      <LedgerStatus glyph={<FileText />}>The full vision</LedgerStatus>
    </LedgerRow>
  );
}

/** `/f/:feature/vision`: the vision's summary page, or the offer to write the vision. */
export function VisionPage() {
  const { checklist } = useFeature();
  if (!checklist) return <LoadState state="loading" title="Loading the feature" />;
  if (!checklist.has_product) {
    return (
      <MissingDocument
        checklist={checklist}
        page="vision"
        sentence="This feature has no vision yet."
      />
    );
  }
  return <ConceptRoute ledger={<VisionSourceRow feature={checklist.feature} />} />;
}

/**
 * `/f/:feature/metaplan`: the Plans home, with the run, the bulk settle action and the findings as
 * rows of its heading's ledger and the gaps above its summary (its Source link opens `index.md`),
 * or the offer to write the metaplan.
 */
export function MetaplanPage() {
  const { checklist } = useFeature();
  if (!checklist) return <LoadState state="loading" title="Loading the feature" />;
  if (checklist.metaplans.length === 0) {
    return (
      <MissingDocument
        checklist={checklist}
        page="metaplan"
        sentence="This feature has no metaplan yet."
      />
    );
  }
  return (
    <ConceptRoute
      ledger={
        <>
          <RunRow />
          <SettleAllAfterDrafting feature={checklist.feature} role="plan" />
          <FindingsRow />
        </>
      }
      above={<Gaps scope="metaplan" />}
    />
  );
}

/**
 * `/f/:feature/source/*`: the source of one of the feature's documents (its vision, anything under
 * `doc/product/<feature>/` or `doc/plans/<feature>/`), with the spine around it. Any other path is
 * refused rather than shown under this feature.
 */
export function FeatureSourcePage() {
  const { checklist } = useFeature();
  const { "*": rel = "" } = useParams();
  const { hash } = useLocation();
  if (!checklist) return <LoadState state="loading" title="Loading the feature" />;
  const path = `doc/${rel}`;
  if (featureOfDocPath(path) !== checklist.feature) {
    return (
      <LoadState
        state="error"
        title="Not a document of this feature"
        message="Only the feature's vision, sections, metaplan and plans open here."
      />
    );
  }
  return <SourcePage feature={checklist.feature} path={path} anchor={hashAnchor(hash)} />;
}
