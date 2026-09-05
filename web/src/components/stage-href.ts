/// The detail route for a stage id, encoded once for every link that needs it.
export function stageHref(id: string): string {
  return `/stages/${encodeURIComponent(id)}`;
}
