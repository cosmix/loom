import {
  Archive,
  BookOpen,
  ChevronsUpDown,
  CircleCheck,
  CircleCheckBig,
  CircleDashed,
  CircleDot,
  CircleHelp,
  FileSearch,
  FileText,
  FlaskConical,
  FolderTree,
  House,
  Library,
  ListChecks,
  Lock,
  Map as MapIcon,
  MessageCircleQuestion,
  Minus,
  PencilLine,
  SlidersHorizontal,
  StickyNote,
  Telescope,
} from "lucide-react";
import type { LucideIcon } from "lucide-react";
import type { ReactNode } from "react";

import type { MarkView } from "@/plan/api/schema";
import { LiveDot } from "@/plan/ui/activity-glyphs";
import { ArrowSpinner } from "@/plan/ui/spinner";

/** The props a glyph site passes: a lucide icon takes them, and so do the arrow spinner and the live dot. */
export interface GlyphProps {
  className?: string;
  size?: number | string;
  "aria-hidden"?: boolean | "true" | "false";
}

/** A state glyph: a lucide icon, the live dot, or the arrow spinner. */
export type Glyph = (props: GlyphProps) => ReactNode;

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
export type ViewId = "home" | "files";

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

// A running plan is a state shown, so the live status dot; a rebuilding summary is an operation
// that ends, so the arrow spinner (ui/DESIGN.md, Status). Only doc kinds must stay distinct.
export const PLAN_STATE_GLYPHS: Record<PlanState, Glyph> = {
  written: PencilLine,
  running: LiveDot,
  done: CircleCheckBig,
  retired: Archive,
};

export const SUMMARY_STATE_GLYPHS: Record<SummaryState, Glyph> = {
  current: CircleCheck,
  rebuilding: ArrowSpinner,
  stale: CircleDashed,
};

// A spine row's mark. A gap is a dashed circle, the same shape a missing document and an
// out-of-date summary take: warning glyphs are for warnings (ui/DESIGN.md, Status).
export const STEP_MARK_GLYPHS: Record<MarkView, LucideIcon> = {
  done: CircleCheck,
  active: CircleDot,
  gap: CircleDashed,
  optional: Minus,
};

export const VIEW_GLYPHS: Record<ViewId, LucideIcon> = {
  home: House,
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
