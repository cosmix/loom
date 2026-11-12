import { useCallback, useMemo } from "react";
import { useNavigate } from "react-router";

import type { DocView, FileTree } from "@/plan/api/schema-files";
import { docPathsOf } from "@/plan/files/tree-docs";

/** The link-resolution props every `<Markdown>` of an open document takes. */
export function useDocMarkdownProps(doc: DocView, tree: FileTree | null) {
  const navigate = useNavigate();
  const docPaths = useMemo(() => docPathsOf(tree, doc.path), [tree, doc.path]);
  const anchors = useMemo(() => new Set(doc.sections.map((s) => s.anchor)), [doc.sections]);
  const anchorsOf = useCallback(
    (target: string) => (target === doc.path ? anchors : null),
    [doc.path, anchors],
  );
  const onNavigate = useCallback((address: string) => void navigate(address), [navigate]);
  return { docPaths, anchorsOf, onNavigate };
}
