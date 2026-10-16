import { act, cleanup, fireEvent, screen, waitFor } from "@testing-library/react";
import { afterEach, expect, it } from "vitest";
import type { ConfigClient, ConfigWrite, WriteResult } from "@/api/config";
import { applyWrite, renderAt, row, snapshot } from "@/test/settings-kit";

afterEach(cleanup);

it.each([false, true])(
  "refreshes concurrent enabled scope cells after reversed responses (reverse persistence=%s)",
  async (reversePersistence) => {
    const persisted = snapshot();
    const pending: {
      request: ConfigWrite;
      resolve: (value: WriteResult) => void;
      result?: WriteResult;
    }[] = [];
    const client: ConfigClient = {
      load: async () => structuredClone(persisted),
      write: async (_csrf, request) => new Promise((resolve) => pending.push({ request, resolve })),
    };
    renderAt("?settings=1", client);
    const user = await screen.findByRole("combobox", { name: "standard_model at user scope" });
    fireEvent.change(user, { target: { value: "haiku" } });
    expect(user).toHaveProperty("disabled", true);
    const project = screen.getByRole("combobox", { name: "standard_model at project scope" });
    expect(project).not.toHaveProperty("disabled", true);
    fireEvent.change(project, { target: { value: "opus" } });
    expect(project).toHaveProperty("disabled", true);
    expect(pending).toHaveLength(2);
    for (const index of reversePersistence ? [1, 0] : [0, 1]) {
      pending[index].result = {
        ok: true,
        entry: structuredClone(applyWrite(persisted, pending[index].request)),
      };
    }
    await act(async () => {
      pending[1].resolve(pending[1].result!);
    });
    await act(async () => {
      pending[0].resolve(pending[0].result!);
    });
    await waitFor(() =>
      expect(
        screen.getByRole("combobox", {
          name: "standard_model at user scope",
        }),
      ).toHaveProperty("value", "haiku"),
    );
    expect(
      screen.getByRole("combobox", { name: "standard_model at project scope" }),
    ).toHaveProperty("value", "opus");
    expect(row("standard").querySelector(".settings-res")?.textContent).toContain("runs opus");
  },
);

it("discards a refresh overtaken by another write in an independently enabled cell", async () => {
  const persisted = snapshot();
  const pending: { request: ConfigWrite; resolve: (value: WriteResult) => void }[] = [];
  const refreshes: (() => void)[] = [];
  let loads = 0;
  const client: ConfigClient = {
    load: async () => {
      const captured = structuredClone(persisted);
      if (loads++ === 0) return captured;
      return new Promise((resolve) => refreshes.push(() => resolve(captured)));
    },
    write: async (_csrf, request) => new Promise((resolve) => pending.push({ request, resolve })),
  };
  renderAt("?settings=1", client);
  const user = await screen.findByRole("combobox", { name: "standard_model at user scope" });
  const project = screen.getByRole("combobox", { name: "standard_model at project scope" });
  fireEvent.change(user, { target: { value: "haiku" } });
  fireEvent.change(project, { target: { value: "opus" } });
  const finish = async (index: number) => {
    await act(async () =>
      pending[index].resolve({
        ok: true,
        entry: structuredClone(applyWrite(persisted, pending[index].request)),
      }),
    );
  };
  await finish(1);
  await finish(0);
  expect(refreshes).toHaveLength(1);
  expect(user).toHaveProperty("disabled", true);
  expect(project).toHaveProperty("disabled", false);
  fireEvent.change(project, { target: { value: "fable" } });
  await finish(2);
  expect(refreshes).toHaveLength(2);
  await act(async () => refreshes[1]());
  await act(async () => refreshes[0]());
  await waitFor(() => expect(user).toHaveProperty("value", "haiku"));
  expect(project).toHaveProperty("value", "fable");
  expect(row("standard").querySelector(".settings-res")?.textContent).toContain("runs fable");
  expect(
    persisted.entries.find((entry) => entry.name === "models.standard_model")?.effective.value,
  ).toBe("fable");
});
