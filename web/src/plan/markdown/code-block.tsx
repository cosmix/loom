import type { JSX } from "react";

import { MermaidBlock } from "@/plan/markdown/mermaid-block";
import { PlainCodeBlock } from "@/plan/markdown/plain-code-block";

/**
 * A fenced or indented block. A `mermaid` fence is drawn as a diagram (mermaid is loaded on first
 * use); every other language is plain monospace text with no highlighting.
 */
export function CodeBlock({ value, lang }: { value: string; lang: string | null }): JSX.Element {
  if (lang === "mermaid") return <MermaidBlock source={value} />;
  return <PlainCodeBlock value={value} lang={lang} />;
}
