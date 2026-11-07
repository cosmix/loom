import { type JSX, useEffect, useId, useState } from "react";

import { mermaidThemeVariables, useThemeSignature } from "@/plan/markdown/mermaid-theme";
import { PlainCodeBlock } from "@/plan/markdown/plain-code-block";

type Drawn = { svg: string } | { error: true };

let drawCount = 0;

/** Draws one diagram; the result is `null` until mermaid loads and renders. */
async function draw(source: string, reactId: string): Promise<Drawn> {
  const id = `mmd-${reactId.replace(/[^a-zA-Z0-9]/g, "")}-${drawCount++}`;
  try {
    // Dynamic import keeps mermaid out of the entry bundle; it loads with the first diagram.
    const { default: mermaid } = await import("mermaid");
    mermaid.initialize({
      startOnLoad: false,
      securityLevel: "strict",
      theme: "base",
      themeVariables: mermaidThemeVariables(),
    });
    const { svg } = await mermaid.render(id, source);
    return { svg };
  } catch {
    // A failed parse leaves its error graphic in the body; remove it.
    document.getElementById(`d${id}`)?.remove();
    return { error: true };
  }
}

/**
 * A `mermaid` fence drawn as a diagram. The source shows as a code block while mermaid loads and
 * when the diagram does not parse (with a muted note). Redrawn when the planner theme changes.
 */
export function MermaidBlock({ source }: { source: string }): JSX.Element {
  const reactId = useId();
  const theme = useThemeSignature();
  const [drawn, setDrawn] = useState<(Drawn & { source: string; theme: string }) | null>(null);

  useEffect(() => {
    let live = true;
    void draw(source, reactId).then((result) => {
      if (live) setDrawn({ ...result, source, theme });
    });
    return () => {
      live = false;
    };
  }, [source, theme, reactId]);

  // A stale result (different source) is not shown; a theme-only change keeps the old drawing
  // until the new one is ready, so the block does not flash to source.
  if (!drawn || drawn.source !== source) {
    return <PlainCodeBlock value={source} lang="mermaid" />;
  }
  if ("error" in drawn) {
    return <PlainCodeBlock value={source} lang="mermaid" note="This diagram could not be drawn." />;
  }
  return (
    <figure
      className="md-diagram"
      aria-label="Mermaid diagram"
      // The svg is mermaid's own output under securityLevel "strict" (sanitised, no scripts).
      dangerouslySetInnerHTML={{ __html: drawn.svg }}
    />
  );
}
