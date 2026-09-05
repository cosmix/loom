import { createStore } from "jotai/vanilla";

export const store = createStore();
export type Store = ReturnType<typeof createStore>;
