import { useAtomValue, useStore } from "jotai/react";
import { ArrowRight } from "lucide-react";
import { Link, Navigate, useLocation, useParams } from "react-router";

import { FIRST_METAPLAN } from "@/plan/api/page-key";
import type { ChecklistView } from "@/plan/api/schema";
import { ConceptRoute } from "@/plan/concept/concept-route";
import { EmptyOffer } from "@/plan/concept/empty-offer";
import { hashAnchor, repoRel } from "@/plan/files/address";
import { useFileSocket } from "@/plan/files/api";
import { treeAtom } from "@/plan/files/atoms";
import { ReadingColumn } from "@/plan/files/reading-column";
import { metaplanSummaryKey } from "@/plan/files/section-chips";
import { missingDocumentOffer } from "@/plan/feature/page-offer";
import { featureOfDocPath, featureSourceHref } from "@/plan/feature/source-href";
import { metaplanFile } from "@/plan/feature/spine-model";
import { featureHref, useFeature } from "@/plan/feature/use-feature";
import { LoadState } from "@/plan/ui/load-state";

/** A document's source, read in the reading column; `anchor` scrolls to one of its sections. */
function SourcePage({ path, anchor = null }: { path: string; anchor?: string | null }) {
  const store = useStore();
  useFileSocket(store);
  const tree = useAtomValue(treeAtom);
  return (
    <div className="plan-document">
      <ReadingColumn key={path} path={repoRel(path)} anchor={anchor} tree={tree} />
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
  return (
    <>
      <ConceptRoute />
      {/* ConceptRoute draws its own column and bottom padding; this one matches the column and
          pulls the link up under the summary. */}
      <div className="-mt-8 w-full px-[clamp(16px,4vw,56px)] pb-12">
        <div className="mx-auto max-w-[68ch]">
          <Link
            to={featureSourceHref(checklist.feature, `doc/product/${checklist.feature}/index.md`)}
            className="concept-read-source"
          >
            Read the full vision
            <ArrowRight aria-hidden="true" />
          </Link>
        </div>
      </div>
    </>
  );
}

/**
 * `/f/:feature/metaplan[/:file]`: a metaplan's summary page (its Source link opens the file); the
 * first metaplan when no file is named. The summary key is read from the path, where the bare route
 * names `index`, so a first metaplan with another name moves to its own file route. A metaplan
 * whose file name is no valid summary stem has no summary and opens as its source.
 */
export function MetaplanPage() {
  const { checklist } = useFeature();
  const { file } = useParams();
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
  const metaplan = file
    ? checklist.metaplans.find((candidate) => metaplanFile(candidate.path) === file)
    : checklist.metaplans[0];
  if (!metaplan) return <LoadState state="not_found" title="No such metaplan" />;
  // A file name outside the summary stem grammar (`metaplan_02.md`) has no summary: show its source.
  if (metaplanSummaryKey(checklist.feature, metaplan.path) === null) {
    return <SourcePage path={metaplan.path} />;
  }
  const name = metaplanFile(metaplan.path);
  if (!file && name.replace(/\.md$/, "") !== FIRST_METAPLAN) {
    const to = featureHref(checklist.feature, `metaplan/${encodeURIComponent(name)}`);
    return <Navigate replace to={to} />;
  }
  return <ConceptRoute />;
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
        message="Only the feature's vision, sections, metaplans and plans open here."
      />
    );
  }
  return <SourcePage path={path} anchor={hashAnchor(hash)} />;
}
