// Mirrors MAX_FEATURE_NAME in loom/src/planning/kinds.rs.
export const MAX_FEATURE_NAME = 64;

export const FEATURE_NAME_HINT: string =
  "Use lowercase ASCII letters, digits, and hyphens, starting with a letter or digit; " +
  "archive, briefs, plans, and overview are reserved; use at most 64 characters.";

export function isFeatureName(name: string): boolean {
  return (
    name.length <= MAX_FEATURE_NAME &&
    /^[a-z0-9]/.test(name) &&
    !/[^a-z0-9-]/.test(name) &&
    !["archive", "briefs", "plans", "overview"].includes(name)
  );
}
