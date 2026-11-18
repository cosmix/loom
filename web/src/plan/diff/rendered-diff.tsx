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
import { docPathsOf } from "@/plan/files/tree-docs";
import { Markdown } from "@/plan/markdown";

import type { DiffBlock } from "./block-diff";
import {
  definitionsOf,
  isDefinitionBlock,
  NO_DEFINITIONS,
  withDefinitions,
  type DiffSide,
} from "./definitions";

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

/** Each side's link reference definitions: a block is drawn alone, so it needs its document's. */
const DefinitionsContext = createContext(NO_DEFINITIONS);

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

/** The nearest box around the element that scrolls, or null where the window does. */
function scrollerOf(element: HTMLElement): HTMLElement | null {
  for (let box = element.parentElement; box; box = box.parentElement) {
    const overflow = getComputedStyle(box).overflowY;
    const scrolls = overflow === "auto" || overflow === "scroll";
    if (scrolls && box.scrollHeight > box.clientHeight) return box;
  }
  return null;
}

/**
 * How much of the top of the scrolling box the head hides: its height where it sticks inside that
 * box (the diff page, a changes list on a phone), nothing where the box is the diff itself and
 * the head sits above it.
 */
function headCover(element: HTMLElement, scroller: HTMLElement | null): number {
  const head = element.closest(".diff-view")?.querySelector<HTMLElement>(".diff-head");
  if (!head || getComputedStyle(head).position !== "sticky") return 0;
  return scroller && !scroller.contains(head) ? 0 : head.offsetHeight;
}

/**
 * Centres the block in what the head leaves of its box, or puts its start under the head when it
 * is taller. The block's scroll margin counts towards where it lands, so the 48px margin that
 * clears the head for a start jump would push a centred block 24px low where nothing covers it:
 * a top margin of exactly the cover and none below centres in the visible part. The margins are
 * set for the jump only. The jump is instant, so it is the same under `prefers-reduced-motion`.
 */
function scrollToChange(first: HTMLElement): void {
  const scroller = scrollerOf(first);
  const cover = headCover(first, scroller);
  const room = (scroller ? scroller.clientHeight : window.innerHeight) - cover;
  const fits = first.offsetHeight <= room;
  const { scrollMarginTop, scrollMarginBottom } = first.style;
  first.style.scrollMarginTop = fits ? `${cover}px` : `max(${cover}px, var(--space-7))`;
  first.style.scrollMarginBottom = "0px";
  // jsdom has no scrollIntoView, hence the optional call.
  first.scrollIntoView?.({ block: fits ? "center" : "start", behavior: "instant" });
  first.style.scrollMarginTop = scrollMarginTop;
  first.style.scrollMarginBottom = scrollMarginBottom;
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

function BlockText(props: { source: string; path: string; side: DiffSide }): JSX.Element {
  const { path, side } = props;
  const { docPaths, onNavigate } = useContext(LinksContext);
  const definitions = useContext(DefinitionsContext)[side];
  const source = useMemo(
    () => withDefinitions(props.source, definitions),
    [props.source, definitions],
  );
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
function Side(props: { side: DiffSide; label: string; source: string; path: string }) {
  const Tag = props.side === "before" ? "del" : "ins";
  return (
    <Tag className="diff-side" data-diff-side={props.side}>
      <span className="diff-side__mark" aria-hidden="true">
        {props.side === "before" ? "−" : "+"}
      </span>
      <span className="sr-only">{props.label}</span>
      {isDefinitionBlock(props.source) ? (
        // Markdown draws a definition as nothing, which would leave a change with no text.
        <code className="diff-side__definition">{props.source}</code>
      ) : (
        <BlockText source={props.source} path={props.path} side={props.side} />
      )}
    </Tag>
  );
}

function Block({ block, path }: { block: DiffBlock; path: string }): JSX.Element | null {
  const changed = block.kind === "changed";
  // An unchanged definition has nothing to show; it only feeds the links of the blocks around it.
  if (block.kind === "same" && isDefinitionBlock(block.after ?? "")) return null;
  return (
    <div className="diff-block" data-diff-block={block.kind}>
      {block.kind === "same" && <BlockText source={block.after ?? ""} path={path} side="after" />}
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
  const definitions = useMemo(() => definitionsOf(blocks), [blocks]);
  const [open, toggle] = useOpenFolds(blocks);

  // On open only: once the reader is in the text, a change to it must not move them.
  useEffect(() => {
    const first = body.current?.querySelector<HTMLElement>(FIRST_CHANGE);
    if (first) scrollToChange(first);
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
      <DefinitionsContext.Provider value={definitions}>
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
      </DefinitionsContext.Provider>
    </div>
  );
}
