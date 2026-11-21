import { describe, expect, it } from "vitest";

type Kind = "history" | "last_screen";
type Labels = Record<
  "button" | "aria" | "screenAria" | "eyebrow" | "footer" | "link" | "missing" | "failed" | "title",
  string
>;
type LabelsModule = { historyLabels(kind: Kind): Labels };

async function loadLabels(): Promise<LabelsModule> {
  // A variable specifier keeps vite from resolving the module at collection, so a missing
  // history-labels.ts fails inside the contract instead of before it runs.
  const specifier = "./history-labels";
  return (await import(/* @vite-ignore */ specifier)) as LabelsModule;
}

/// Today's literals, from the stage actions, the modal, the saved-screen view
/// and blocked attention.
const HISTORY: Labels = {
  button: "Terminal history",
  aria: "open terminal history",
  screenAria: "Saved terminal history",
  eyebrow: "Terminal history",
  footer: "read-only · the session ended · this is its saved screen",
  link: "View terminal history",
  missing: "This session left no saved terminal history.",
  failed: "The saved terminal history could not be loaded.",
  title: "The session ended; its saved screen opens read-only",
};

const LAST_SCREEN: Labels = {
  button: "Last screen",
  aria: "open last screen",
  screenAria: "Saved last screen",
  eyebrow: "Last screen",
  footer: "read-only · the session ended · this is its last screen",
  link: "View last screen",
  missing: "This session left no saved screen.",
  failed: "The saved screen could not be loaded.",
  title: "The session ended; its last screen opens read-only",
};

describe("history labels", () => {
  it("a last screen never mentions history", async () => {
    const { historyLabels } = await loadLabels();
    const history = historyLabels("history");
    const lastScreen = historyLabels("last_screen");

    expect(history).toMatchObject(HISTORY);
    expect(lastScreen).toMatchObject(LAST_SCREEN);
    for (const [key, value] of Object.entries(lastScreen)) {
      expect(String(value), key).not.toMatch(/history/i);
    }
    for (const labels of [history, lastScreen]) {
      expect(new Set([labels.button, labels.aria, labels.screenAria]).size).toBe(3);
    }
  });
});
