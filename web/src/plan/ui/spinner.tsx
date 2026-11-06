import type { CSSProperties, JSX, SVGProps } from "react";

import "./spinner.css";

/**
 * The block-sized loading spinner (`ui/DESIGN.md`, Status, rule 4): a 20px hairline ring with one
 * turning arc, in `currentColor`, for a state block or a working card. It is decorative
 * (`aria-hidden`), so the words beside it are the announcement and the caller keeps its own role.
 */
export function Spinner(): JSX.Element {
  return (
    <svg className="plan-spinner" viewBox="0 0 16 16" aria-hidden="true" focusable="false">
      <circle className="plan-spinner__track" cx="8" cy="8" r="6.25" />
      <circle className="plan-spinner__arc" cx="8" cy="8" r="6.25" pathLength="100" />
    </svg>
  );
}

export interface ArrowSpinnerProps extends Omit<
  SVGProps<SVGSVGElement>,
  "aria-hidden" | "ref" | "size"
> {
  /** Whether the arrow turns. A resting arrow sits still and dims. */
  busy?: boolean;
  /** Box size as a CSS length; a number is px. Defaults to the 14px glyph box. */
  size?: number | string;
  /** Accepted so a glyph map can pass it; the root always sets `aria-hidden="true"` after it. */
  "aria-hidden"?: boolean | "true" | "false";
}

/**
 * Work in progress at glyph size (`ui/DESIGN.md`, Status): a lucide-weight circular arrow that
 * turns, in `currentColor`. It takes the props a glyph site passes a lucide icon (`className`,
 * `size`, data attributes), so the glyph maps in `ui/glyphs.tsx` hold it beside the icons. It is
 * always `aria-hidden`: the words beside it are the announcement. Static under reduced motion.
 */
export function ArrowSpinner({
  busy = true,
  size,
  className,
  style,
  ...rest
}: ArrowSpinnerProps): JSX.Element {
  const sized: CSSProperties | undefined =
    size === undefined ? style : { ...style, width: size, height: size };
  return (
    <svg
      {...rest}
      className={`plan-arrow-spinner${className ? ` ${className}` : ""}`}
      data-busy={busy ? "true" : "false"}
      style={sized}
      viewBox="0 0 24 24"
      fill="none"
      stroke="currentColor"
      strokeWidth={2}
      strokeLinecap="round"
      strokeLinejoin="round"
      aria-hidden="true"
      focusable="false"
    >
      <path d="M21 12a9 9 0 1 1-9-9c2.52 0 4.93 1 6.74 2.74L21 8" />
      <path d="M21 3v5h-5" />
    </svg>
  );
}
