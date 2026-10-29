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
}

interface PinnedSceneProps {
  number: number;
  title: string;
  notes: readonly SceneNote[];
  scale?: number;
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

function measureTarget(elements: Element[], origin: DOMRect): Box | null {
  return anchorBox(
    elements.flatMap((element) => {
      const rect = element.getBoundingClientRect();
      if (rect.width === 0 || rect.height === 0) return [];
      return [
        { x: rect.left - origin.left, y: rect.top - origin.top, w: rect.width, h: rect.height },
      ];
    }),
  );
}

function usePinTargets(
  canvas: RefObject<HTMLDivElement | null>,
  stage: RefObject<HTMLDivElement | null>,
  notes: readonly SceneNote[],
  scale: number,
  children: ReactNode,
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
      const height = sample.getBoundingClientRect().height;
      const targets = notes.map((note) =>
        measureTarget(targetElements(sample, note.target), origin),
      );
      const bounds = { width: origin.width, height: height + CANVAS_MARGIN * 2 };
      setMeasured({ height, targets, pins: placePins(targets, bounds, PIN_SIZE) });
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
  }, [canvas, stage, notes, scale, children]);
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
      <span className="planner-help-scene__hint">Hover a note to find it</span>
    </header>
  );
}

export function PinnedScene({
  number,
  title,
  notes,
  scale = 1,
  children,
}: PinnedSceneProps): JSX.Element {
  const titleId = useId();
  const canvas = useRef<HTMLDivElement>(null);
  const stage = useRef<HTMLDivElement>(null);
  const measured = usePinTargets(canvas, stage, notes, scale, children);
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
              inert
              style={{ width: `${100 / scale}%`, transform: `scale(${scale})` }}
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
