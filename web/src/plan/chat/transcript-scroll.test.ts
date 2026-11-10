import { expect, it } from "vitest";

import columnCss from "./chat-column.css?raw";
import composerCss from "./chat-composer.css?raw";
import surfaceCss from "./chat-surface.css?raw";
import transcriptCss from "./chat-transcript.css?raw";

/** The declarations of the first top-level rule whose selector is exactly `selector`. */
function ruleBody(css: string, selector: string): string {
  const escaped = selector.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");
  return new RegExp(`(?:^|\\n)${escaped} \\{([^}]*)\\}`).exec(css)?.[1] ?? "";
}

/** The text of the `prefers-reduced-motion: reduce` block, up to the next top-level rule. */
function reducedMotion(css: string): string {
  const start = css.indexOf("@media (prefers-reduced-motion: reduce)");
  return start < 0 ? "" : css.slice(start);
}

it("the transcript scroller is the containing block of the positioned text inside it", () => {
  // Without it the typing bubble's sr-only "Working" (position: absolute) is placed against
  // Transcript's relative root outside the scroller and the page, not the log, scrolls the chat.
  expect(ruleBody(transcriptCss, ".chat-transcript")).toMatch(/\n\s*position: relative;/);
});

it("the typing bubble mirrors the user's bubble on the left and rests under reduced motion", () => {
  const bubble = ruleBody(transcriptCss, ".chat-typing");
  expect(bubble).toContain("justify-self: start;");
  expect(bubble).toContain("background: var(--muted);");
  expect(bubble).toContain(
    "border-radius: var(--chat-card-corner) var(--chat-card-corner) var(--chat-card-corner) 4px;",
  );
  expect(ruleBody(transcriptCss, ".chat-typing-dot")).toContain(
    "animation: chat-typing 1.2s ease-in-out infinite;",
  );
  const reduced = reducedMotion(transcriptCss);
  expect(reduced).toMatch(/\.chat-typing \{\s*animation: none;/);
  expect(reduced).toMatch(/\.chat-typing-dot \{\s*opacity: 0\.6;\s*animation: none;/);
});

// What these two pin is measured in a browser by scripts/chat-layout-test.mjs; they are here so
// that `bun run test`, which lays nothing out, still fails when the column's rules are taken out.
it("the column's body takes what is left, and its top and composer cannot push it out", () => {
  const body = ruleBody(columnCss, ".chat-empty");
  expect(body).toContain("flex: 1 1 0;");
  expect(body).toContain("min-height: min(7rem, 30%);");
  expect(ruleBody(columnCss, ".chat-transcript-frame")).toContain("overflow: hidden;");
  expect(ruleBody(columnCss, ".chat-panel-top:not(:last-child)")).toContain("max-height: 50%;");
  const composer = ruleBody(columnCss, ".chat-composer");
  expect(composer).toContain("flex: 0 1 auto;");
  expect(composer).toContain("min-height: 0;");
  expect(composer).toContain("overflow: hidden auto;");
});

it("no part of the chat column is sized by the window", () => {
  for (const css of [columnCss, composerCss, surfaceCss, transcriptCss]) {
    expect(css).not.toMatch(/\d[sld]?v[hb]\b/);
  }
});
