import { LockIcon } from "lucide-react";
import type { JSX } from "react";

import type { LockView } from "@/plan/api/schema-files";

/** The server's lock notice, verbatim. */
export function LockNotice({ lock }: { lock: LockView }): JSX.Element {
  return (
    <p
      role="note"
      className="flex items-center gap-2 border-b border-hairline px-6 py-2 text-[13px] text-muted-foreground"
    >
      <LockIcon className="size-3.5 shrink-0" aria-hidden="true" />
      <span>{lock.notice}</span>
    </p>
  );
}
