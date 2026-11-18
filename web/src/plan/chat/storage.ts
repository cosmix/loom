import { atom, getDefaultStore, type PrimitiveAtom } from "jotai/vanilla";
import { createJSONStorage } from "jotai/utils";

import { z } from "@/api/zod";

export const storageFullAtom: PrimitiveAtom<boolean> = atom(false);

const jsonStorage = createJSONStorage<unknown>(() => localStorage);

/** Quota errors differ by browser: DOMException name, legacy code 22, Firefox code 1014. */
function isQuotaError(error: unknown): boolean {
  return (
    error instanceof DOMException &&
    (error.name === "QuotaExceededError" ||
      error.name === "NS_ERROR_DOM_QUOTA_REACHED" ||
      error.code === 22 ||
      error.code === 1014)
  );
}

export const safeStorage = {
  getItem<Value>(key: string, initialValue: Value): Value {
    try {
      return jsonStorage.getItem(key, initialValue) as Value;
    } catch {
      return initialValue;
    }
  },
  setItem(key: string, value: unknown): void {
    try {
      jsonStorage.setItem(key, value);
    } catch (error) {
      // Only a full disk warns the user; blocked storage keeps the value in memory silently.
      if (isQuotaError(error)) getDefaultStore().set(storageFullAtom, true);
      return;
    }
    getDefaultStore().set(storageFullAtom, false);
  },
  removeItem(key: string): void {
    try {
      jsonStorage.removeItem(key);
    } catch {
      // Keep in-memory resets usable when browser storage is blocked.
    }
  },
  subscribe<Value>(key: string, callback: (value: Value) => void, initialValue: Value): () => void {
    try {
      return (
        jsonStorage.subscribe?.(key, (value) => callback(value as Value), initialValue) ??
        (() => {})
      );
    } catch {
      return () => {};
    }
  },
};

/** safeStorage whose reads, and storage-event updates, fall back to the initial value unless they parse. */
export function validatedStorage<Value>(schema: z.ZodType<Value>) {
  const check = (value: unknown, initialValue: Value): Value => {
    const result = schema.safeParse(value);
    return result.success ? result.data : initialValue;
  };
  return {
    ...safeStorage,
    getItem(key: string, initialValue: Value): Value {
      return check(safeStorage.getItem<unknown>(key, initialValue), initialValue);
    },
    subscribe(key: string, callback: (value: Value) => void, initialValue: Value): () => void {
      return safeStorage.subscribe<unknown>(
        key,
        (value) => callback(check(value, initialValue)),
        initialValue,
      );
    },
  };
}

export function storageKey(projectKey: string, name: string): string {
  return `loom-plan:${projectKey}:${name}`;
}
