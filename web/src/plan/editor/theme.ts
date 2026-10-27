export const themeSpec = {
  "&": {
    color: "var(--foreground)",
    backgroundColor: "var(--background)",
    fontFamily: "var(--font-mono)",
  },
  ".cm-gutters": {
    color: "var(--muted-foreground)",
    backgroundColor: "var(--muted)",
    borderRightColor: "var(--border)",
  },
  ".cm-activeLine, .cm-activeLineGutter": {
    backgroundColor: "var(--muted)",
  },
  ".cm-cursor, .cm-dropCursor": {
    borderLeftColor: "var(--foreground)",
  },
  "&.cm-focused .cm-selectionBackground, .cm-selectionBackground, .cm-content ::selection": {
    backgroundColor: "var(--accent)",
  },
  ".cm-panels": {
    color: "var(--foreground)",
    backgroundColor: "var(--muted)",
    borderColor: "var(--border)",
  },
  ".cm-searchMatch": {
    backgroundColor: "var(--primary)",
    color: "var(--primary-foreground)",
  },
};
