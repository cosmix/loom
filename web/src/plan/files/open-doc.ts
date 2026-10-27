import { useAtomValue, useStore } from "jotai/react";
import {
  useEffect,
  useRef,
  useState,
  type Dispatch,
  type RefObject,
  type SetStateAction,
} from "react";

import { fetchDoc } from "@/plan/files/api";
import {
  docChangedAtom,
  openDocAtom,
  openDocIsStaleAtom,
  previousOpenTextAtom,
} from "@/plan/files/atoms";

export type Load =
  | { status: "loading" }
  | { status: "ready" }
  | { status: "missing" }
  | { status: "error"; message: string };

type SetLoad = Dispatch<SetStateAction<Load>>;
export type SlugRef = RefObject<{ path: string; slug: string } | null>;

/** A refetch of a changed document failed: what to tell the reader, and how to try again. */
export type StaleRefresh = { message: string; retry: () => void };

const HEADINGS = "h1[id],h2[id],h3[id],h4[id],h5[id],h6[id]";

/** The id of the heading at or just above the top of the column, so a refresh can restore it. */
function headingAtTop(container: HTMLElement | null): string | null {
  if (!container) return null;
  const headings = [...container.querySelectorAll<HTMLElement>(HEADINGS)];
  const top = container.closest("[data-reading-scroll]")?.getBoundingClientRect().top ?? 0;
  const above = headings.filter((heading) => heading.getBoundingClientRect().top <= top + 1);
  return (above.at(-1) ?? headings[0])?.id ?? null;
}

/** Fetches the document at `path` into `openDocAtom`; a new `attempt` fetches it again. */
function useInitialFetch(path: string, attempt: number, setLoad: SetLoad, slug: SlugRef): void {
  const store = useStore();
  useEffect(() => {
    let active = true;
    store.set(openDocAtom, null);
    store.set(previousOpenTextAtom, null);
    setLoad({ status: "loading" });
    void fetchDoc(path).then((result) => {
      if (!active) return;
      if (!result.ok) {
        setLoad(
          result.status === 404
            ? { status: "missing" }
            : { status: "error", message: result.message },
        );
        return;
      }
      const doc = result.value;
      slug.current = doc.plan ? { path, slug: doc.plan.slug.toLowerCase() } : null;
      store.set(openDocAtom, { doc, seenHash: doc.hash });
      setLoad({ status: "ready" });
    });
    return () => {
      active = false;
    };
  }, [path, attempt, store, setLoad, slug]);
}

/**
 * While the document is open, a changed hash (`openDocIsStaleAtom`) refetches in place and
 * records the text it replaced, so the reader keeps their place and can see what changed. A
 * failed refetch leaves the old text up and comes back as a `StaleRefresh`; its Retry, or the
 * next change recorded for the path, refetches again.
 */
function useStaleRefetch(
  path: string,
  container: RefObject<HTMLElement | null>,
  setLoad: SetLoad,
  keep: RefObject<string | null>,
): StaleRefresh | null {
  const store = useStore();
  const stale = useAtomValue(openDocIsStaleAtom);
  const seenHash = useAtomValue(openDocAtom)?.seenHash;
  const changedTo = useAtomValue(docChangedAtom)[path];
  const [failure, setFailure] = useState<{ path: string; message: string } | null>(null);
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    const open = store.get(openDocAtom);
    if (!stale || !open || open.doc.path !== path) return;
    let active = true;
    keep.current = headingAtTop(container.current);
    void fetchDoc(path).then((result) => {
      if (!active) return;
      if (result.ok) {
        setFailure(null);
        store.set(previousOpenTextAtom, { path, text: open.doc.text });
        store.set(openDocAtom, { doc: result.value, seenHash: result.value.hash });
      } else if (result.status === 404) setLoad({ status: "missing" });
      else setFailure({ path, message: result.message });
    });
    return () => {
      active = false;
    };
  }, [stale, seenHash, changedTo, attempt, path, store, container, setLoad, keep]);
  if (!stale || failure?.path !== path) return null;
  return {
    message: failure.message,
    retry: () => {
      setFailure(null);
      setAttempt((n) => n + 1);
    },
  };
}

export function useOpenDoc(path: string, container: RefObject<HTMLElement | null>) {
  const [load, setLoad] = useState<Load>({ status: "loading" });
  const [attempt, setAttempt] = useState(0);
  const keep = useRef<string | null>(null);
  const slug = useRef<{ path: string; slug: string } | null>(null);
  useInitialFetch(path, attempt, setLoad, slug);
  const refresh = useStaleRefetch(path, container, setLoad, keep);
  return { load, retry: () => setAttempt((n) => n + 1), refresh, keep, slug };
}
