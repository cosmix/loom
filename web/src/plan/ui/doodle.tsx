import type { JSX } from "react";

import "./doodle.css";

const TITLE = "A thread leaves a spool, loops through a thought and weaves into cloth on a loom";

const THREAD_PATHS = [
  "M24 75Q38 82 52 75M24 84Q38 91 52 84M24 93Q38 100 52 93",
  "M160 52V116",
  "M170 52V116",
  "M180 52V116",
  "M190 52V116",
  "M200 52V116",
  "M52 84C68 96 84 92 89 74C92 64 101 64 104 57" +
    "C91 61 87 48 94 41C100 34 109 37 113 41C124 35 134 43 130 53" +
    "C127 62 115 65 104 57C112 79 137 88 158 82" +
    "Q164 79 170 82T194 82Q208 78 204 89" +
    "Q198 92 192 89T168 89Q154 85 158 96" +
    "Q164 93 170 96T194 96Q208 92 204 103" +
    "Q198 106 192 103T168 103Q154 99 158 110" +
    "Q164 107 170 110T194 110L204 110",
] as const;

export function LoomDoodle({ className }: { className?: string }): JSX.Element {
  return (
    <svg
      className={className ? `loom-doodle ${className}` : "loom-doodle"}
      viewBox="0 0 240 140"
      role="img"
      aria-label={TITLE}
      fill="none"
      stroke="currentColor"
      strokeWidth={1.7}
      strokeLinecap="round"
      strokeLinejoin="miter"
    >
      <title>{TITLE}</title>
      <ellipse cx="38" cy="64" rx="18" ry="6" />
      <path d="M22 66V106M54 66V106M20 106C20 114 56 114 56 106" />
      <path d="M150 122V44H214V122M146 52H218M146 116H218" />
      {THREAD_PATHS.map((d) => (
        <path key={d} className="loom-doodle__thread" d={d} pathLength="1" />
      ))}
      <path className="loom-doodle__spark" d="M72 43V53M67 48H77" />
      <path className="loom-doodle__spark" d="M139 22V30M135 26H143" />
      <path className="loom-doodle__spark" d="M225 68V78M220 73H230" />
    </svg>
  );
}
