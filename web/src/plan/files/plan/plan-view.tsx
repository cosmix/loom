import { useAtomValue } from "jotai/react";

import type { DocView } from "@/plan/api/schema-files";
import { treeAtom } from "@/plan/files/atoms";
import { splitPlan } from "@/plan/files/plan/split";
import { StagesPanel } from "@/plan/files/plan/stages-panel";
import { useDocMarkdownProps } from "@/plan/files/use-doc-markdown";
import { Markdown } from "@/plan/markdown";

/// A plan, rendered: the prose before the metadata block, the Stages
/// panel in place of the block, then the pressure-test record. `doc.plan` is set.
export function PlanView({ doc }: { doc: DocView }) {
  const plan = doc.plan!;
  const tree = useAtomValue(treeAtom);
  const links = useDocMarkdownProps(doc, tree);
  const parts = splitPlan(doc.text, plan.metadata_line);
  const shared = { path: doc.path, sections: doc.sections, ...links };
  return (
    <article>
      <Markdown
        source={parts.prose}
        lineOffset={parts.prose_offset}
        suppressTitle={doc.title}
        {...shared}
      />
      <StagesPanel plan={plan} links={{ path: doc.path, ...links }} />
      {parts.record !== "" && (
        <Markdown source={parts.record} lineOffset={parts.record_offset} {...shared} />
      )}
    </article>
  );
}
