import { act, fireEvent, screen, within } from "@testing-library/react";
import { expect, it, vi } from "vitest";

import { card, harness, payload, permission, useFakeClock } from "./permission-card-test-kit";

useFakeClock();

it("bash body", () => {
  const command = "printf 'hello'\nls doc/";
  harness().show(permission({ input: JSON.stringify({ command }) }));

  expect(screen.getByText(/printf/).closest("pre")?.textContent).toBe(command);
  expect(screen.getByText(/printf/).closest("pre")?.className).toContain("font-mono");
});

it("edit body diff", () => {
  harness().show(
    permission({
      tool: "Edit",
      input: JSON.stringify({
        file_path: "doc/file.md",
        old_string: "old",
        new_string: "new",
      }),
    }),
  );

  expect(screen.getByText("doc/file.md")).toBeTruthy();
  expect(screen.getByText("+new").closest("pre")?.textContent).toBe(
    "--- a/doc/file.md\n+++ b/doc/file.md\n@@ -1,1 +1,1 @@\n-old\n+new",
  );
});

it("write body diff", () => {
  harness().show(
    permission({
      tool: "Write",
      input: JSON.stringify({ file_path: "doc/new.md", content: "new" }),
    }),
  );

  expect(screen.getByText("+new").closest("pre")?.textContent).toBe(
    "--- a/doc/new.md\n+++ b/doc/new.md\n@@ -0,0 +1,1 @@\n+new",
  );
});

it("other tool json", () => {
  harness().show(permission({ tool: "Search", input: '{"query":"needle","limit":2}' }));

  expect(screen.getByText(/needle/).closest("pre")?.textContent).toBe(
    '{\n  "query": "needle",\n  "limit": 2\n}',
  );
});

it("time left from expires_at", () => {
  harness().show(permission({ expires_at: "2026-10-28T10:02:30Z" }));
  expect(screen.getByText("2m 30s left before automatic deny")).toBeTruthy();

  act(() => vi.advanceTimersByTime(1000));
  expect(screen.getByText("2m 29s left before automatic deny")).toBeTruthy();
});

it("deny with note sends note", async () => {
  const h = harness();
  h.show();
  fireEvent.click(screen.getByRole("button", { name: "Deny" }));
  fireEvent.change(screen.getByRole("textbox", { name: "Deny note" }), {
    target: { value: "Please leave it" },
  });
  fireEvent.click(screen.getByRole("button", { name: "Deny with note" }));

  expect(h.send).toHaveBeenCalledWith(
    expect.objectContaining({ ...payload, allow: false, note: "Please leave it" }),
  );
  await act(async () => h.reply());
});

it("two buttons only", () => {
  harness().show();

  expect(
    within(card())
      .getAllByRole("button")
      .map((button) => button.textContent),
  ).toEqual(["Allow", "Deny"]);
});

it("edit replace_all is shown and the diff ignores patch and diff fields", () => {
  harness().show(
    permission({
      tool: "Edit",
      input: JSON.stringify({
        file_path: "doc/file.md",
        old_string: "must",
        new_string: "must not",
        replace_all: true,
        patch: "--- a/other\n+++ b/other\n@@ -1,1 +1,1 @@\n-x\n+y",
        diff: "+hidden",
      }),
    }),
  );

  expect(screen.getByText("Replaces every occurrence")).toBeTruthy();
  expect(screen.getByText("+must not").closest("pre")?.textContent).toBe(
    "--- a/doc/file.md\n+++ b/doc/file.md\n@@ -1,1 +1,1 @@\n-must\n+must not",
  );
  expect(screen.queryByText("+y")).toBeNull();
  const extras = screen.getByText(/"diff"/).closest("pre");
  expect(extras?.textContent).toContain('"patch"');
  expect(extras?.textContent).toContain('"diff": "+hidden"');
  expect(extras?.textContent).not.toContain("replace_all");
});

it("edit without replace_all shows no replace line and no extras", () => {
  harness().show(
    permission({
      tool: "Edit",
      input: JSON.stringify({
        file_path: "doc/file.md",
        old_string: "a",
        new_string: "b",
        replace_all: false,
      }),
    }),
  );

  expect(screen.queryByText("Replaces every occurrence")).toBeNull();
  expect(screen.queryByText("Other input fields")).toBeNull();
});

it("unknown write fields are shown as indented json", () => {
  harness().show(
    permission({
      tool: "Write",
      input: JSON.stringify({ file_path: "doc/new.md", content: "new", mode: "0777" }),
    }),
  );

  expect(screen.getByText("Other input fields")).toBeTruthy();
  expect(screen.getByText(/"mode"/).closest("pre")?.textContent).toBe('{\n  "mode": "0777"\n}');
});

it("input renders as text", () => {
  const { container } = harness().show(
    permission({ input: '{"command":"<img src=x onerror=alert(1)>"}' }),
  );

  expect(screen.getByText("<img src=x onerror=alert(1)>")).toBeTruthy();
  expect(container.querySelector("img, script")).toBeNull();
});

it("malformed input remains visible", () => {
  harness().show(permission({ input: '{"command": "unfinished' }));

  expect(screen.getByText('{"command": "unfinished')).toBeTruthy();
});

it("command, path, diff and json show visible controls", () => {
  const h = harness();
  const { container, unmount } = h.show(
    permission({ input: '{"command":"ls doc/\u202efile\u200b"}' }),
  );
  expect(container.textContent).toContain("ls doc/⟨U+202E⟩file⟨U+200B⟩");
  expect(screen.getByText(/ls doc/).style.unicodeBidi).toBe("isolate");
  expect(screen.getByText(/ls doc/).style.direction).toBe("ltr");
  expect(container.textContent).not.toMatch(/\p{Cf}/u);
  unmount();
  const edit = h.show(
    permission({
      tool: "Edit",
      input: JSON.stringify({
        file_path: "doc/\u202efile",
        old_string: "old\u200b",
        new_string: "<b>new</b>\u2066",
      }),
    }),
  );
  expect(edit.container.textContent).toContain("doc/⟨U+202E⟩file");
  expect(edit.container.textContent).toContain("+<b>new</b>⟨U+2066⟩");
  expect(edit.container.querySelector("b")).toBeNull();
  expect(edit.container.textContent).not.toMatch(/\p{Cf}/u);
  edit.unmount();
  const other = h.show(permission({ tool: "Search", input: '{"query":"a\u200bb"}' }));
  expect(other.container.textContent).toContain("a⟨U+200B⟩b");
  expect(other.container.textContent).not.toMatch(/\p{Cf}/u);
});
