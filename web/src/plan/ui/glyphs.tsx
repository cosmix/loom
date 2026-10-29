import {
  Archive,
  BookOpen,
  ChevronsUpDown,
  CircleCheck,
  CircleCheckBig,
  CircleDashed,
  CircleDot,
  CircleHelp,
  Cog,
  FileSearch,
  FileText,
  FlaskConical,
  FolderTree,
  Gauge,
  House,
  Library,
  ListChecks,
  Lock,
  Map as MapIcon,
  MessageCircleQuestion,
  Minus,
  Package,
  PencilLine,
  Play,
  RefreshCw,
  ScanEye,
  SlidersHorizontal,
  StickyNote,
  Telescope,
  TriangleAlert,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";

import type { MarkView } from "@/plan/api/schema";

export type DocKind =
  | "vision"
  | "section"
  | "note"
  | "metaplan"
  | "plan"
  | "brief"
  | "knowledge"
  | "review";
export type PlanState = "written" | "running" | "done" | "retired";
export type SummaryState = "current" | "rebuilding" | "stale";
export type ViewId = "home" | "product" | "engineering" | "files";

// Step n uses index n - 1 in both arrays.
export const STEP_GLYPHS: readonly LucideIcon[] = [
  Telescope,
  BookOpen,
  MapIcon,
  ListChecks,
  Gauge,
  ScanEye,
  Play,
];

export const STEP_DOC_KIND: readonly DocKind[] = [
  "vision",
  "section",
  "metaplan",
  "plan",
  "plan",
  "review",
  "plan",
];

export const DOC_KIND_GLYPHS: Record<DocKind, LucideIcon> = {
  vision: Telescope,
  section: BookOpen,
  note: StickyNote,
  metaplan: MapIcon,
  plan: ListChecks,
  brief: FileText,
  knowledge: Library,
  review: FileSearch,
};

export const PLAN_STATE_GLYPHS: Record<PlanState, LucideIcon> = {
  written: PencilLine,
  running: Play,
  done: CircleCheckBig,
  retired: Archive,
};

export const SUMMARY_STATE_GLYPHS: Record<SummaryState, LucideIcon> = {
  current: CircleCheck,
  rebuilding: RefreshCw,
  stale: CircleDashed,
};

// Only done has a shared status glyph in the rail; other rail cells show the step's glyph.
export const STEP_MARK_GLYPHS: Record<MarkView, LucideIcon> = {
  done: CircleCheck,
  active: CircleDot,
  gap: TriangleAlert,
  optional: Minus,
};

export const VIEW_GLYPHS: Record<ViewId, LucideIcon> = {
  home: House,
  product: Package,
  engineering: Cog,
  files: FolderTree,
};

export const OTHER_GLYPHS: {
  readOnly: LucideIcon;
  pressureTested: LucideIcon;
  openQuestions: LucideIcon;
  stepDone: LucideIcon;
  switcher: LucideIcon;
  settings: LucideIcon;
  help: LucideIcon;
} = {
  readOnly: Lock,
  pressureTested: FlaskConical,
  openQuestions: MessageCircleQuestion,
  stepDone: STEP_MARK_GLYPHS.done,
  switcher: ChevronsUpDown,
  settings: SlidersHorizontal,
  help: CircleHelp,
};
