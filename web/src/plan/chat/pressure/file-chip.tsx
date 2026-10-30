import { FileDiff } from "lucide-react";
import type { JSX } from "react";
import { Link } from "react-router";

import { Untrusted, visibleControls } from "../visible-controls";

/**
 * A file the run edited, drawn as the transcript's changed-file chip: the basename is the link,
 * the whole path is the tooltip, and the Diff link (when the caller has one) follows it.
 */
export function FileChip({
  path,
  to,
  diffTo,
}: {
  path: string;
  to: string;
  diffTo: string | null;
}): JSX.Element {
  return (
    <span className="chat-file-chip max-w-full justify-self-start" title={visibleControls(path)}>
      <FileDiff size={12} aria-hidden="true" className="shrink-0" />
      <Link className="min-w-0 underline" to={to}>
        <Untrusted text={path.split("/").pop() ?? path} />
      </Link>
      {diffTo !== null && (
        <Link className="shrink-0 underline" to={diffTo}>
          Diff
        </Link>
      )}
    </span>
  );
}
