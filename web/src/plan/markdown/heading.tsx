import { LinkIcon } from "lucide-react";
import type { JSX, ReactNode } from "react";

import { toAddress } from "@/plan/files/address";
import { CopyButton } from "@/plan/markdown/copy-button";

const TAGS = ["h1", "h2", "h3", "h4", "h5", "h6"] as const;

/**
 * A heading carries the server's anchor as its id. The copy control sits
 * beside the heading, outside it, so the heading's accessible name stays its
 * own text.
 */
export function Heading({
  depth,
  anchor,
  path,
  children,
}: {
  depth: number;
  /** Null when the heading has no known anchor or ids are off for this render. */
  anchor: string | null;
  path: string;
  children: ReactNode;
}): JSX.Element {
  const Tag = TAGS[Math.min(Math.max(depth, 1), 6) - 1];
  if (anchor === null) return <Tag>{children}</Tag>;
  return (
    <div className="md-heading" data-depth={depth}>
      <Tag id={anchor}>{children}</Tag>
      <CopyButton
        text={window.location.origin + toAddress(path, anchor)}
        label="Copy link to this heading"
        icon={LinkIcon}
      />
    </div>
  );
}
