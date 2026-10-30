import type { JSX } from "react";

import "./doodle.css";

const TITLE = "A thread leaves a spool, loops through a thought and weaves into cloth on a loom";

// The spool: a flange ellipse, two cheeks, a rounded foot and two turns of wound thread.
const SPOOL =
  "M18 72V102M44 72V102M18 102C18 108 44 108 44 102M18 82Q31 88 44 82M18 92Q31 98 44 92";

// The frame is a rect (below); six warp lines run top to bottom inside it.
const WARPS = [163, 175, 188, 200, 213, 225] as const;

// One weft: out of the spool, a single loop around the thought at (100, 40), into the frame and
// across the warps in three passes (left to right, right to left, left to right), ending at the
// dot at (204, 92).
const WEFT =
  "M44 88C60 88 74 72 98 55C112 45 120 32 108 27C94 22 82 36 90 48C98 60 124 66 150 74" +
  "Q156 68 163 68Q169 62 175 68T188 68T200 68T213 68T225 68L232 68" +
  "Q238 74 232 80Q226 86 219 80T206 80T194 80T181 80T169 80L156 80" +
  "Q150 86 158 92L163 92Q169 86 175 92T188 92T200 92L204 92";

const SPARKS = ["M62 32V42M57 37H67", "M130 22V32M125 27H135", "M80 12V20M76 16H84"] as const;

export function LoomDoodle({ className }: { className?: string }): JSX.Element {
  return (
    <svg
      className={className ? `loom-doodle ${className}` : "loom-doodle"}
      viewBox="0 0 270 128"
      role="img"
      aria-label={TITLE}
      fill="none"
      strokeLinecap="round"
      strokeLinejoin="round"
    >
      <title>{TITLE}</title>
      <ellipse className="loom-doodle__ink" cx="31" cy="72" rx="13" ry="4" />
      <path className="loom-doodle__ink" d={SPOOL} />
      <rect className="loom-doodle__ink" x="150" y="52" width="88" height="60" rx="2" />
      {WARPS.map((x) => (
        <path key={x} className="loom-doodle__ink" d={`M${x} 52V112`} />
      ))}
      <path className="loom-doodle__thread" d={WEFT} pathLength="1" />
      <circle className="loom-doodle__dot" cx="204" cy="92" r="2.6" />
      {SPARKS.map((d) => (
        <path key={d} className="loom-doodle__spark" d={d} />
      ))}
    </svg>
  );
}
