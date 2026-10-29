export const FEATURE_NAME_HINT: string =
  "Use lowercase ASCII letters, digits, and hyphens, starting with a letter or digit; " +
  "archive, briefs, plans, and overview are reserved.";

export function isFeatureName(name: string): boolean {
  return (
    /^[a-z0-9]/.test(name) &&
    !/[^a-z0-9-]/.test(name) &&
    !["archive", "briefs", "plans", "overview"].includes(name)
  );
}
