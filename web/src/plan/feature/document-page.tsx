import { useAtomValue, useStore } from "jotai/react";
import { useParams } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { EmptyOffer } from "@/plan/concept/empty-offer";
import { repoRel } from "@/plan/files/address";
import { useFileSocket } from "@/plan/files/api";
import { treeAtom } from "@/plan/files/atoms";
import { ReadingColumn } from "@/plan/files/reading-column";
import { missingDocumentOffer } from "@/plan/feature/page-offer";
import { metaplanFile } from "@/plan/feature/spine-model";
import { useFeature } from "@/plan/feature/use-feature";
import { LoadState } from "@/plan/ui/load-state";

/** The vision and the metaplans have no summaries, so these pages read the source. */
function SourcePage({ path }: { path: string }) {
  const store = useStore();
  useFileSocket(store);
  const tree = useAtomValue(treeAtom);
  return (
    <div className="plan-document">
      <ReadingColumn key={path} path={repoRel(path)} anchor={null} tree={tree} />
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

/** `/f/:feature/vision`: the vision's source, or the offer to write it. */
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
  return <SourcePage path={`product/${checklist.feature}/index.md`} />;
}

/** `/f/:feature/metaplan[/:file]`: a metaplan's source; the first one when no file is named. */
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
  return <SourcePage path={metaplan.path} />;
}
