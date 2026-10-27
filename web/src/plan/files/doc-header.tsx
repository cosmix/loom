import { LockIcon } from "lucide-react";
import type { JSX } from "react";
import { Link } from "react-router";

import type { DocView } from "@/plan/api/schema-files";
import { toAddress } from "@/plan/files/address";

/**
 * Title, path, plan state and the uncommitted-changes link. The link is the address
 * form of `openDiff("head", path)`; the diff view itself belongs to the editor stage.
 */
export function DocHeader({ doc }: { doc: DocView }): JSX.Element {
  return (
    <header className="flex flex-wrap items-baseline gap-x-3 gap-y-1 px-6 pt-5 pb-3">
      <h1 className="font-display text-2xl">{doc.title}</h1>
      {doc.plan && (
        <span className="text-[13px] font-medium text-muted-foreground">{doc.plan.state}</span>
      )}
      {doc.lock && (
        <LockIcon className="size-3.5 self-center text-muted-foreground" aria-label="Locked" />
      )}
      <code className="font-mono text-xs text-muted-foreground">{doc.path}</code>
      <Link
        to={`${toAddress(doc.path)}?diff=head`}
        className="ml-auto text-[13px] font-medium text-muted-foreground hover:text-foreground"
      >
        Uncommitted changes
      </Link>
    </header>
  );
}
