let codeMirror: Promise<typeof import("./codemirror")> | undefined;

/** One import for the page's lifetime; a failed import (network, stale chunk) is retried. */
export function loadCodeMirror(): Promise<typeof import("./codemirror")> {
  codeMirror ??= import("./codemirror").catch((error: unknown) => {
    codeMirror = undefined;
    throw error;
  });
  return codeMirror;
}
