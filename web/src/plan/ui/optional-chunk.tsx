import { Suspense, type ReactNode } from "react";

import { ErrorBoundary } from "@/aurora-ui/feedback/ErrorBoundary";

/**
 * Mounts a lazy component that the screen can do without (a dialog fetched after first paint).
 * Nothing shows while its chunk loads, and nothing shows if the chunk fails: the shell's own
 * boundary would otherwise replace the whole planner with an error card for a missing dialog.
 */
export function OptionalChunk({ children }: { children: ReactNode }) {
  return (
    <ErrorBoundary fallback={null}>
      <Suspense fallback={null}>{children}</Suspense>
    </ErrorBoundary>
  );
}
