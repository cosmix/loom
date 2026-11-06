import { darkVariantAtom } from "@/aurora-ui/shared/atoms/theme";
import type { Store } from "@/state/store";

const DARK_VARIANT_KEY = "loom:dark-variant";

/**
 * The planner's dark theme is Graphite until the reader picks one. The shared atom defaults to
 * Walnut for the dashboard; the planner is served on its own origin, so its storage is its own,
 * and an empty key means no theme was ever picked here. Light or dark still follows the desktop.
 */
export function seedPlannerDarkTheme(store: Store, storage: Storage | undefined): void {
  try {
    if (storage && storage.getItem(DARK_VARIANT_KEY) === null) store.set(darkVariantAtom, "gray");
  } catch {
    // Storage blocked (a private window, cleared site data): the shared default stands.
  }
}
