import type { ComponentProps, CSSProperties, JSX } from "react";

import { BusyRoundel } from "@/aurora-ui/feedback/BusyRoundel";
import { cn } from "cn";

import "./roundel-glyph.css";

export interface RoundelGlyphProps extends Omit<ComponentProps<"span">, "aria-hidden"> {
  /** Whether the disc turns. A resting disc sits upright and dims (busy-roundel.css). */
  busy?: boolean;
  /** Box size as a CSS length; a number is px. Defaults to the glyph box the stylesheet sets. */
  size?: number | string;
}

/**
 * The roundel as a decorative glyph (`ui/DESIGN.md`, Status): the one sign of work in progress,
 * drawn wherever a state glyph stands. The root is `aria-hidden` and the disc carries no live
 * region of its own, so the words beside it are the only announcement and the caller keeps its
 * own role. It takes the props a glyph site passes a lucide icon (`className`, `size`, data
 * attributes), so the glyph maps in `ui/glyphs.tsx` can hold it beside the icons.
 */
export function RoundelGlyph({
  busy = true,
  size,
  className,
  style,
  ...rest
}: RoundelGlyphProps): JSX.Element {
  const sized: CSSProperties | undefined =
    size === undefined ? style : { ...style, width: size, height: size };
  return (
    <span
      {...rest}
      className={cn("plan-roundel-glyph", className)}
      style={sized}
      aria-hidden="true"
    >
      <BusyRoundel busy={busy} size="100%" busyLabel="" idleLabel="" />
    </span>
  );
}
