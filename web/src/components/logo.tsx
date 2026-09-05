/*
 * The terminal logo (`crate::LOGO`) redrawn as strokes:
 *
 *    ╷
 *    │  ┌─┐┌─┐┌┬┐
 *    │  │ ││ ││││
 *    ┴─┘└─┘└─┘┴ ┴
 *
 * Twelve character cells wide once the three-space indent is dropped, four
 * rows tall; a cell is 1 unit wide and 2 units tall, so every stroke sits on
 * a cell centre (x + 0.5, 2y + 1) exactly where the box-drawing glyph puts it.
 * Round caps mark free ends; a cap that lands on a crossing stroke is hidden
 * inside it, so corners and T-junctions stay square.
 */
const LOGO_PATHS = [
  // l: the half-stem of ╷ through two │ into ┴, then the foot and the ┘ hook.
  "M0.5 1V7",
  "M0 7H2.5V6",
  // o o: each ┌─┐ / │ │ / └─┘ box is one closed rectangle.
  "M3.5 3H5.5V7H3.5Z",
  "M6.5 3H8.5V7H6.5Z",
  // m: ┌┬┐ over │││ over ┴ ┴ — two outer legs with feet, a short middle leg.
  "M9.5 7V3H11.5V7",
  "M10.5 3V6",
  "M9 7H10",
  "M11 7H12",
] as const;

const LOGO_VIEWBOX = "-0.5 0.4 13 7.2";

export function Logo({ className, title = "loom" }: { className?: string; title?: string }) {
  return (
    <svg
      role="img"
      aria-label="loom"
      viewBox={LOGO_VIEWBOX}
      className={className}
      fill="none"
      stroke="currentColor"
      strokeWidth={0.42}
      strokeLinecap="round"
      strokeLinejoin="miter"
    >
      <title>{title}</title>
      {LOGO_PATHS.map((d) => (
        <path key={d} d={d} />
      ))}
    </svg>
  );
}
