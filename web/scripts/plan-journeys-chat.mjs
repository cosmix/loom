// J4 (the chat knows the page) and J5 (start a new feature) of web/scripts/plan-journeys.mjs.
//
// Selectors the chat-redesign stage must honour (chat-redesign.md, Panel behaviour):
//   context line   [aria-label^="Viewing "]            (reads "About section_05")
//   dismiss        button named "Dismiss context"
//   prompts        [aria-label="Suggested prompts"]    (buttons that fill the composer)
//   composer       aside[aria-label="Chat"] textarea   (placeholder names the document)
// None of them exists before that stage; J4 prints PENDING until the context line does.

import {
  NO_TMUX,
  Pending,
  ROOT,
  SECTION_5,
  blockedBy,
  chatEnded,
  chatFrames,
  clickSpineRow,
  fakeClaudePids,
  killSockets,
  newSockets,
  pathOf,
  planSockets,
  until,
  visit,
} from "./plan-journeys-kit.mjs";

// J4, J5 ----------------------------------------------------------------------------------------

const chat = (page) => page.locator('aside[aria-label="Chat"]');
const lineOf = (page) => page.locator('[aria-label^="Viewing "]').first();
const composerOf = (page) => chat(page).locator("textarea").first();

async function contextNow(page) {
  return {
    label: await lineOf(page).getAttribute("aria-label"),
    text: await lineOf(page).innerText(),
    placeholder: await composerOf(page).getAttribute("placeholder"),
  };
}

async function sendTyped(env, text) {
  const { page, frames } = env;
  await composerOf(page).fill(text);
  const send = chat(page).getByRole("button", { name: "Send", exact: true });
  const blocked = await blockedBy(page, send);
  if (blocked !== null) throw new Error(`Send is blocked: ${blocked}`);
  const before = chatFrames(frames).length;
  await send.click();
  await until("a chat command from Send", () => chatFrames(frames).length > before);
  return chatFrames(frames).at(-1);
}

async function j4Context(env) {
  const { page, origin, shot } = env;
  await visit(page, origin, `${ROOT}/product/section_05`);
  if ((await page.locator('[aria-label^="Viewing "]').count()) === 0) {
    throw new Pending('chat context line ([aria-label^="Viewing "]) not built yet (chat-redesign)');
  }
  await lineOf(page).waitFor();
  const here = await contextNow(page);
  if (![here.label, here.text, here.placeholder].every((s) => s?.includes("section_05"))) {
    throw new Error(
      `the context line or placeholder does not name section_05: ${JSON.stringify(here)}`,
    );
  }
  await shot(page, "context-section-05");
  if ((await clickSpineRow(page, (href) => href.includes("/metaplan"))) === false) {
    await visit(page, origin, `${ROOT}/metaplan`);
  }
  await page.waitForFunction(
    (was) => document.querySelector('[aria-label^="Viewing "]')?.getAttribute("aria-label") !== was,
    here.label,
  );
  const there = await contextNow(page);
  if (there.placeholder === here.placeholder)
    throw new Error("the placeholder did not change on the metaplan");
  await shot(page, "context-metaplan");
}

async function j4Prompts(env) {
  const { page, origin, frames, shot } = env;
  await visit(page, origin, `${ROOT}/product/section_05`);
  const sockets = planSockets();
  const sent = chatFrames(frames).length;
  const prompt = page.locator('[aria-label="Suggested prompts"] button').first();
  if ((await prompt.count()) === 0) throw new Error("no Suggested prompts on section_05");
  await prompt.click();
  if ((await composerOf(page).inputValue()) === "")
    throw new Error("a prompt did not fill the composer");
  if (chatFrames(frames).length !== sent || newSockets(sockets).length) {
    throw new Error("clicking a prompt started a session");
  }
  await shot(page, "prompt-filled");
}

/** The session is up when tmux has a new socket and the fake claude runs more than before. */
async function sessionUp(before, claudes) {
  await until("a new tmux socket for the session", () => newSockets(before).length > 0, 20_000);
  await until("the fake claude to start", () => fakeClaudePids().length > claudes, 20_000);
}

async function j4Send(env, before, claudes) {
  const first = await sendTyped(env, "Journey check: which sections does this touch?");
  const paths = (first.attachments ?? []).map((a) => a.path);
  if (!paths.includes(SECTION_5)) {
    throw new Error(
      `the first message carried ${paths.join(", ") || "no attachment"}, not ${SECTION_5}`,
    );
  }
  if (!env.tmuxOk) return;
  await sessionUp(before, claudes);
  await env.shot(env.page, "session-started");
}

async function j4Dismiss(env) {
  const { page, shot } = env;
  await clickSpineRow(page, (href) => href.endsWith("/product/section_06"));
  await lineOf(page).waitFor();
  await page.getByRole("button", { name: "Dismiss context" }).click();
  const second = await sendTyped(env, "Journey check: and this one?");
  if ((second.attachments ?? []).length) {
    throw new Error("a dismissed context still sent an attachment");
  }
  await shot(page, "dismissed-sent");
}

export async function j4(env) {
  await j4Context(env);
  await j4Prompts(env);
  const before = planSockets();
  const claudes = fakeClaudePids().length;
  const sent = chatFrames(env.frames).length;
  try {
    await j4Send(env, before, claudes);
    if (!env.tmuxOk) {
      throw new Pending(
        `${NO_TMUX}; the context, prompt and attachment steps passed, the session and dismissal steps did not run`,
      );
    }
    await j4Dismiss(env);
  } finally {
    // J5 starts its own chat, which a running one would refuse, so end this one: kill its tmux.
    // Without tmux no session started, and an unscoped scan would reach the operator's sockets.
    if (env.tmuxOk) killSockets(newSockets(before));
    await until("the fake claude to exit", () => fakeClaudePids().length <= claudes).catch(
      () => {},
    );
    // /new refuses a start until the server reports this chat over; a dead tmux is not enough.
    // A chat that never ends must not replace the Pending or the real error thrown above: J5's
    // own refused start reports it.
    if (chatFrames(env.frames).length > sent) {
      await until("the server to end the chat", chatEnded, 20_000).catch(() => {});
    }
  }
  return "context, prompts, attachment and dismissal";
}

export async function j5(env) {
  const { page, origin, frames, shot } = env;
  await visit(page, origin, "/");
  if (pathOf(page.url()) !== "/")
    await page.getByRole("link", { name: "Loom planner home" }).click();
  const start = page.getByRole("link", { name: "Start a conversation" });
  if ((await start.count()) === 0)
    throw new Error("home has no Start a conversation link (authoring blocked?)");
  await start.click();
  await page.waitForURL((u) => u.pathname === "/new");
  const composer = page.getByRole("textbox", { name: /What do you want to build/ });
  await composer.focus();
  if (!(await composer.evaluate((el) => el === document.activeElement)))
    throw new Error("/new composer is not focusable");
  await composer.fill("Journey check: a feature that lists my reading.");
  await shot(page, "new-typed");
  const before = planSockets();
  const claudes = fakeClaudePids().length;
  const sent = chatFrames(frames).length;
  await page.getByRole("button", { name: /^Start( the conversation)?$/ }).click();
  await until("a chat start command", () => chatFrames(frames).length > sent);
  if (!env.tmuxOk) throw new Pending(`${NO_TMUX}; the composer and start command passed`);
  await sessionUp(before, claudes);
  await shot(page, "session-started");
}
