import { LockIcon } from "lucide-react";
import type { JSX } from "react";
import { useAtomValue } from "jotai/react";
import { Link, useSearchParams } from "react-router";

import type { DocView } from "@/plan/api/schema-files";
import { openDiff } from "@/plan/diff/open-diff";
import { toAddress } from "@/plan/files/address";
import { canAuthorAtom } from "@/plan/state/atoms";

/**
 * Title, path, plan state, the Edit link and the uncommitted-changes link
 * (`openDiff("head", path)`).
 */
export function DocHeader({ doc }: { doc: DocView }): JSX.Element {
  const canAuthor = useAtomValue(canAuthorAtom);
  const [params] = useSearchParams();
  const showEdit = doc.editable && canAuthor && params.get("edit") !== "1";
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
      {showEdit && (
        <Link
          to={`${toAddress(doc.path)}?edit=1`}
          className="ml-auto text-[13px] font-medium text-foreground underline"
        >
          Edit
        </Link>
      )}
      <Link
        to={openDiff("head", doc.path)}
        className={`${showEdit ? "" : "ml-auto "} text-[13px] font-medium text-muted-foreground hover:text-foreground`}
      >
        Uncommitted changes
      </Link>
    </header>
  );
}
