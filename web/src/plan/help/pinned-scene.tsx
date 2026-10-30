import { MousePointer2Icon } from "lucide-react";
import {
  useId,
  useLayoutEffect,
  useRef,
  useState,
  type ComponentType,
  type CSSProperties,
  type JSX,
  type ReactNode,
  type RefObject,
} from "react";

import { anchorBox, CANVAS_MARGIN, PIN_SIZE, placePins, type Box } from "./pin-placement";
import "./pinned-scene.css";

export interface SceneNote {
  target: string | readonly string[];
  text: string;
}

export interface PlannerHelpScene {
  title: string;
  Scene: ComponentType;
  notes: readonly SceneNote[];
  scale?: number;
  /** A chat-panel-width scene: the lifted stage hugs the panel and centres, freeing the gutters. */
  panel?: boolean;
}

interface PinnedSceneProps {
  number: number;
  title: string;
  notes: readonly SceneNote[];
  scale?: number;
  panel?: boolean;
  children: ReactNode;
}

interface Measurements {
  height: number | undefined;
  targets: (Box | null)[];
  pins: (Box | null)[];
}

function targetElements(stage: HTMLElement, target: SceneNote["target"]): Element[] {
  const selectors = typeof target === "string" ? [target] : target;
  return [...new Set(selectors.flatMap((selector) => [...stage.querySelectorAll(selector)]))];
}

/**
 * Rectangles come back in viewport pixels with every ancestor transform applied, and the pins are
 * positioned in the canvas's own pixels. The stage's own `scale` is already in the rectangles, but
 * an ancestor's is not (the dialog zooms in as it opens), so divide it out: `k` is how much the
 * canvas is scaled on screen.
 */
function layoutScale(root: HTMLElement, origin: DOMRect): number {
  return root.offsetWidth > 0 && origin.width > 0 ? origin.width / root.offsetWidth : 1;
}

type Rect = Pick<DOMRect, "left" | "top" | "width" | "height">;

function toCanvasBox(rect: Rect, origin: DOMRect, k: number): Box | null {
  if (rect.width === 0 || rect.height === 0) return null;
  return {
    x: (rect.left - origin.left) / k,
    y: (rect.top - origin.top) / k,
    w: rect.width / k,
    h: rect.height / k,
  };
}

function measureTarget(elements: Element[], origin: DOMRect, k: number): Box | null {
  return anchorBox(
    elements.flatMap((element) => {
      const box = toCanvasBox(element.getBoundingClientRect(), origin, k);
      return box ? [box] : [];
    }),
  );
}

const CONTROLS = "button, a[href], input, select, textarea, [role='button']";
/** A control holding any of these is a wrapper (a card link): its text and inner controls count instead. */
const WRAPPED = `${CONTROLS}, div, p, ol, ul, section, h1, h2, h3, h4, h5, h6`;

/** One rectangle per line of every non-empty text node; none where the browser has no layout. */
function textRects(stage: HTMLElement): Rect[] {
  const range = document.createRange();
  if (typeof range.getClientRects !== "function") return [];
  const walker = document.createTreeWalker(stage, NodeFilter.SHOW_TEXT);
  const rects: Rect[] = [];
  for (let node = walker.nextNode(); node; node = walker.nextNode()) {
    if (!node.nodeValue?.trim()) continue;
    range.selectNodeContents(node);
    rects.push(...range.getClientRects());
  }
  return rects;
}

/** What a pin must not cover: the stage's text runs and its buttons, links and inputs. */
function measureOccupied(stage: HTMLElement, origin: DOMRect, k: number): Box[] {
  const controls = [...stage.querySelectorAll(CONTROLS)]
    .filter((element) => element.querySelector(WRAPPED) === null)
    .map((element) => element.getBoundingClientRect());
  return [...textRects(stage), ...controls].flatMap((rect) => {
    const box = toCanvasBox(rect, origin, k);
    return box ? [box] : [];
  });
}

function usePinTargets(
  canvas: RefObject<HTMLDivElement | null>,
  stage: RefObject<HTMLDivElement | null>,
  notes: readonly SceneNote[],
  scale: number,
): Measurements {
  const [measured, setMeasured] = useState<Measurements>({
    height: undefined,
    targets: [],
    pins: [],
  });
  useLayoutEffect(() => {
    const root = canvas.current;
    const sample = stage.current;
    if (!root || !sample) return;
    const measure = () => {
      const origin = root.getBoundingClientRect();
      const k = layoutScale(root, origin);
      const height = sample.getBoundingClientRect().height / k;
      const targets = notes.map((note) =>
        measureTarget(targetElements(sample, note.target), origin, k),
      );
      const bounds = { width: origin.width / k, height: height + CANVAS_MARGIN * 2 };
      const layout = {
        occupied: measureOccupied(sample, origin, k),
        stage: toCanvasBox(sample.getBoundingClientRect(), origin, k),
      };
      const next = { height, targets, pins: placePins(targets, bounds, PIN_SIZE, layout) };
      // Boxes are plain numbers: an unchanged layout keeps the old state and skips the re-render.
      setMeasured((prev) => (JSON.stringify(prev) === JSON.stringify(next) ? prev : next));
    };
    measure();
    // Production components can finish their layout after their first render.
    const timers = [80, 300, 800].map((delay) => setTimeout(measure, delay));
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    const elements = notes.map((note) => targetElements(sample, note.target));
    for (const element of new Set([root, sample, ...elements.flat()])) observer?.observe(element);
    window.addEventListener("resize", measure);
    return () => {
      timers.forEach(clearTimeout);
      observer?.disconnect();
      window.removeEventListener("resize", measure);
    };
    // `children` is not a dependency: the parent builds a new element on every render (each search
    // keystroke), and the observers already report the stage's own layout changes.
  }, [canvas, stage, notes, scale]);
  return measured;
}

function boxStyle(box: Box): CSSProperties {
  return { left: box.x, top: box.y, width: box.w, height: box.h };
}

function SceneOverlay({ measured, active }: { measured: Measurements; active: number | null }) {
  const ring = active === null ? null : measured.targets[active];
  return (
    <div className="planner-help-scene__overlay" aria-hidden="true">
      {ring && <span className="planner-help-scene__ring" style={boxStyle(ring)} />}
      {measured.pins.map((pin, index) =>
        pin ? (
          <span
            key={index}
            className="planner-help-scene__pin"
            data-active={active === index || undefined}
            style={boxStyle(pin)}
          >
            {index + 1}
          </span>
        ) : null,
      )}
    </div>
  );
}

function SceneNotes({
  notes,
  active,
  onHover,
  onFocus,
}: {
  notes: readonly SceneNote[];
  active: number | null;
  onHover: (index: number | null) => void;
  onFocus: (index: number | null) => void;
}) {
  return (
    <ol className="planner-help-scene__notes" role="list">
      {notes.map((note, index) => (
        <li
          key={index}
          tabIndex={0}
          data-active={active === index || undefined}
          onMouseEnter={() => onHover(index)}
          onMouseLeave={() => onHover(null)}
          onFocus={() => onFocus(index)}
          onBlur={() => onFocus(null)}
        >
          <span className="planner-help-scene__note-number" aria-hidden="true">
            {index + 1}
          </span>
          <p>{note.text}</p>
        </li>
      ))}
    </ol>
  );
}

function SceneHeader({
  number,
  title,
  titleId,
}: {
  number: number;
  title: string;
  titleId: string;
}) {
  return (
    <header className="planner-help-scene__header">
      <span className="planner-help-scene__label">Scene {number}</span>
      <h3 id={titleId}>{title}</h3>
      <span className="planner-help-scene__hint">
        <MousePointer2Icon aria-hidden="true" />
        Hover a note to find it
      </span>
    </header>
  );
}

export function PinnedScene({
  number,
  title,
  notes,
  scale = 1,
  panel = false,
  children,
}: PinnedSceneProps): JSX.Element {
  const titleId = useId();
  const canvas = useRef<HTMLDivElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const measured = usePinTargets(canvas, stage, notes, scale);
  const [hovered, setHovered] = useState<number | null>(null);
  const [focused, setFocused] = useState<number | null>(null);
  const active = hovered ?? focused;
  return (
    <section className="planner-help-scene" aria-labelledby={titleId}>
      <SceneHeader number={number} title={title} titleId={titleId} />
      <div
        className="planner-help-scene__scroll"
        tabIndex={0}
        role="region"
        aria-label={`${title} preview`}
      >
        <div ref={canvas} className="planner-help-scene__canvas" style={{ padding: CANVAS_MARGIN }}>
          <div className="planner-help-scene__frame" style={{ height: measured.height }}>
            <div
              ref={stage}
              className="planner-help-scene__stage"
              data-panel={panel || undefined}
              inert
              style={{ width: panel ? undefined : `${100 / scale}%`, transform: `scale(${scale})` }}
            >
              {children}
            </div>
          </div>
          <SceneOverlay measured={measured} active={active} />
        </div>
      </div>
      <SceneNotes notes={notes} active={active} onHover={setHovered} onFocus={setFocused} />
    </section>
  );
}
