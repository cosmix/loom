import type { ApiResult } from "@/plan/api/client";
import type { SaveResult } from "@/plan/api/schema-files";

type Validation = Extract<SaveResult, { result: "invalid" }>["validation"];
export type SaveStatus = "idle" | "saving" | "saved" | "conflict" | "invalid" | "error";

export type SaveFlowState = {
  status: SaveStatus;
  openedHash: string;
  text: string;
  savedText: string;
  validation: Validation | null;
  currentText: string | null;
  error: string | null;
  submittedText: string | null;
  clearDrafts: boolean;
};

export type SaveFlowEvent =
  | { type: "change"; text: string }
  | { type: "save" }
  | { type: "result"; result: ApiResult<SaveResult> }
  | { type: "merge"; currentText: string; currentHash: string; text: string }
  | { type: "discard" }
  | { type: "draftsCleared" };

export function initialSaveFlow(text: string, openedHash: string): SaveFlowState {
  return {
    status: "idle",
    openedHash,
    text,
    savedText: text,
    validation: null,
    currentText: null,
    error: null,
    submittedText: null,
    clearDrafts: false,
  };
}

/** Pure state transitions; the caller performs saves and clears drafts when requested. */
export function saveFlow(state: SaveFlowState, event: SaveFlowEvent): SaveFlowState {
  switch (event.type) {
    case "change":
      return {
        ...state,
        text: event.text,
        clearDrafts: false,
        // An invalid verdict covers the text that was submitted; "Save with errors" must not
        // force-save text typed since, so editing sends the user back through a validated save.
        status: state.status === "saved" || state.status === "invalid" ? "idle" : state.status,
      };
    case "save":
      if (state.status === "saving") return state;
      return { ...state, status: "saving", submittedText: state.text, error: null };
    case "result":
      return state.status === "saving" ? saveResult(state, event.result) : state;
    case "merge":
      return {
        ...state,
        status: "conflict",
        openedHash: event.currentHash,
        currentText: event.currentText,
        savedText: event.currentText,
        text: event.text,
        error: null,
      };
    case "discard":
      return {
        ...state,
        status: "idle",
        text: state.savedText,
        currentText: null,
        validation: null,
        error: null,
        submittedText: null,
        clearDrafts: true,
      };
    case "draftsCleared":
      return { ...state, clearDrafts: false };
  }
}

function saveResult(state: SaveFlowState, result: ApiResult<SaveResult>): SaveFlowState {
  if (!result.ok) {
    return { ...state, status: "error", error: result.message, submittedText: null };
  }
  const value = result.value;
  switch (value.result) {
    case "saved": {
      const savedText = state.submittedText ?? state.text;
      return {
        ...state,
        status: "saved",
        openedHash: value.hash,
        savedText,
        validation: value.validation,
        currentText: null,
        submittedText: null,
        clearDrafts: state.text === savedText,
      };
    }
    case "conflict":
      return {
        ...state,
        status: "conflict",
        openedHash: value.current_hash,
        currentText: value.current_text,
        savedText: value.current_text,
        validation: null,
        submittedText: null,
      };
    case "invalid":
      return { ...state, status: "invalid", validation: value.validation, submittedText: null };
  }
}
