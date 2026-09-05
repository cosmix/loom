import { isRouteErrorResponse, Link, useRouteError } from "react-router";

import { HazardPanel } from "@/aurora-ui/feedback/HazardPanel";
import { Logo } from "@/components/logo";

/// The router's `errorElement`: an intentional HTTP response first, then a
/// thrown `Error`, then whatever else.
export function RouteError() {
  const error = useRouteError();
  const { title, detail } = explain(error);
  return (
    <main className="mx-auto flex min-h-dvh max-w-xl flex-col items-start justify-center gap-4 px-6">
      <Logo className="h-10 w-auto text-muted-foreground" />
      <HazardPanel tone="error" title={title} className="w-full bg-card">
        {detail && (
          <pre className="max-w-full overflow-auto text-xs whitespace-pre-wrap">{detail}</pre>
        )}
        <Link to="/" className="mt-2 inline-block text-sm underline underline-offset-4">
          back to the overview
        </Link>
      </HazardPanel>
    </main>
  );
}

function explain(error: unknown): { title: string; detail: string | null } {
  if (isRouteErrorResponse(error)) {
    return { title: `${error.status} ${error.statusText}`.trim(), detail: null };
  }
  if (error instanceof Error) {
    return { title: "The dashboard hit an error", detail: error.message };
  }
  return { title: "The dashboard hit an unknown error", detail: null };
}
