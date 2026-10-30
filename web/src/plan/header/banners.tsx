import { useAtomValue } from "jotai/react";
import { TriangleAlertIcon } from "lucide-react";
import type { ReactNode } from "react";

import { serverInfoAtom, stoppingAtom } from "@/plan/state/atoms";

function Banner({ children }: { children: ReactNode }) {
  return (
    // One line at desktop, wrapping on a phone: the glyph carries the tone, the words stay in
    // --foreground (ui/DESIGN.md, Do not).
    <div
      role="status"
      className="flex items-start gap-2 border-b border-hairline px-(--page-gutter) py-2 text-[13px] leading-[1.35] font-medium text-foreground"
    >
      <TriangleAlertIcon
        className="mt-px size-3.5 shrink-0 text-(--tone-warning)"
        aria-hidden="true"
      />
      <p className="m-0 min-w-0 [overflow-wrap:anywhere]">{children}</p>
    </div>
  );
}

/** Server messages are not uniform: some end in a full stop, and the banner adds its own. */
function withoutTrailingStop(message: string): string {
  return message.replace(/[.!?\s]+$/, "");
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
          loom plan is read-only.{" "}
          {server.read_only.map((problem) => (
            <span key={problem.what}>
              {withoutTrailingStop(problem.message)}. {withoutTrailingStop(problem.fix)}.{" "}
            </span>
          ))}
        </Banner>
      )}
      {stopping && <Banner>loom plan is stopping.</Banner>}
    </>
  );
}
