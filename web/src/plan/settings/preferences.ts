import { z } from "@/api/zod";
import { projectPref } from "@/plan/chat/prefs";

export type ReadingFace = "source-serif" | "literata" | "piazzolla";
export type ReadingSize = 17 | 19 | 21;

export const READING_FACES: readonly {
  id: ReadingFace;
  label: string;
  scripts: string;
  family: string;
}[] = [
  {
    id: "source-serif",
    label: "Source Serif 4",
    scripts: "Latin, Greek, Cyrillic",
    family: '"Source Serif 4 Variable", "Source Serif 4", Georgia, serif',
  },
  {
    id: "literata",
    label: "Literata",
    scripts: "Latin, Greek incl. polytonic, Cyrillic",
    family: '"Literata Variable", Literata, Georgia, serif',
  },
  {
    id: "piazzolla",
    label: "Piazzolla",
    scripts: "Latin, Greek incl. polytonic, Cyrillic",
    family: '"Piazzolla Variable", Piazzolla, Georgia, serif',
  },
];

export const READING_SIZES: readonly { size: ReadingSize; label: string }[] = [
  { size: 17, label: "Compact" },
  { size: 19, label: "Comfortable" },
  { size: 21, label: "Large" },
];

export const DEFAULT_READING_FACE: ReadingFace = "source-serif";
export const DEFAULT_READING_SIZE: ReadingSize = 19;

export const readingFaceAtom = projectPref<ReadingFace>(
  "reading-face",
  DEFAULT_READING_FACE,
  z.enum(["source-serif", "literata", "piazzolla"]),
);
export const readingSizeAtom = projectPref<ReadingSize>(
  "reading-size",
  DEFAULT_READING_SIZE,
  z.union([z.literal(17), z.literal(19), z.literal(21)]),
);
export const sourceMarksAtom = projectPref("source-marks", true, z.boolean());
export const autoRebuildAtom = projectPref("auto-rebuild-summaries", true, z.boolean());
