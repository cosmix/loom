import { useAtomValue } from "jotai";
import { Provider } from "jotai/react";
import { useState } from "react";

import type { BackgroundTaskView } from "@/plan/api/schema-background";
import type { Attachment, Item, SessionView, Turn } from "@/plan/api/schema-chat";
import type { AttachmentDraft } from "@/plan/attachments";
import { BackgroundLine, useBackgroundDisclosure } from "@/plan/chat/background-line";
import { BackgroundList } from "@/plan/chat/background-list";
import { ComposerArea } from "@/plan/chat/composer";
import { draftAtom } from "@/plan/chat/drafts";
import { snapshotAtom } from "@/plan/chat/model";
import { holdNoteSeenAtom } from "@/plan/chat/prefs";
import { panelSessionAtom } from "@/plan/chat/session-home";
import { StateLine } from "@/plan/chat/state-line";
import { Transcript } from "@/plan/chat/transcript";
import type { PlannerHelpScene } from "@/plan/help/pinned-scene";
import { createSampleStore, PANEL_LOOK, SAMPLE_HOLDER } from "@/plan/help/sample";
import { SceneRouter } from "@/plan/help/scene-router";
import { backgroundAtom, connectionAtom } from "@/plan/state/atoms";

const STARTED_AT = "2026-10-29T10:00:00Z";
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
    fetch_grant: false,
    last_activity: STARTED_AT,
    failure: null,
    offer: null,
    feature: null,
    plan: null,
  };
}

function sampleStore(state: SessionView["state"]) {
  const store = createSampleStore();
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
        command: null,
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
        fetch: null,
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
        <div data-loom-plan inert className={PANEL_LOOK}>
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
        <div data-loom-plan inert className={`${PANEL_LOOK} space-y-3 p-3`}>
          <StateLine />
          <Transcript />
        </div>
      </SceneRouter>
    </Provider>
  );
}

const SPAWN_ID = "toolu_help_agent";

/** Two unended tasks in the shapes the server sends: an agent launched by the chat's Agent row, and a shell. */
function sampleTasks(): BackgroundTaskView[] {
  const ago = (minutes: number) => new Date(Date.now() - minutes * 60_000).toISOString();
  return [
    {
      id: "help-agent",
      kind: "agent",
      label: "Survey the release goals",
      agent_type: "general-purpose",
      tool_use_id: SPAWN_ID,
      state: "running",
      started_at: ago(4),
      last_activity_at: ago(0),
      steps: 12,
      last_tool: "Read",
      report_preview: null,
      model: "claude-sonnet-5-5",
      total: 48_200,
      tokens_per_min: 3_100,
      active: true,
    },
    {
      id: "help-shell",
      kind: "shell",
      label: "Run the link check",
      agent_type: null,
      tool_use_id: null,
      state: "running",
      started_at: ago(2),
      last_activity_at: ago(2),
      steps: null,
      last_tool: null,
      report_preview: null,
      model: null,
      total: null,
      tokens_per_min: null,
      active: null,
    },
  ];
}

function backgroundTurn(): Turn {
  return {
    ...sampleTurn(),
    items: [
      {
        type: "user",
        id: "help-ask",
        text: "Survey the release goals while I read the vision.",
        delivery: "delivered",
        origin: null,
        command: null,
        attachments: [],
      },
      {
        ...sampleTool(SPAWN_ID, "Agent", "Survey the release goals"),
        nested_tools: 12,
        input: JSON.stringify({ description: "Survey the release goals" }),
      },
    ],
  };
}

/**
 * The head of an idle chat that waits on background tasks. A scene is inert, so the list is drawn
 * open, as it looks once the reader has pressed the arrow.
 */
function BackgroundHead() {
  const session = useAtomValue(panelSessionAtom);
  const disclosure = useBackgroundDisclosure(session);
  const open = { ...disclosure, open: disclosure.waiting };
  return (
    <header className="chat-head">
      <div className="chat-head-text">
        <h2 className="chat-head-title">{session?.name}</h2>
        <div className="chat-head-sub">
          <BackgroundLine disclosure={open} />
        </div>
        <BackgroundList disclosure={open} />
      </div>
    </header>
  );
}

export function ChatBackgroundScene() {
  const [store] = useState(() => {
    const sample = sampleStore("idle");
    sample.set(snapshotAtom, (snapshot) => ({ ...snapshot, turns: [backgroundTurn()] }));
    sample.set(backgroundAtom, { session: "help-chat", tasks: sampleTasks() });
    return sample;
  });
  return (
    <Provider store={store}>
      <SceneRouter>
        <div data-loom-plan inert className={`${PANEL_LOOK} space-y-3 pb-3`}>
          <BackgroundHead />
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
    panel: true,
    notes: [
      {
        target: 'textarea[aria-label="Message"]',
        text: "Message holds the text you want to send with the attached passage.",
      },
      {
        target: '[aria-label="Pending attachments"] button:not([aria-label^="Remove"])',
        text: "The passage chip opens the document at the attached passage, and its remove button detaches it from your draft.",
      },
      {
        target: ".chat-composer-actions .chat-attach",
        text: "Attach adds another document to the draft, and pressing a on a sentence attaches that passage.",
      },
      {
        target: ".chat-composer-send button",
        text: "Send submits your text and attached passages to the session.",
      },
      {
        target: ".chat-composer-actions .chat-composer-stop button",
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
    panel: true,
    notes: [
      {
        target: '[aria-label="Chat transcript"] .chat-tool-fold > summary',
        text: "A run of tool calls folds into one line; open it to see each tool's input and result.",
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
        target: '[aria-label="Permission: Bash"] .chat-decision-actions > :first-child button',
        text: "Allow approves the requested tool action.",
      },
      {
        target: '[aria-label="Permission: Bash"] .chat-decision-actions > :last-child button',
        text: "Deny opens the refusal controls with an optional note.",
      },
      {
        target: ".chat-state-line",
        text: "The head's state line says the session needs your answer.",
      },
    ],
  },
  {
    title: "Background tasks",
    Scene: ChatBackgroundScene,
    panel: true,
    notes: [
      {
        target: ".chat-background-words",
        text: "An idle chat that waits on background agents or shells says so in its head.",
      },
      {
        target: ".chat-background-toggle",
        text: "The arrow opens and closes the list of those tasks.",
      },
      {
        target: ".chat-background-list",
        text: "Each task shows its state, name and running time, and an agent adds its model and tokens.",
      },
      {
        target: '[aria-label="Chat transcript"] .chat-tool-head',
        text: "The agent's row stays out of the tool fold while it runs and shows the same state word.",
      },
    ],
  },
];

export const CHAT_TIPS: readonly string[] = [
  "Enter sends your message and Shift+Enter adds a new line; edits made while it sends stay in the draft, and stopping a turn returns its unsent queued messages to it.",
  "An idle chat that waits on background agents or shells says so in its head; open the list to see each task.",
  "Typing / as the first character opens the command list: Up and Down move, Enter or Tab insert a command, Escape closes the list, and terminal-only commands are listed but disabled, each with its reason.",
];
