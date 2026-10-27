import type { SectionView } from "@/plan/api/schema-files";
import { editorProvenance } from "@/plan/editor/editor-provenance";

/** Builds source provenance for a selection on either side of a diff. */
export function diffProvenance(path: string, sections: SectionView[]) {
  return editorProvenance(path, sections, []);
}
