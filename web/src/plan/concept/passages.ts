import { atom, type Atom, type createStore } from "jotai";
import { apiGet } from "@/plan/api/client";
import { docViewSchema, type DocView } from "@/plan/api/schema-files";
import { docRel } from "@/plan/files/address";

type Store = ReturnType<typeof createStore>;

type Passage = {
  title: string;
  anchor: string;
  startLine: number;
  endLine: number;
  markdown: string;
  hash: string;
};

export function passageOf(doc: DocView, anchor: string): Passage | null {
  const section = anchor === "" ? undefined : doc.sections.find((item) => item.anchor === anchor);
  if (anchor !== "" && !section) return null;

  const lines = doc.text.split("\n");
  const startLine = section?.start_line ?? 1;
  const endLine = section?.end_line ?? (doc.sections[0]?.start_line ?? lines.length + 1) - 1;
  return {
    title: section?.title ?? doc.title,
    anchor,
    startLine,
    endLine,
    markdown: lines.slice(startLine - 1, endLine).join("\n"),
    hash: `${doc.hash}#${anchor}`,
  };
}

function makeDocView(repoPath: string) {
  const revision = atom(0);
  let loaded = false;
  const view = atom(async (get): Promise<DocView> => {
    get(revision);
    loaded = true;
    const result = await apiGet(
      `/api/doc?path=${encodeURIComponent(docRel(repoPath))}`,
      docViewSchema,
    );
    if (!result.ok) throw new Error(result.message);
    return result.value;
  });
  return { revision, view, isLoaded: () => loaded };
}

const documents = new Map<string, ReturnType<typeof makeDocView>>();

function documentFor(repoPath: string) {
  let document = documents.get(repoPath);
  if (!document) {
    document = makeDocView(repoPath);
    documents.set(repoPath, document);
  }
  return document;
}

export function docViewAtom(repoPath: string): Atom<Promise<DocView>> {
  return documentFor(repoPath).view;
}

export function refreshDocView(store: Store, repoPath: string): void {
  const document = documentFor(repoPath);
  store.set(document.revision, (revision) => revision + 1);
}

export function refreshAllDocViews(store: Store): void {
  for (const document of documents.values()) {
    if (document.isLoaded()) store.set(document.revision, (revision) => revision + 1);
  }
}
