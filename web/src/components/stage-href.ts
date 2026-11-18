/// The detail route for a stage id, encoded once for every link that needs it.
export function stageHref(id: string): string {
  return `/stages/${encodeURIComponent(id)}`;
}

/// The query parameter naming the stage the modal shows, so an open modal
/// has a URL and the back button closes it.
export const STAGE_PARAM = "stage";

/// The URL hash that asks a stage report to bring its history card into view.
export const HISTORY_HASH = "#history";
