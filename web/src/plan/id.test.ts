import { afterEach, beforeEach, expect, it, vi } from "vitest";

import { newId } from "./id";

const UUID_V4 = /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/;

let saved: PropertyDescriptor | undefined;

beforeEach(() => {
  saved = Object.getOwnPropertyDescriptor(crypto, "randomUUID");
});

afterEach(() => {
  vi.restoreAllMocks();
  if (saved) Object.defineProperty(crypto, "randomUUID", saved);
  else Reflect.deleteProperty(crypto, "randomUUID");
});

it("falls back to a v4 UUID when randomUUID is missing", () => {
  Object.defineProperty(crypto, "randomUUID", { configurable: true, value: undefined });
  const first = newId();
  const second = newId();
  expect(first).toMatch(UUID_V4);
  expect(second).toMatch(UUID_V4);
  expect(first).not.toBe(second);
});

it("returns crypto.randomUUID when it is present", () => {
  const spy = vi
    .spyOn(crypto, "randomUUID")
    .mockReturnValue("00000000-0000-4000-8000-000000000001");
  expect(newId()).toBe("00000000-0000-4000-8000-000000000001");
  expect(spy).toHaveBeenCalledTimes(1);
});
