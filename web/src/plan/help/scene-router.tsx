import { MemoryRouter, useInRouterContext, type MemoryRouterProps } from "react-router";

/** Standalone scenes need a router; in-app scenes reuse the surrounding router and stay inert. */
export function SceneRouter({
  children,
  initialEntries,
}: Pick<MemoryRouterProps, "children" | "initialEntries">) {
  const routed = useInRouterContext();
  return routed ? (
    children
  ) : (
    <MemoryRouter initialEntries={initialEntries}>{children}</MemoryRouter>
  );
}
