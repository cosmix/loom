let codeMirror: Promise<typeof import("./codemirror")> | undefined;

export function loadCodeMirror(): Promise<typeof import("./codemirror")> {
  return (codeMirror ??= import("./codemirror"));
}
