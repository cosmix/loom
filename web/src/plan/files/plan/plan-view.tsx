import { useAtomValue } from "jotai/react";
import { useCallback, useMemo } from "react";
import { useNavigate } from "react-router";

import type { DocView } from "@/plan/api/schema-files";
import { treeAtom } from "@/plan/files/atoms";
import { PlanHeader } from "@/plan/files/plan/plan-header";
import { splitPlan } from "@/plan/files/plan/split";
import { StagesPanel } from "@/plan/files/plan/stages-panel";
import { docPathsOf } from "@/plan/files/tree";
import { Markdown } from "@/plan/markdown";

/// A plan, rendered: header, the prose before the metadata block, the Stages
/// panel in place of the block, then the pressure-test record. `doc.plan` is set.
export function PlanView({ doc }: { doc: DocView }) {
  const plan = doc.plan!;
  const navigate = useNavigate();
  const tree = useAtomValue(treeAtom);
  const docPaths = useMemo(() => docPathsOf(tree, doc.path), [tree, doc.path]);
  const ownAnchors = useMemo(() => new Set(doc.sections.map((s) => s.anchor)), [doc.sections]);
  const anchorsOf = useCallback(
    (path: string) => (path === doc.path ? ownAnchors : null),
    [doc.path, ownAnchors],
  );
  const onNavigate = useCallback((address: string) => void navigate(address), [navigate]);
  const parts = splitPlan(doc.text, plan.metadata_line);
  const shared = { path: doc.path, sections: doc.sections, onNavigate, docPaths, anchorsOf };
  return (
    <article>
      <PlanHeader doc={doc} plan={plan} />
      <Markdown source={parts.prose} lineOffset={parts.prose_offset} {...shared} />
      <StagesPanel plan={plan} />
      {parts.record !== "" && (
        <Markdown source={parts.record} lineOffset={parts.record_offset} {...shared} />
      )}
    </article>
  );
}
