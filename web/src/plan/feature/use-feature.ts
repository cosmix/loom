import { useAtomValue } from "jotai/react";
import { useParams } from "react-router";

import type { ChecklistView } from "@/plan/api/schema";
import { featuresAtom, projectAtom } from "@/plan/state/atoms";

function decodeFeature(feature: string): string {
  try {
    return decodeURIComponent(feature);
  } catch {
    return feature;
  }
}

/** The path of one of a feature's pages; `page` is "vision", "product/section_01" and so on. */
export function featureHref(feature: string, page: string): string {
  return `/f/${encodeURIComponent(feature)}/${page}`;
}

/**
 * The feature the `:feature` route param names. `name` is the decoded param, for messages about a
 * feature that does not exist. `loaded` is false until the project arrives, so a missing checklist
 * means "still loading" then and "no such feature" afterwards.
 */
export function useFeature(): {
  name: string;
  checklist: ChecklistView | undefined;
  loaded: boolean;
} {
  const { feature } = useParams();
  const features = useAtomValue(featuresAtom);
  const loaded = useAtomValue(projectAtom) !== null;
  const name = decodeFeature(feature ?? "");
  // Router params are normally decoded already; preserve names containing literal escapes.
  const checklist =
    features.find((candidate) => candidate.feature === feature) ??
    features.find((candidate) => candidate.feature === name);
  return { name, checklist, loaded };
}
