import { RefreshCw } from "lucide-react";
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
 * Work in progress at glyph size (`ui/DESIGN.md`, Status): lucide's two-arrow `RefreshCw` that
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
    <RefreshCw
      {...rest}
      className={`plan-arrow-spinner${className ? ` ${className}` : ""}`}
      data-busy={busy ? "true" : "false"}
      style={sized}
      aria-hidden="true"
      focusable="false"
    />
  );
}
