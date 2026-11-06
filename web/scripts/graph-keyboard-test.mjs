// Run with: bun scripts/graph-keyboard-test.mjs
// Uses an installed Chrome and a stub status socket that delivers the fixture to exercise native key activation.
import assert from "node:assert/strict";
import { readFile } from "node:fs/promises";
import { chromium } from "playwright-core";
import { createServer } from "vite";

const snapshot = JSON.parse(
  await readFile(new URL("../src/api/fixtures/snapshot.json", import.meta.url)),
);
snapshot.terminals = true;
const stage = snapshot.status.stages.find((item) => item.id === "client");
assert.ok(stage);
stage.session_alive = true;
stage.session_backend = "tmux";

const server = await createServer({ server: { host: "127.0.0.1", port: 0 } });
await server.listen();
const address = server.httpServer.address();
const browser = await chromium.launch({
  executablePath: process.env.CHROME_PATH ?? "/usr/bin/google-chrome",
  args: ["--no-sandbox"],
});
try {
  const page = await browser.newPage();
  await page.addInitScript((snapshot) => {
    // The stub socket opens and delivers one `full` message, as the server does.
    window.WebSocket = class extends EventTarget {
      constructor() {
        super();
        setTimeout(() => {
          this.onopen?.();
          this.onmessage?.({ data: JSON.stringify({ t: "full", snapshot }) });
        }, 0);
      }
      close() {}
      send() {}
    };
    window.navigationCount = 0;
    const push = history.pushState.bind(history);
    history.pushState = (...args) => {
      window.navigationCount++;
      return push(...args);
    };
  }, snapshot);
  const url = `http://127.0.0.1:${address.port}/`;
  await page.goto(url);
  const wrapper = page.locator(`.react-flow__node-stage[data-id="${stage.id}"]`);
  const terminal = wrapper.getByRole("button", { name: `open terminal for ${stage.name}` });
  await terminal.focus();
  await page.keyboard.press("Enter");
  await page.waitForURL(`**/?stage=${stage.id}&view=terminal`);
  assert.equal(await page.evaluate(() => window.navigationCount), 1);

  await page.goto(url);
  await wrapper.focus();
  await page.keyboard.press("Enter");
  await page.waitForURL(`**/?stage=${stage.id}`);
  assert.equal(await page.evaluate(() => window.navigationCount), 1);

  await page.goto(url);
  await wrapper.getByRole("button", { name: `open ${stage.name}` }).focus();
  await page.keyboard.press("Enter");
  await page.waitForURL(`**/?stage=${stage.id}`);
  assert.equal(await page.evaluate(() => window.navigationCount), 1);

  await page.goto(url);
  const key = page
    .getByRole("list", { name: "stage states in this plan" })
    .getByRole("button")
    .first();
  await key.focus();
  await page.keyboard.press("Space");
  assert.equal(await key.getAttribute("aria-pressed"), "true");
  await page.keyboard.press("Escape");
  assert.equal(await key.getAttribute("aria-pressed"), "false");
  console.log("graph keyboard activation passed");
} finally {
  await browser.close();
  await server.close();
}
