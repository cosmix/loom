// A plain style spec (no CodeMirror import): codemirror.ts wraps it with EditorView.theme.
// Every colour is a design-token variable, so `.dark` on <html> switches it with no second
// palette. CodeMirror's own light and dark base rules still apply underneath (codemirror.ts
// selects them from the page's colour scheme); the rules below override what they colour.
export const themeSpec = {
  "&": {
    color: "var(--foreground)",
    backgroundColor: "var(--background)",
    fontFamily: "var(--font-mono)",
  },
  // The base theme sets `monospace` on the scroller, which wins over the value on `&`.
  ".cm-scroller": {
    fontFamily: "var(--font-mono)",
  },
  // The base theme draws a dotted near-black outline, which disappears on a dark page.
  "&.cm-focused": {
    outline: "1px solid var(--ring)",
  },
  // Without drawSelection the browser draws the caret and the selection itself.
  ".cm-content": {
    caretColor: "var(--foreground)",
  },
  // --highlight is near-black in the light themes, so the text takes its paired colour.
  ".cm-content ::selection": {
    backgroundColor: "var(--highlight)",
    color: "var(--highlight-foreground)",
  },
  ".cm-gutters": {
    color: "var(--muted-foreground)",
    backgroundColor: "var(--muted)",
    borderRightColor: "var(--border)",
  },
  ".cm-panels": {
    color: "var(--foreground)",
    backgroundColor: "var(--muted)",
    borderColor: "var(--border)",
  },
  ".cm-panels-top": {
    borderBottomColor: "var(--border)",
  },
  ".cm-panels-bottom": {
    borderTopColor: "var(--border)",
  },
  // The search panel's controls: the base theme gives them a light gradient, a white field,
  // 70% type and the monospace face; controls take the body face at 12px.
  ".cm-button, .cm-button:active": {
    backgroundImage: "none",
    backgroundColor: "var(--secondary)",
    color: "var(--secondary-foreground)",
    borderColor: "var(--border)",
    fontSize: "12px",
    fontFamily: "var(--font-sans)",
  },
  ".cm-button:hover, .cm-button:active": {
    backgroundColor: "var(--accent)",
  },
  ".cm-textfield": {
    backgroundColor: "var(--background)",
    color: "var(--foreground)",
    borderColor: "var(--input)",
    fontSize: "12px",
    fontFamily: "var(--font-sans)",
  },
  ".cm-searchMatch": {
    backgroundColor: "var(--primary)",
    color: "var(--primary-foreground)",
  },
  // Validation marks: the dot in the gutter and a faint tint on the gutter cell. The tint is
  // a mix, not a bare variable, so it uses `background` and stays out of the colour check.
  ".cm-mark-gutter .cm-gutterElement": {
    padding: "0 4px",
  },
  ".cm-line-mark": {
    color: "var(--destructive)",
    fontSize: "0.75em",
  },
  ".cm-marked-line": {
    background: "color-mix(in oklch, var(--destructive) 14%, transparent)",
  },
};
