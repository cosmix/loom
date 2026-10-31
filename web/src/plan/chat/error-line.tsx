import type { JSX, ReactNode } from "react";

import { Untrusted } from "./visible-controls";
import "./chat-surface.css";

/**
 * A failed load inside the transcript: one line in the panel's notice style, a toned glyph and
 * the server's words in the normal colour. A transcript item is not a surface, so it draws no
 * box (DESIGN.md, Surfaces and borders). `role` stays with the caller, which knows whether the
 * line is announced.
 */
export function ErrorLine({
  text,
  role,
  children,
}: {
  text: string;
  role?: "status" | "alert";
  children?: ReactNode;
}): JSX.Element {
  return (
    <div className="chat-notice" data-tone="error" role={role}>
      <p>
        <span aria-hidden="true" className="chat-notice-glyph">
          ⚠
        </span>{" "}
        <Untrusted text={text} />
      </p>
      {children}
    </div>
  );
}
