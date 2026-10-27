import { useAtomValue } from "jotai/react";
import { TriangleAlertIcon } from "lucide-react";
import type { ReactNode } from "react";

import { serverInfoAtom, stoppingAtom } from "@/plan/state/atoms";

function Banner({ children }: { children: ReactNode }) {
  return (
    <div
      role="status"
      className="flex items-start gap-2 border-b border-hairline px-4 py-2 text-[13px] font-medium text-(--tone-warning)"
    >
      <TriangleAlertIcon className="mt-0.5 size-3.5 shrink-0" aria-hidden="true" />
      <div className="min-w-0">{children}</div>
    </div>
  );
}

/** Remote bind, read-only mode (one banner listing each prerequisite) and stopping. */
export function Banners() {
  const server = useAtomValue(serverInfoAtom);
  const stopping = useAtomValue(stoppingAtom) || server?.stopping === true;
  return (
    <>
      {server?.remote && (
        <Banner>
          This server is plain HTTP. Anyone who can see the traffic can read the link and every
          document.
        </Banner>
      )}
      {server && server.read_only.length > 0 && (
        <Banner>
          <p>loom plan is read-only.</p>
          <ul>
            {server.read_only.map((problem) => (
              <li key={problem.what}>
                {problem.message}. {problem.fix}
              </li>
            ))}
          </ul>
        </Banner>
      )}
      {stopping && <Banner>loom plan is stopping.</Banner>}
    </>
  );
}
