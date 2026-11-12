import { useAtomValue } from "jotai/react";
import { ChevronRight } from "lucide-react";
import {
  createContext,
  useContext,
  useEffect,
  useMemo,
  useRef,
  useState,
  type JSX,
  type ReactNode,
} from "react";
import { useInRouterContext, useNavigate } from "react-router";

import type { SectionView } from "@/plan/api/schema-files";
import { treeAtom } from "@/plan/files/atoms";
import { docPathsOf } from "@/plan/files/tree";
import { Markdown } from "@/plan/markdown";

import type { DiffBlock } from "./block-diff";

export interface RenderedDiffProps {
  /** The document's blocks in order, from `diffBlocks`. */
  blocks: DiffBlock[];
  /** Repo-relative path of the document; relative links resolve from it. */
  path: string;
}

/** Unchanged blocks kept in view on each side of a change. */
const CONTEXT = 2;
/** A run that would hide fewer blocks than this stays open: its fold row would save too little. */
const MIN_FOLD = 4;

// Headings carry no id, since both texts hold the same ones, and no section anchors are known.
const NO_SECTIONS: SectionView[] = [];
const noAnchors = (): null => null;
const stay = (): void => {};

/** What a link needs: the documents file mode can open, and where a followed link goes. */
interface Links {
  docPaths: ReadonlySet<string>;
  onNavigate: (address: string) => void;
}
const LinksContext = createContext<Links>({ docPaths: new Set(), onNavigate: stay });

/** Links go to the planner's router, as in the chat. */
function RouterLinks(props: { docPaths: ReadonlySet<string>; children: ReactNode }): JSX.Element {
  const navigate = useNavigate();
  const { docPaths } = props;
  const links = useMemo<Links>(
    () => ({
      docPaths,
      onNavigate: (address) => {
        void navigate(address);
      },
    }),
    [docPaths, navigate],
  );
  return <LinksContext.Provider value={links}>{props.children}</LinksContext.Provider>;
}

const FIRST_CHANGE = '[data-diff-block]:not([data-diff-block="same"])';

/** The height a block has to show in: the nearest box around it that scrolls, else the window. */
function roomFor(element: HTMLElement): number {
  for (let box = element.parentElement; box; box = box.parentElement) {
    const overflow = getComputedStyle(box).overflowY;
    const scrolls = overflow === "auto" || overflow === "scroll";
    if (scrolls && box.scrollHeight > box.clientHeight) return box.clientHeight;
  }
  return window.innerHeight;
}

type Row =
  | { type: "block"; index: number; block: DiffBlock }
  | { type: "fold"; index: number; blocks: DiffBlock[] };

/** The blocks as rows, with each long run of unchanged blocks away from a change folded. */
function rowsOf(blocks: DiffBlock[]): Row[] {
  const rows: Row[] = [];
  const show = (from: number, to: number): void => {
    for (let index = from; index < to; index++) {
      rows.push({ type: "block", index, block: blocks[index] });
    }
  };
  // With nothing changed there is no change to keep context for: the document reads whole.
  if (blocks.every((block) => block.kind === "same")) {
    show(0, blocks.length);
    return rows;
  }
  let start = 0;
  while (start < blocks.length) {
    let end = start + 1;
    if (blocks[start].kind === "same") {
      while (end < blocks.length && blocks[end].kind === "same") end++;
      const from = start === 0 ? start : start + CONTEXT;
      const to = end === blocks.length ? end : end - CONTEXT;
      if (to - from >= MIN_FOLD) {
        show(start, from);
        rows.push({ type: "fold", index: from, blocks: blocks.slice(from, to) });
        show(to, end);
        start = end;
        continue;
      }
    }
    show(start, end);
    start = end;
  }
  return rows;
}

function BlockText({ source, path }: { source: string; path: string }): JSX.Element {
  const { docPaths, onNavigate } = useContext(LinksContext);
  return (
    <Markdown
      source={source}
      path={path}
      sections={NO_SECTIONS}
      docPaths={docPaths}
      anchorsOf={noAnchors}
      onNavigate={onNavigate}
      headingIds={false}
    />
  );
}

/** One side of a change: the removed text in a `del`, the added text in an `ins`. */
function Side(props: { side: "before" | "after"; label: string; source: string; path: string }) {
  const Tag = props.side === "before" ? "del" : "ins";
  return (
    <Tag className="diff-side" data-diff-side={props.side}>
      <span className="diff-side__mark" aria-hidden="true">
        {props.side === "before" ? "−" : "+"}
      </span>
      <span className="sr-only">{props.label}</span>
      <BlockText source={props.source} path={props.path} />
    </Tag>
  );
}

function Block({ block, path }: { block: DiffBlock; path: string }): JSX.Element {
  const changed = block.kind === "changed";
  return (
    <div className="diff-block" data-diff-block={block.kind}>
      {block.kind === "same" && <BlockText source={block.after ?? ""} path={path} />}
      {block.kind !== "same" && block.before !== null && (
        <Side
          side="before"
          label={changed ? "Changed from:" : "Removed:"}
          source={block.before}
          path={path}
        />
      )}
      {block.kind !== "same" && block.after !== null && (
        <Side
          side="after"
          label={changed ? "Changed to:" : "Added:"}
          source={block.after}
          path={path}
        />
      )}
    </div>
  );
}

function Fold(props: {
  blocks: DiffBlock[];
  path: string;
  open: boolean;
  onToggle: () => void;
}): JSX.Element {
  const count = props.blocks.length;
  return (
    <>
      <button
        type="button"
        className="diff-fold"
        aria-expanded={props.open}
        onClick={props.onToggle}
      >
        <ChevronRight size={14} aria-hidden="true" />
        <span>
          {count} unchanged {count === 1 ? "block" : "blocks"}
        </span>
      </button>
      {props.open &&
        props.blocks.map((block, offset) => <Block key={offset} block={block} path={props.path} />)}
    </>
  );
}

/** The opened folds of one list of blocks; a new list starts with every fold closed. */
function useOpenFolds(blocks: DiffBlock[]): [ReadonlySet<number>, (index: number) => void] {
  const [state, setState] = useState<{ blocks: DiffBlock[]; open: ReadonlySet<number> }>({
    blocks,
    open: new Set(),
  });
  const open = state.blocks === blocks ? state.open : new Set<number>();
  const toggle = (index: number): void => {
    const next = new Set(open);
    if (!next.delete(index)) next.add(index);
    setState({ blocks, open: next });
  };
  return [open, toggle];
}

/**
 * The diff as the rendered document: unchanged blocks read as the document does, an added or a
 * removed block is marked in the margin, and a changed block shows its text before and after.
 */
export function RenderedDiff(props: RenderedDiffProps): JSX.Element {
  const tree = useAtomValue(treeAtom);
  const docPaths = useMemo(() => docPathsOf(tree, props.path), [tree, props.path]);
  const routed = useInRouterContext();
  const unrouted = useMemo<Links>(() => ({ docPaths, onNavigate: stay }), [docPaths]);
  // Outside a router (a bare DiffView) a followed link has nowhere to go.
  const body = <DiffBody {...props} />;
  if (routed) return <RouterLinks docPaths={docPaths}>{body}</RouterLinks>;
  return <LinksContext.Provider value={unrouted}>{body}</LinksContext.Provider>;
}

function DiffBody({ blocks, path }: RenderedDiffProps): JSX.Element {
  const body = useRef<HTMLDivElement>(null);
  const rows = useMemo(() => rowsOf(blocks), [blocks]);
  const [open, toggle] = useOpenFolds(blocks);

  // On open only: once the reader is in the text, a change to it must not move them. The jump is
  // instant, so it is the same under `prefers-reduced-motion`.
  useEffect(() => {
    const first = body.current?.querySelector<HTMLElement>(FIRST_CHANGE);
    if (!first) return;
    // Centring a block taller than its room would put its first lines out of view.
    const block = first.offsetHeight > roomFor(first) ? "start" : "center";
    // jsdom has no scrollIntoView, hence the optional call.
    first.scrollIntoView?.({ block, behavior: "instant" });
  }, []);

  return (
    // Focusable, so the keyboard can scroll a diff that holds no link and no fold row.
    <div
      ref={body}
      className="diff-rendered"
      role="group"
      aria-label="Rendered changes"
      tabIndex={0}
    >
      {rows.map((row) =>
        row.type === "block" ? (
          <Block key={row.index} block={row.block} path={path} />
        ) : (
          <Fold
            key={row.index}
            blocks={row.blocks}
            path={path}
            open={open.has(row.index)}
            onToggle={() => toggle(row.index)}
          />
        ),
      )}
    </div>
  );
}
