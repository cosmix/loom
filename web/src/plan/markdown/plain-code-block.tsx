import { CopyIcon } from "lucide-react";
import { type JSX, useRef } from "react";

import { CopyButton, selectContents } from "@/plan/markdown/copy-button";

/**
 * A fenced or indented block as plain monospace text with its language label and a copy control.
 * `note` is a muted line under the source (the diagram renderer's fallback explanation).
 */
export function PlainCodeBlock({
  value,
  lang,
  note,
}: {
  value: string;
  lang: string | null;
  note?: string;
}): JSX.Element {
  const code = useRef<HTMLElement>(null);
  return (
    <div className="md-code">
      <div className="md-code-bar">
        {lang ? <span className="md-code-lang">{lang}</span> : <span />}
        <CopyButton
          text={value}
          label={lang ? `Copy ${lang} code` : "Copy code"}
          icon={CopyIcon}
          onFailed={() => selectContents(code.current)}
        />
      </div>
      {/* Focusable so a keyboard can scroll a long line sideways. */}
      <pre tabIndex={0}>
        <code ref={code}>{value}</code>
      </pre>
      {note ? <p className="md-code-note">{note}</p> : null}
    </div>
  );
}
