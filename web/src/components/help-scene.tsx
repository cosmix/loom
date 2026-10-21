import { useLayoutEffect, useRef, useState, type CSSProperties, type ReactNode } from "react";

import type { HelpNote, HelpScene, PinPlace } from "@/components/help-topics";
import "./help-scene.css";

interface Box {
  x: number;
  y: number;
  w: number;
  h: number;
}

const PIN = 22;

/// An illustration with numbered pins on the elements its notes explain.
/// Hovering a note or its pin rings the element it points at.
export function SceneFigure({
  scene,
  index,
  children,
}: {
  scene: HelpScene;
  index: number;
  children: ReactNode;
}) {
  const stage = useRef<HTMLDivElement>(null);
  const boxes = usePinTargets(stage, scene.notes);
  const [active, setActive] = useState<number | null>(null);
  const width = stage.current?.clientWidth ?? 0;
  const ring = active === null ? null : boxes[active];
  return (
    <section className="help-scene help-reveal" style={{ "--i": index + 1 } as CSSProperties}>
      <h3>{scene.title}</h3>
      <div className="help-scene-canvas">
        <div ref={stage} className="help-scene-stage">
          {children}
          <div className="help-scene-overlay" aria-hidden="true">
            {ring && (
              <span
                className="help-ring"
                style={{ left: ring.x - 4, top: ring.y - 4, width: ring.w + 8, height: ring.h + 8 }}
              />
            )}
            {scene.notes.map((note, i) => {
              const box = boxes[i];
              if (!box) return null;
              return (
                <span
                  key={note.at}
                  className="help-pin"
                  data-active={active === i || undefined}
                  style={pinPosition(box, note.place ?? "t", width)}
                  onMouseEnter={() => setActive(i)}
                  onMouseLeave={() => setActive(null)}
                >
                  {i + 1}
                </span>
              );
            })}
          </div>
        </div>
      </div>
      <ol className="help-notes">
        {scene.notes.map((note, i) => (
          <li
            key={note.at}
            data-active={active === i || undefined}
            onMouseEnter={() => setActive(i)}
            onMouseLeave={() => setActive(null)}
          >
            <span className="help-note-number" aria-hidden="true">
              {i + 1}
            </span>
            <p>{note.text}</p>
          </li>
        ))}
      </ol>
    </section>
  );
}

function pinPosition(box: Box, place: PinPlace, width: number): CSSProperties {
  const half = PIN / 2;
  const middle = box.y + box.h / 2 - half;
  const spots: Record<PinPlace, [number, number]> = {
    t: [box.x + box.w / 2 - half, box.y - PIN - 6],
    tl: [box.x - half, box.y - half],
    tr: [box.x + box.w - half, box.y - half],
    l: [box.x - PIN - 6, middle],
    r: [box.x + box.w + 6, middle],
  };
  const [left, top] = spots[place];
  // Pins may reach into the canvas padding around the stage, never past it.
  const bleed = 14;
  return {
    left: Math.max(-bleed, Math.min(left, width - PIN + bleed)),
    top: Math.max(-bleed - 6, top),
  };
}

/// Measures each note's target relative to the stage. The graph lays itself
/// out after mount, so the first frames are measured again on a short timer.
function usePinTargets(stage: React.RefObject<HTMLDivElement | null>, notes: HelpNote[]) {
  const [boxes, setBoxes] = useState<(Box | null)[]>([]);
  useLayoutEffect(() => {
    const root = stage.current;
    if (!root) return;
    const measure = () => {
      const origin = root.getBoundingClientRect();
      setBoxes(
        notes.map(({ at }) => {
          const rect = root.querySelector(at)?.getBoundingClientRect();
          if (!rect || rect.width === 0) return null;
          return {
            x: rect.left - origin.left,
            y: rect.top - origin.top,
            w: rect.width,
            h: rect.height,
          };
        }),
      );
    };
    measure();
    const timers = [80, 300, 800].map((ms) => setTimeout(measure, ms));
    const observer = typeof ResizeObserver === "undefined" ? null : new ResizeObserver(measure);
    observer?.observe(root);
    return () => {
      timers.forEach(clearTimeout);
      observer?.disconnect();
    };
  }, [stage, notes]);
  return boxes;
}
