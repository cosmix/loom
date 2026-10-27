import { useAtomValue } from "jotai/react";
import { useCallback, useMemo } from "react";
import { useNavigate } from "react-router";

import type { DocView, FileTree, TreeEntry } from "@/plan/api/schema-files";
import { treeAtom } from "@/plan/files/atoms";
import { PlanHeader } from "@/plan/files/plan/plan-header";
import { splitPlan } from "@/plan/files/plan/split";
import { StagesPanel } from "@/plan/files/plan/stages-panel";
import { Markdown } from "@/plan/markdown";

function collectPaths(entries: TreeEntry[], into: Set<string>): void {
  for (const entry of entries) {
    if (entry.path !== null) into.add(entry.path);
    collectPaths(entry.children, into);
  }
}

function docPathsOf(tree: FileTree | null, own: string): Set<string> {
  const paths = new Set([own]);
  if (tree === null) return paths;
  for (const group of [tree.product, tree.plans, tree.knowledge, tree.other, tree.archive]) {
    collectPaths(group, paths);
  }
  return paths;
}

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
