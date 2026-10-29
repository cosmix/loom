import { createStore } from "jotai";
import { Provider } from "jotai/react";
import { useState } from "react";

import type { Attachment, Item, SessionView, Turn } from "@/plan/api/schema-chat";
import type { AttachmentDraft } from "@/plan/attachments";
import { ComposerArea } from "@/plan/chat/composer";
import { draftAtom } from "@/plan/chat/drafts";
import { snapshotAtom } from "@/plan/chat/model";
import { holdNoteSeenAtom } from "@/plan/chat/prefs";
import { StateLine } from "@/plan/chat/state-line";
import { Transcript } from "@/plan/chat/transcript";
import type { PlannerHelpScene } from "@/plan/help/pinned-scene";
import { SAMPLE_HOLDER, SAMPLE_SERVER } from "@/plan/help/sample";
import { SceneRouter } from "@/plan/help/scene-router";
import { authoringAtom, connectionAtom, youAtom } from "@/plan/state/atoms";

const STARTED_AT = "2026-10-29T10:00:00Z";
const READER = SAMPLE_SERVER.you;
const PASSAGE: AttachmentDraft = {
  kind: "source",
  path: "doc/vision.md",
  anchor: "Audience",
  lines: null,
  stageId: null,
  hash: null,
  text: "Atlas helps small teams plan their next release together.",
};
const SENT_PASSAGE: Attachment = {
  kind: "source",
  path: "doc/vision.md",
  anchor: "Audience",
  lines: null,
  stage: null,
  summary_sentence: null,
  rests_on: [],
  quote: PASSAGE.text,
  changed_since_selected: false,
  locked: null,
};

function sampleSession(state: SessionView["state"]): SessionView {
  return {
    id: "help-chat",
    kind: "planner",
    state,
    name: "Atlas planning",
    model: "Claude",
    socket: "help-sample",
    holder: SAMPLE_HOLDER.holder.name,
    away_until: null,
    queued: state === "working" ? 1 : 0,
    started_at: STARTED_AT,
    state_since: STARTED_AT,
    working_since: STARTED_AT,
    subagents_running: 0,
    last_activity: STARTED_AT,
    failure: null,
    offer: null,
  };
}

function sampleStore(state: SessionView["state"]) {
  const store = createStore();
  store.set(youAtom, READER);
  store.set(authoringAtom, SAMPLE_HOLDER);
  store.set(connectionAtom, { phase: "open", everOpen: true, lastMessageAt: null });
  // No server/project key: draft and preference atoms stay in this store's memory.
  store.set(holdNoteSeenAtom, true);
  store.set(snapshotAtom, {
    session: sampleSession(state),
    turns: [],
    queue: [],
    previous: null,
  });
  return store;
}

function sampleTool(id: string, name: string, summary: string): Extract<Item, { type: "tool" }> {
  return {
    type: "tool",
    id,
    tool_use_id: id,
    name,
    summary,
    status: "done",
    subagent: null,
    parent: null,
    input: JSON.stringify({ file_path: "doc/vision.md" }),
    result: "Done",
    result_truncated: false,
    error_line: null,
    added: null,
    removed: null,
    patch: null,
    nested_tools: 0,
  };
}

function sampleTurn(): Turn {
  return {
    id: "t1",
    started_at: STARTED_AT,
    duration_ms: null,
    // Open turns skip attachment/diff requests and show the real in-progress controls.
    open: true,
    tokens: { input: 1200, cache_write: 0, cache_read: 600, output: 240 },
    changed_files: [{ path: "doc/vision.md", before_hash: "help-before", by_tool: true }],
    summaries_rebuilding: [],
    items: [
      {
        type: "user",
        id: "help-message",
        text: "Clarify who Atlas is for using this passage.",
        delivery: "delivered",
        origin: null,
        attachments: [SENT_PASSAGE],
      },
      sampleTool("help-edit", "Edit", "Clarified the audience in vision.md"),
      {
        type: "permission",
        id: "help-permission",
        request_id: "help-request",
        tool: "Bash",
        input: JSON.stringify({ command: "rg -n Atlas doc/vision.md" }),
        status: "pending",
        deny_reason: null,
        note: null,
        expires_at: new Date(Date.now() + 3_600_000).toISOString(),
      },
    ],
  };
}

export function ChatComposerScene() {
  const [store] = useState(() => {
    const sample = sampleStore("working");
    sample.set(draftAtom, {
      text: "Make the audience description more specific.",
      attachments: [PASSAGE],
    });
    sample.set(snapshotAtom, (snapshot) => ({
      ...snapshot,
      queue: [
        {
          id: "help-queued",
          first_line: "Then check the release goals.",
          text: "Then check the release goals.",
        },
      ],
    }));
    return sample;
  });
  return (
    <Provider store={store}>
      <SceneRouter>
        <div data-loom-plan inert>
          <ComposerArea />
        </div>
      </SceneRouter>
    </Provider>
  );
}

export function ChatConversationScene() {
  const [store] = useState(() => {
    const sample = sampleStore("permission");
    sample.set(snapshotAtom, (snapshot) => ({ ...snapshot, turns: [sampleTurn()] }));
    return sample;
  });
  return (
    <Provider store={store}>
      <SceneRouter>
        <div data-loom-plan inert className="space-y-3">
          <StateLine />
          <Transcript />
        </div>
      </SceneRouter>
    </Provider>
  );
}

export const CHAT_SCENES: readonly PlannerHelpScene[] = [
  {
    title: "The composer",
    Scene: ChatComposerScene,
    notes: [
      {
        target: 'textarea[aria-label="Message"]',
        text: "Message holds the text you want to send with the attached passage.",
      },
      {
        target: '[aria-label="Pending attachments"] button:not([aria-label^="Remove"])',
        text: "The passage chip opens the document at the attached passage.",
      },
      {
        target: '[aria-label="Pending attachments"] button[aria-label^="Remove"]',
        text: "The remove button detaches the passage from your draft.",
      },
      {
        target: 'textarea[aria-label="Message"] + div > button',
        text: "Send submits your text and attached passages to the session.",
      },
      {
        target: 'div:has(> textarea[aria-label="Message"]) + div button',
        text: "Stop interrupts the current turn.",
      },
      {
        target: '[aria-label="Queued messages"]',
        text: "Remove withdraws a queued message, while Edit returns its text to the draft.",
      },
    ],
  },
  {
    title: "A conversation",
    Scene: ChatConversationScene,
    notes: [
      {
        target: '[aria-label="Chat transcript"] button[aria-expanded]',
        text: "The tool row expands to show the tool input and result.",
      },
      {
        target: '[aria-label="Sent attachments"] button',
        text: "The sent passage chip opens the source document.",
      },
      {
        target: '[aria-label="Turn 1 changes"]',
        text: "The changes list shows the files this turn touched, and Diff becomes available when the turn ends.",
      },
      {
        target: '[aria-label="Permission: Bash"] button:first-child',
        text: "Allow approves the requested tool action.",
      },
      {
        target: '[aria-label="Permission: Bash"] button:last-child',
        text: "Deny opens the refusal controls with an optional note.",
      },
      {
        target: ".chat-state-line",
        text: "The state line says the session needs a decision and names the tool that is waiting.",
      },
    ],
  },
];

export const CHAT_TIPS: readonly string[] = [
  "Enter sends your message, while Shift+Enter adds a new line.",
  "If you edit the draft while a message is sending, your edits stay in the draft when it is accepted.",
  "Stopping a turn returns its unsent queued messages to your draft.",
];
